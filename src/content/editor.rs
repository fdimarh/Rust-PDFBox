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
}
