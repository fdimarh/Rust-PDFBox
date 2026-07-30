//! High-level PdfEditor — typed content-stream editing at the document level.
//!
//! # Design
//!
//! [`PdfEditor`] wraps a [`Document`] and exposes per-page content-stream
//! manipulation through the strongly-typed [`ContentOperator`] API.
//! All operations work on decoded stream bytes so they are independent of
//! whatever compression filter the PDF uses.
//!
//! # WASM compatibility
//!
//! - No platform-specific I/O (load from / save to `Vec<u8>`).
//! - All public types implement `Send` (no `Rc` / `RefCell`).
//! - The editor is usable as a state machine: load → edit page(s) → save.

use crate::content::edit::{ContentOperator, parse_content_operators, serialise_content_stream};
use crate::cos::{CosDictionary, CosName, CosObject, CosStream, ObjectId};
use crate::io::decode_stream;
use crate::pageops::{PdfMerger, PdfSplitter, extract_pages, rotate_page as rotate_page_fn};
use crate::{Document, PdfError, PdfResult};

/// High-level content-stream editor for PDF documents.
pub struct PdfEditor {
    doc: Document,
}

// ── helpers ────────────────────────────────────────────────────────────

/// Walk pages tree and collect page ObjectIds.
fn collect_page_ids(node_ref: ObjectId, store: &crate::ObjectStore, pages: &mut Vec<ObjectId>) {
    let Some(node) = store.get(&node_ref) else {
        return;
    };
    let Some(dict) = node.as_dictionary() else {
        return;
    };

    let type_name = dict
        .get(&CosName::type_name())
        .and_then(|o| o.as_name())
        .map(|n| n.as_bytes())
        .unwrap_or(b"");

    if type_name == b"Pages" {
        let kids_arr = dict
            .get(&CosName::new(b"Kids".to_vec()))
            .and_then(|o| store.resolve(o))
            .and_then(|o| o.as_array());
        if let Some(kids) = kids_arr {
            for kid in kids {
                if let Some(ref_id) = kid.as_reference() {
                    collect_page_ids(ref_id, store, pages);
                }
            }
        }
    } else {
        // Page or unrecognised leaf
        pages.push(node_ref);
    }
}

impl PdfEditor {
    /// Create a new editor, taking ownership of the document.
    pub fn new(doc: Document) -> Self {
        Self { doc }
    }

    /// Consume the editor and return the underlying `Document`.
    pub fn into_document(self) -> Document {
        self.doc
    }

