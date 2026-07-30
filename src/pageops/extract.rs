use crate::cos::{CosDictionary, CosName, CosObject};
use crate::parser::xref::XRefEntry;
use crate::{Document, PdfResult};

/// Extracts a subset of pages into a new `Document`.
///
/// `page_indices` is zero-based array of pages to include.
/// The resulting document will have identical page sizes and content.
pub fn extract_pages(doc: &mut Document, page_indices: &[usize]) -> PdfResult<Document> {
    let mut new_doc = Document::empty();

    let catalog_id = new_doc.allocate_object_id();
    let pages_id = new_doc.allocate_object_id();

    // Trailer & Catalog setup
    new_doc.xref.trailer.insert(
        CosName::new(b"Root".to_vec()),
        CosObject::Reference(catalog_id),
    );

    let mut catalog = CosDictionary::new();
    catalog.insert(
        CosName::type_name(),
        CosObject::Name(CosName::new(b"Catalog".to_vec())),
    );
    catalog.insert(CosName::pages(), CosObject::Reference(pages_id));
    new_doc.insert_object(catalog_id, CosObject::Dictionary(catalog));
    new_doc.xref.insert_if_absent(
        catalog_id,
        XRefEntry::InUse {
            offset: 0,
            generation: 0,
        },
    );

    let tree = doc.pages()?;
    let mut kids = Vec::new();

    for &idx in page_indices {
        if let Some(page) = tree.get(idx) {
            let mut page_dict = page.dictionary().clone();
            let new_pid = new_doc.allocate_object_id();

            // Adjust parent to the new pages node
            page_dict.insert(
                CosName::new(b"Parent".to_vec()),
                CosObject::Reference(pages_id),
            );

            // Note: In a complete implementation we need deep copying of all referenced objects,
            // or merging ObjectStores and remapping ObjectIDs.
            // For now, simpler extract for flat pages (assumes shared store or single pass deep copy).
            // A true deep copy mechanism will be required.
            // This is a minimal placeholder showing structure.

            new_doc.insert_object(new_pid, CosObject::Dictionary(page_dict));
            new_doc.xref.insert_if_absent(
                new_pid,
                XRefEntry::InUse {
                    offset: 0,
                    generation: 0,
                },
            );
            kids.push(CosObject::Reference(new_pid));
        }
    }

    let mut pages = CosDictionary::new();
    pages.insert(
        CosName::type_name(),
        CosObject::Name(CosName::new(b"Pages".to_vec())),
    );
    pages.insert(CosName::count(), CosObject::Integer(kids.len() as i64));
    pages.insert(CosName::kids(), CosObject::Array(kids));

    new_doc.insert_object(pages_id, CosObject::Dictionary(pages));
    new_doc.xref.insert_if_absent(
        pages_id,
        XRefEntry::InUse {
            offset: 0,
            generation: 0,
        },
    );

    // Copy all objects from the original document that are referenced.
    // For a simplistic extract, we can just copy the entire object store.
    for key in doc.objects.keys().cloned().collect::<Vec<_>>() {
        if let Some(obj) = doc.objects.get(&key) {
            if !new_doc.objects.get(&key).is_some() {
                new_doc.insert_object(key, obj.clone());
                new_doc.xref.insert_if_absent(
                    key,
                    XRefEntry::InUse {
                        offset: 0,
                        generation: 0,
                    },
                );
            }
        }
    }

    let size = new_doc.objects.max_object_number() + 1;
    new_doc.xref.trailer.insert(
        CosName::new(b"Size".to_vec()),
        CosObject::Integer(size as i64),
    );

    Ok(new_doc)
}