    /// Borrow the underlying `Document`.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Mutable access to the underlying `Document`.
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.doc
    }

    // ── Page enumeration ────────────────────────────────────────────────

    /// Number of pages in the document.
    pub fn page_count(&self) -> usize {
        self.page_ids().len()
    }

    /// Returns the object IDs of every page in order.
    fn page_ids(&self) -> Vec<ObjectId> {
        let catalog = match self.doc.catalog() {
            Some(c) => c,
            None => return Vec::new(),
        };
        let pages_ref = match catalog
            .get(&CosName::pages())
            .and_then(|v| v.as_reference())
        {
            Some(r) => r,
            None => return Vec::new(),
        };
        let mut pages = Vec::new();
        collect_page_ids(pages_ref, &self.doc.objects, &mut pages);
        pages
    }

    /// Resolve page ObjectId by index.
    fn page_id(&self, index: usize) -> PdfResult<ObjectId> {
        let ids = self.page_ids();
        ids.get(index).copied().ok_or_else(|| PdfError::Parse {
            offset: None,
            context: format!("page index {index} out of range (count={})", ids.len()),
        })
    }

    // ── Content stream access ────────────────────────────────────────────

    /// Parse and return the typed content operators for the given page.
    pub fn get_content_operators(&self, page_index: usize) -> PdfResult<Vec<ContentOperator>> {
        let raw = self.page_raw_content_bytes(page_index)?;
        parse_content_operators(&raw).map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("content parse error: {e}"),
        })
    }

    /// Replace the content stream of a page with newly serialised operators.
    pub fn set_content_operators(
        &mut self,
        page_index: usize,
        ops: &[ContentOperator],
    ) -> PdfResult<()> {
        let bytes = serialise_content_stream(ops);
        self.replace_page_content_stream(page_index, bytes)
    }

    // ── Convenience: text find & replace ────────────────────────────────

    /// Find all text occurrences matching `needle` across all pages.
    pub fn find_text_all_pages(&self, needle: &str) -> Vec<(usize, usize)> {
        let n = self.page_count();
        let mut results = Vec::new();
        for page_idx in 0..n {
            if let Ok(ops) = self.get_content_operators(page_idx) {
                for (op_idx, op) in ops.iter().enumerate() {
                    let found = match op {
                        ContentOperator::ShowText(s) => std::str::from_utf8(s)
                            .ok()
                            .map_or(false, |t| t.contains(needle)),
                        ContentOperator::ShowTextPositioned(items) => items.iter().any(|item| {
                            if let crate::content::edit::TjItem::Text(s) = item {
                                std::str::from_utf8(s)
                                    .ok()
                                    .map_or(false, |t| t.contains(needle))
                            } else {
                                false
                            }
                        }),
                        _ => false,
                    };
                    if found {
                        results.push((page_idx, op_idx));
                    }
                }
            }
        }
        results
    }

    /// Replace `old` with `new` in all `ShowText` / `TJ` operators on a page.
    pub fn replace_text_on_page(
        &mut self,
        page_index: usize,
        old: &str,
        new: &str,
    ) -> PdfResult<usize> {
        let mut ops = self.get_content_operators(page_index)?;
        let mut count = 0;
        for i in 0..ops.len() {
            if crate::content::edit::replace_show_text(&mut ops, i, old, new) {
                count += 1;
            }
        }
        if count > 0 {
            self.set_content_operators(page_index, &ops)?;
        }
        Ok(count)
    }

    // ── XObject rename ─────────────────────────────────────────────────

    /// Rename all `Do` references from `old_name` to `new_name` on a page.
    pub fn rename_xobject_on_page(
        &mut self,
        page_index: usize,
        old_name: &str,
        new_name: &str,
    ) -> PdfResult<usize> {
        let mut ops = self.get_content_operators(page_index)?;
        let count = crate::content::edit::rename_xobject_operator(&mut ops, old_name, new_name);
        if count > 0 {
            self.set_content_operators(page_index, &ops)?;
        }
        Ok(count)
    }

    // ── Image XObject replacement ──────────────────────────────────────

    /// Replace the raw data and metadata of an image XObject in a page's
    /// resource dictionary.
    pub fn replace_image_xobject(
        &mut self,
        page_index: usize,
        resource_name: &str,
        new_data: &[u8],
        new_width: u32,
        new_height: u32,
        new_color_space: &str,
        new_bits_per_component: u8,
        new_filter: Option<&str>,
    ) -> PdfResult<()> {
        let id = self.page_id(page_index)?;

        // Step 1: find the stream's ObjectId (immutable borrow of objects)
        let stream_id = {
            let page_dict = self
                .doc
                .objects
                .get(&id)
                .and_then(|o| o.as_dictionary())
                .ok_or_else(|| PdfError::Parse {
                    offset: None,
                    context: format!("page {page_index} dictionary not found"),
                })?;

            let resources = page_dict
                .get(&CosName::resources())
                .and_then(|v| v.as_dictionary())
                .ok_or_else(|| PdfError::Parse {
                    offset: None,
                    context: format!("page {page_index} has no /Resources"),
                })?;

            let xobject_dict = resources
                .get(&CosName::new(b"XObject".to_vec()))
                .and_then(|v| v.as_dictionary())
                .ok_or_else(|| PdfError::Parse {
                    offset: None,
                    context: format!("page {page_index} has no /XObject"),
                })?;

            let name_key = CosName::new(resource_name.as_bytes().to_vec());
            let entry = xobject_dict.get(&name_key).ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} has no XObject named '{resource_name}'"),
            })?;

            match entry {
                CosObject::Reference(rid) => Some(*rid),
                CosObject::Stream(_) => None,
                _ => {
                    return Err(PdfError::Parse {
                        offset: None,
                        context: format!(
                            "XObject '{resource_name}' is neither stream nor reference"
                        ),
                    });
                }
            }
        };

        // Step 2: mutable access through the single stream
        let stream_value = match stream_id {
            Some(rid) => {
                let obj = self
                    .doc
                    .objects
                    .get_mut(&rid)
                    .ok_or_else(|| PdfError::Parse {
                        offset: None,
                        context: format!("reference {rid} not found for XObject '{resource_name}'"),
                    })?;
                match obj {
                    CosObject::Stream(s) => s,
                    _ => {
                        return Err(PdfError::Parse {
                            offset: None,
                            context: format!(
                                "XObject '{resource_name}' ref resolves to non-stream"
                            ),
                        });
                    }
                }
            }
            None => {
                return Err(PdfError::Parse {
                    offset: None,
                    context: format!("XObject '{resource_name}' is inline; not yet supported"),
                });
            }
        };

        // Step 3: update data + metadata
        stream_value.data = new_data.to_vec();
        let d = &mut stream_value.dictionary;

        d.insert(
            CosName::new(b"Type".to_vec()),
            CosObject::Name(CosName::new(b"XObject".to_vec())),
        );
        d.insert(
            CosName::new(b"Subtype".to_vec()),
            CosObject::Name(CosName::new(b"Image".to_vec())),
        );
        d.insert(
            CosName::new(b"Width".to_vec()),
            CosObject::Integer(new_width as i64),
        );
        d.insert(
            CosName::new(b"Height".to_vec()),
            CosObject::Integer(new_height as i64),
        );
        d.insert(
            CosName::new(b"ColorSpace".to_vec()),
            CosObject::Name(CosName::new(new_color_space.as_bytes().to_vec())),
        );
        d.insert(
            CosName::new(b"BitsPerComponent".to_vec()),
            CosObject::Integer(new_bits_per_component as i64),
        );

        let filter_key = CosName::new(b"Filter".to_vec());
        d.remove(&filter_key);
        if let Some(filter) = new_filter {
            d.insert(
                filter_key,
                CosObject::Name(CosName::new(filter.as_bytes().to_vec())),
            );
        }

        Ok(())
    }

    // ── Save ────────────────────────────────────────────────────────────

    /// Serialize the document to a byte vector.
    pub fn save_to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.doc.save_to(&mut std::io::Cursor::new(&mut buf))?;
        Ok(buf)
    }

    /// Save the document to a file path.
    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        self.doc.save(path)
    }

    // ── Text extraction ─────────────────────────────────────────────────

    /// Extract all text from a single page as a plain string.
    pub fn extract_text_from_page(&self, page_index: usize) -> PdfResult<String> {
        let ops = self.get_content_operators(page_index)?;
        let mut text = String::new();
        for op in &ops {
            match op {
                ContentOperator::ShowText(s) => {
                    if let Ok(t) = std::str::from_utf8(s) {
                        text.push_str(t);
                    }
                }
                ContentOperator::ShowTextPositioned(items) => {
                    for item in items {
                        if let crate::content::edit::TjItem::Text(s) = item {
                            if let Ok(t) = std::str::from_utf8(s) {
                                text.push_str(t);
                            }
                        }
                    }
                }
                ContentOperator::MoveNextLineShowText(s) => {
                    text.push('\n');
                    if let Ok(t) = std::str::from_utf8(s) {
                        text.push_str(t);
                    }
                }
                ContentOperator::SetSpacingMoveNextLineShowText(_, _, s) => {
                    text.push('\n');
                    if let Ok(t) = std::str::from_utf8(s) {
                        text.push_str(t);
                    }
                }
                _ => {}
            }
        }
        Ok(text)
    }

    /// Extract text from all pages. Each element is text from one page.
    pub fn extract_text_all_pages(&self) -> Vec<(usize, String)> {
        let n = self.page_count();
        let mut results = Vec::new();
        for i in 0..n {
            if let Ok(text) = self.extract_text_from_page(i) {
                if !text.is_empty() {
                    results.push((i, text));
                }
            }
        }
        results
    }

    // ── Image enumeration & replacement ─────────────────────────────────
    /// Find image XObjects referenced in a page's Resources dictionary.
    pub fn find_images_on_page(
        &self,
        page_index: usize,
    ) -> PdfResult<Vec<crate::content::edit::XObjectImage>> {
        let id = self.page_id(page_index)?;
        let page_dict = self
            .doc
            .objects
            .get(&id)
            .and_then(|o| o.as_dictionary())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} dictionary not found"),
            })?;
        let resources = page_dict
            .get(&CosName::resources())
            .and_then(|v| v.as_dictionary())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} has no /Resources"),
            })?;
        // Resolve references manually since XObjectImage::from_stream doesn't have store access
        let xobj_dict_val = resources.get(&CosName::new(b"XObject".to_vec()));
        let Some(xobj_dict) = xobj_dict_val.and_then(|v| {
            match v {
                CosObject::Dictionary(d) => Some(d),
                CosObject::Reference(rid) => self.doc.objects.get(rid).and_then(|o| o.as_dictionary()),
                _ => None,
            }
        }) else {
            return Ok(Vec::new());
        };

        let mut images = Vec::new();
        for (name, obj) in xobj_dict.iter() {
            let resolved = match obj {
                CosObject::Reference(rid) => self.doc.objects.get(rid),
                other => Some(other),
            };
            if let Some(stream) = resolved.and_then(|o| o.as_stream()) {
                if let Some(img) = crate::content::edit::XObjectImage::from_stream(
                    name.clone(),
                    &CosObject::Stream(stream.clone()),
                ) {
                    images.push(img);
                }
            }
        }
        Ok(images)
    }

    /// Replace an XObject image reference in the content stream AND update
    /// the page Resources dict. This combines content-stream renaming with
    /// resource dictionary insertion using the Phase 1.3 engine.
    pub fn replace_image_on_page(
        &mut self,
        page_index: usize,
        old_name: &str,
        new_name: &str,
        new_image: CosObject,
        remove_old: bool,
    ) -> PdfResult<usize> {
        let mut ops = self.get_content_operators(page_index)?;
        let id = self.page_id(page_index)?;

        // Get mutable page dict to modify Resources
        let page_dict = self
            .doc
            .objects
            .get_mut(&id)
            .and_then(|o| o.as_dictionary_mut())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} dictionary not found"),
            })?;

        // Ensure /Resources exists
        if page_dict.get(&CosName::resources()).is_none() {
            page_dict.insert(
                CosName::resources(),
                CosObject::Dictionary(CosDictionary::new()),
            );
        }

        let resources = page_dict
            .get_mut(&CosName::resources())
            .and_then(|o| o.as_dictionary_mut())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} /Resources is not a dict"),
            })?;

        let changed = crate::content::edit::replace_image_xobject(
            &mut ops, resources, old_name, new_name, new_image, remove_old,
        );

        if changed > 0 {
            self.set_content_operators(page_index, &ops)?;
        }

        Ok(changed)
    }

    // ── Page operations ───────────────────────────────────────────────

    /// Merge another document into this one by appending all its pages.
    pub fn merge_document(&mut self, other: &Document) -> PdfResult<()> {
        let mut merger = PdfMerger::new();
        // First, add our current doc's pages to the merger
        merger.append(&self.doc)?;
        // Then append the other doc
        merger.append(other)?;
        self.doc = merger.finish();
        Ok(())
    }

    /// Split the document into multiple documents, each with at most `pages_per_doc` pages.
    pub fn split(&mut self, pages_per_doc: usize) -> PdfResult<Vec<Document>> {
        let mut splitter = PdfSplitter::new(&mut self.doc);
        splitter.split(pages_per_doc)
    }

    /// Extract a subset of pages into a new `Document`.
    /// `indices` are zero-based page indices. The source document is not modified.
    pub fn extract_pages(&self, indices: &[usize]) -> PdfResult<Document> {
        // extract_pages takes &mut Document but only reads the structure
        extract_pages(&mut self.doc.clone(), indices)
    }

    /// Delete pages by index. Remaining pages keep their relative order.
    pub fn delete_page(&mut self, page_index: usize) -> PdfResult<()> {
        let n = self.page_count();
        if page_index >= n {
            return Err(PdfError::Parse {
                offset: None,
                context: format!(
                    "page index {page_index} out of range (count={n})"
                ),
            });
        }
        let indices: Vec<usize> = (0..n).filter(|&i| i != page_index).collect();
        self.doc = extract_pages(&mut self.doc, &indices)?;
        Ok(())
    }

    /// Delete multiple pages by index. Remaining pages keep their relative order.
    pub fn delete_pages(&mut self, page_indices: &[usize]) -> PdfResult<()> {
        let n = self.page_count();
        let keep: std::collections::HashSet<usize> =
            (0..n).filter(|i| !page_indices.contains(i)).collect();
        let mut indices: Vec<usize> = keep.into_iter().collect();
        indices.sort_unstable();
        self.doc = extract_pages(&mut self.doc, &indices)?;
        Ok(())
    }

    /// Reorder pages. `order` is the desired order as zero-based page indices.
    /// For example, `reorder_pages(&[2, 0, 1])` moves page 3 to the front.
    pub fn reorder_pages(&mut self, order: &[usize]) -> PdfResult<()> {
        self.doc = extract_pages(&mut self.doc, order)?;
        Ok(())
    }

    /// Rotate a page by the given number of degrees (multiple of 90).
    /// Positive = clockwise, negative = counter-clockwise.
    pub fn rotate_page(&mut self, page_index: usize, degrees: i64) -> PdfResult<()> {
        rotate_page_fn(&mut self.doc, page_index, degrees)
    }

    // ── Internal: read raw decoded content bytes ───────────────────────

    fn page_raw_content_bytes(&self, page_index: usize) -> PdfResult<Vec<u8>> {
        let id = self.page_id(page_index)?;
        let page_dict = self
            .doc
            .objects
            .get(&id)
            .and_then(|o| o.as_dictionary())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} dictionary not found"),
            })?;

        let contents_obj = page_dict
            .get(&CosName::contents())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} has no /Contents"),
            })?;

        let resolved = self.doc.objects.resolve(contents_obj);

        if let Some(stream) = resolved.and_then(|o| o.as_stream()) {
            let raw = &stream.data;
            let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
            decode_stream(raw, filter).map_err(|e| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} content decode: {e}"),
            })
        } else if let Some(arr) = resolved.and_then(|o| o.as_array()) {
            let mut combined = Vec::new();
            for item in arr {
                let Some(resolved_item) = self.doc.objects.resolve(item) else {
                    continue;
                };
                let Some(stream) = resolved_item.as_stream() else {
                    continue;
                };
                let raw = &stream.data;
                let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
                let decoded = decode_stream(raw, filter).map_err(|e| PdfError::Parse {
                    offset: None,
                    context: format!("page {page_index} content decode: {e}"),
                })?;
                combined.extend_from_slice(&decoded);
            }
            Ok(combined)
        } else {
            Err(PdfError::Parse {
                offset: None,
                context: format!("page {page_index} /Contents is neither stream nor array"),
            })
        }
    }

    // ── Internal: replace page content stream ──────────────────────────

    fn replace_page_content_stream(
        &mut self,
        page_index: usize,
        new_bytes: Vec<u8>,
    ) -> PdfResult<()> {
        let id = self.page_id(page_index)?;

        // allocate fresh ObjectId
        let max_num = self.doc.objects.max_object_number();
        let stream_id = ObjectId::new(max_num + 1, 0);
        let stream_obj = CosObject::Stream(CosStream {
            dictionary: CosDictionary::new(),
            data: new_bytes,
        });
        self.doc.objects.insert(stream_id, stream_obj);

        // Update page dict's /Contents to point to the new stream
        let page_dict = self
            .doc
            .objects
            .get_mut(&id)
            .and_then(|o| o.as_dictionary_mut())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {page_index} dictionary not found"),
            })?;
        page_dict.insert(CosName::contents(), CosObject::Reference(stream_id));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::CosName;

    fn minimal_pdf_bytes() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();

        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        let content = b"BT ET";
        let obj3_offset = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R /Resources << /XObject << /Im1 4 0 R >> >> >>\nendobj\n",
        );

        let obj4_offset = pdf.len();
        pdf.extend_from_slice(b"4 0 obj\n<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n\xff\nendstream\nendobj\n");

        let obj5_offset = pdf.len();
        pdf.extend_from_slice(
            format!("5 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes(),
        );
        pdf.extend_from_slice(content);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n");
        pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj3_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj5_offset).as_bytes());

        pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());
        pdf
    }

    #[test]
    fn test_editor_page_count() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        assert_eq!(editor.page_count(), 1);
    }

    #[test]
    fn test_editor_replace_image_xobject() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);

        let new_data = vec![0x80u8; 1]; // 1x1 128 gray
        editor
            .replace_image_xobject(0, "Im1", &new_data, 1, 1, "DeviceGray", 8, None)
            .unwrap();

        // Verify the image stream was updated
        let page_id = editor.page_id(0).unwrap();
        let page_dict = editor
            .document()
            .objects
            .get(&page_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let xobj = page_dict
            .get(&CosName::new(b"Resources".to_vec()))
            .and_then(|r| r.as_dictionary())
            .and_then(|r| r.get(&CosName::new(b"XObject".to_vec())))
            .and_then(|x| x.as_dictionary())
            .and_then(|x| x.get(&CosName::new(b"Im1".to_vec())));
        let stream = match xobj {
            Some(CosObject::Reference(rid)) => editor
                .document()
                .objects
                .get(rid)
                .and_then(|o| o.as_stream()),
            _ => None,
        };
        assert!(stream.is_some());
        let stream = stream.unwrap();
        assert_eq!(stream.data, new_data);
        assert_eq!(
            stream
                .dictionary
                .get(&CosName::new(b"Width".to_vec()))
                .and_then(|v| v.as_integer()),
            Some(1)
        );
        assert_eq!(
            stream
                .dictionary
                .get(&CosName::new(b"Height".to_vec()))
                .and_then(|v| v.as_integer()),
            Some(1)
        );
    }

    #[test]
    fn test_editor_rename_xobject() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);
        let count = editor.rename_xobject_on_page(0, "Im1", "ImX").unwrap();
        assert_eq!(count, 0); // no content stream operators that use Im1
    }

    #[test]
    fn test_editor_text_roundtrip() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);

        // Content stream has BT ET operators
        let ops = editor.get_content_operators(0).unwrap();
        assert!(!ops.is_empty());
        // Roundtrip: set back and parse again
        editor.set_content_operators(0, &ops).unwrap();
        let ops2 = editor.get_content_operators(0).unwrap();
        assert_eq!(ops.len(), ops2.len());
    }

    #[test]
    fn test_editor_save_roundtrip() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        let bytes = editor.save_to_bytes().unwrap();
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn test_editor_into_document() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        let doc_back = editor.into_document();
        assert_eq!(doc_back.page_count(), 1);
    }

    #[test]
    fn test_editor_document_mut_access() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);
        let doc_mut = editor.document_mut();
        assert!(doc_mut.catalog().is_some());
    }

    #[test]
    fn test_editor_find_text_empty() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        let results = editor.find_text_all_pages("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_editor_extract_text_empty() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        let text = editor.extract_text_from_page(0).unwrap();
        // minimal PDF has "BT ET" — no text show ops, so empty
        assert!(text.is_empty());
    }

    #[test]
    fn test_editor_extract_text_all_pages_empty() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        let results = editor.extract_text_all_pages();
        assert!(results.is_empty());
    }

    #[test]
    fn test_editor_find_images_on_page() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        let images = editor.find_images_on_page(0).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].name.as_str(), Some("Im1"));
    }

    #[test]
    fn test_editor_find_images_out_of_range() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let editor = PdfEditor::new(doc);
        assert!(editor.find_images_on_page(99).is_err());
    }

    #[test]
    fn test_editor_replace_image_on_page() {
        use crate::cos::CosStream;
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);

        // Add a /Do operator referencing the image to the content stream
        let mut ops = editor.get_content_operators(0).unwrap();
        ops.push(crate::content::edit::ContentOperator::InvokeXObject(
            CosName::new(b"Im1".to_vec()),
        ));
        editor.set_content_operators(0, &ops).unwrap();

        let mut img_dict = CosDictionary::new();
        img_dict.set(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"XObject".to_vec())));
        img_dict.set(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Image".to_vec())));
        img_dict.set(CosName::new(b"Width".to_vec()), CosObject::Integer(2));
        img_dict.set(CosName::new(b"Height".to_vec()), CosObject::Integer(2));
        img_dict.set(CosName::new(b"ColorSpace".to_vec()), CosObject::Name(CosName::new(b"DeviceGray".to_vec())));
        img_dict.set(CosName::new(b"BitsPerComponent".to_vec()), CosObject::Integer(8));
        let new_stream = CosObject::Stream(CosStream::new(img_dict, b"replacement-data".to_vec()));

        let changed = editor
            .replace_image_on_page(0, "Im1", "Im1", new_stream, false)
            .unwrap();
        assert_eq!(changed, 1);
    }

    #[test]
    fn test_editor_replace_image_rename() {
        use crate::cos::CosStream;
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);

        // Add /Do operators for the image
        let mut ops = editor.get_content_operators(0).unwrap();
        ops.push(crate::content::edit::ContentOperator::InvokeXObject(
            CosName::new(b"Im1".to_vec()),
        ));
        ops.push(crate::content::edit::ContentOperator::InvokeXObject(
            CosName::new(b"Im1".to_vec()),
        ));
        editor.set_content_operators(0, &ops).unwrap();

        let mut img_dict = CosDictionary::new();
        img_dict.set(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"XObject".to_vec())));
        img_dict.set(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Image".to_vec())));
        let new_stream = CosObject::Stream(CosStream::new(img_dict, b"data".to_vec()));

        let changed = editor
            .replace_image_on_page(0, "Im1", "ImNew", new_stream, true)
            .unwrap();
        assert_eq!(changed, 2);
    }

    // ── Page operation tests ───────────────────────────────────────────

    #[test]
    fn test_editor_merge_document() {
        let doc1 = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc1);
        let doc2 = {
            let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
            doc
        };
        editor.merge_document(&doc2).unwrap();
        assert_eq!(editor.page_count(), 2);
    }

    #[test]
    fn test_editor_split() {
        use crate::pdmodel::{DocumentBuilder, PageSize};
        let mut merger = PdfMerger::new();
        for _ in 0..4 {
            let doc = DocumentBuilder::new().page_size(PageSize::A4).build().unwrap();
            merger.append(&doc).unwrap();
        }
        let doc = merger.finish();
        let mut editor = PdfEditor::new(doc);
        let result = editor.split(2).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].page_count(), 2);
        assert_eq!(result[1].page_count(), 2);
    }

    #[test]
    fn test_editor_extract_pages() {
        use crate::pdmodel::{DocumentBuilder, PageSize};
        let mut merger = PdfMerger::new();
        for _ in 0..3 {
            let doc = DocumentBuilder::new().page_size(PageSize::A4).build().unwrap();
            merger.append(&doc).unwrap();
        }
        let doc = merger.finish();
        let editor = PdfEditor::new(doc);
        let extracted = editor.extract_pages(&[0, 2]).unwrap();
        assert_eq!(extracted.page_count(), 2);
    }

    #[test]
    fn test_editor_delete_page() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);
        assert_eq!(editor.page_count(), 1);
        editor.delete_page(0).unwrap();
        assert_eq!(editor.page_count(), 0);
    }

    #[test]
    fn test_editor_delete_page_out_of_range() {
        let doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let mut editor = PdfEditor::new(doc);
        assert!(editor.delete_page(99).is_err());
    }

    #[test]
    fn test_editor_delete_pages() {
        use crate::pdmodel::{DocumentBuilder, PageSize};
        let mut merger = PdfMerger::new();
        for _ in 0..5 {
            let doc = DocumentBuilder::new().page_size(PageSize::A4).build().unwrap();
            merger.append(&doc).unwrap();
        }
        let doc = merger.finish();
        let mut editor = PdfEditor::new(doc);
        editor.delete_pages(&[1, 3]).unwrap();
        assert_eq!(editor.page_count(), 3);
    }

    #[test]
    fn test_editor_reorder_pages() {
        use crate::pdmodel::{DocumentBuilder, PageSize};
        let mut merger = PdfMerger::new();
        for _ in 0..4 {
            let doc = DocumentBuilder::new().page_size(PageSize::A4).build().unwrap();
            merger.append(&doc).unwrap();
        }
        let doc = merger.finish();
        let mut editor = PdfEditor::new(doc);
        editor.reorder_pages(&[3, 2, 1, 0]).unwrap();
        assert_eq!(editor.page_count(), 4);
    }

    #[test]
    fn test_editor_rotate_page() {
        use crate::pdmodel::{DocumentBuilder, PageSize};
        let doc = DocumentBuilder::new().page_size(PageSize::A4).build().unwrap();
        let mut editor = PdfEditor::new(doc);
        editor.rotate_page(0, 90).unwrap();
        // verify by accessing the underlying doc
        let tree = editor.document().pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 90);
    }
}