// =========================================================================
// Unit tests for extract_pages
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdmodel::{DocumentBuilder, PageSize};

    /// Builds a multi-page document by merging `count` single-page docs.
    fn build_multi_page_doc(count: usize) -> Document {
        let mut merger = crate::pageops::PdfMerger::new();
        for _ in 0..count {
            let doc = DocumentBuilder::new()
                .page_size(PageSize::A4)
                .build()
                .unwrap();
            merger.append(&doc).unwrap();
        }
        merger.finish()
    }

    // ── Happy path ──────────────────────────────────────────────────────

    #[test]
    fn test_extract_first_page_from_two_page_doc() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(2);
        let extracted = extract_pages(&mut doc, &[0])?;
        assert_eq!(extracted.page_count(), 1);
        Ok(())
    }

    #[test]
    fn test_extract_last_page_from_two_page_doc() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(2);
        let extracted = extract_pages(&mut doc, &[1])?;
        assert_eq!(extracted.page_count(), 1);
        Ok(())
    }

    #[test]
    fn test_extract_multiple_pages() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(4);
        let extracted = extract_pages(&mut doc, &[0, 2])?;
        assert_eq!(extracted.page_count(), 2);
        Ok(())
    }

    #[test]
    fn test_extract_consecutive_range() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(5);
        let extracted = extract_pages(&mut doc, &[1, 2, 3])?;
        assert_eq!(extracted.page_count(), 3);
        Ok(())
    }

    #[test]
    fn test_extract_all_pages() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(3);
        let extracted = extract_pages(&mut doc, &[0, 1, 2])?;
        assert_eq!(extracted.page_count(), 3);
        Ok(())
    }

    #[test]
    fn test_extract_all_pages_reversed() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(3);
        let extracted = extract_pages(&mut doc, &[2, 1, 0])?;
        assert_eq!(extracted.page_count(), 3);
        Ok(())
    }

    // ── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn test_extract_empty_indices() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(3);
        let extracted = extract_pages(&mut doc, &[])?;
        assert_eq!(extracted.page_count(), 0);
        Ok(())
    }

    #[test]
    fn test_extract_from_single_page_doc() -> PdfResult<()> {
        let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
        let extracted = extract_pages(&mut doc, &[0])?;
        assert_eq!(extracted.page_count(), 1);
        Ok(())
    }

    #[test]
    fn test_extract_nonexistent_index_returns_zero_pages() -> PdfResult<()> {
        // An out-of-bounds index is silently skipped by the current implementation
        // (tree.get(idx) returns None). This test documents that behaviour.
        let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
        let extracted = extract_pages(&mut doc, &[99])?;
        assert_eq!(extracted.page_count(), 0);
        Ok(())
    }

    #[test]
    fn test_extract_mixed_valid_and_invalid_indices() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(2);
        let extracted = extract_pages(&mut doc, &[0, 99, 1])?;
        // Valid pages [0, 1] are extracted; invalid index 99 is silently skipped.
        assert_eq!(extracted.page_count(), 2);
        Ok(())
    }

    // ── Round-trip ──────────────────────────────────────────────────────

    #[test]
    fn test_extract_round_trip() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(3);
        let extracted = extract_pages(&mut doc, &[0, 2])?;

        let mut buf = std::io::Cursor::new(Vec::new());
        extracted.save_to(&mut buf)?;
        let reloaded = Document::load_from_bytes(buf.get_ref())?;
        assert_eq!(reloaded.page_count(), 2);
        Ok(())
    }

    #[test]
    fn test_extract_all_round_trip() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(2);
        let extracted = extract_pages(&mut doc, &[0, 1])?;

        let mut buf = std::io::Cursor::new(Vec::new());
        extracted.save_to(&mut buf)?;
        let reloaded = Document::load_from_bytes(buf.get_ref())?;
        assert_eq!(reloaded.page_count(), 2);
        Ok(())
    }

    // ── Source document unchanged for valid indices ──────────────────────

    #[test]
    fn test_source_doc_unchanged_after_extract() -> PdfResult<()> {
        let mut doc = build_multi_page_doc(4);
        let _extracted = extract_pages(&mut doc, &[0, 2])?;

        // The source document should still have its original page count
        assert_eq!(doc.page_count(), 4);
        Ok(())
    }
}
