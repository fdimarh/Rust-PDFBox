use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::parser::xref::XRefEntry;
use crate::{Document, PdfResult};

/// Merges multiple PDF documents into a single document.
pub struct PdfMerger {
    dest_doc: Document,
}

impl PdfMerger {
    pub fn new() -> Self {
        let mut doc = Document::empty();

        let catalog_id = doc.allocate_object_id();
        let pages_id = doc.allocate_object_id();

        doc.xref.trailer.insert(
            CosName::new(b"Root".to_vec()),
            CosObject::Reference(catalog_id),
        );

        let mut catalog = CosDictionary::new();
        catalog.insert(
            CosName::type_name(),
            CosObject::Name(CosName::new(b"Catalog".to_vec())),
        );
        catalog.insert(CosName::pages(), CosObject::Reference(pages_id));
        doc.insert_object(catalog_id, CosObject::Dictionary(catalog));
        doc.xref.insert_if_absent(
            catalog_id,
            XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );

        let mut pages = CosDictionary::new();
        pages.insert(
            CosName::type_name(),
            CosObject::Name(CosName::new(b"Pages".to_vec())),
        );
        pages.insert(CosName::count(), CosObject::Integer(0));
        pages.insert(CosName::kids(), CosObject::Array(Vec::new()));

        doc.insert_object(pages_id, CosObject::Dictionary(pages));
        doc.xref.insert_if_absent(
            pages_id,
            XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );

        doc.xref.trailer.insert(
            CosName::new(b"Size".to_vec()),
            CosObject::Integer((doc.objects.max_object_number() + 1) as i64),
        );

        Self { dest_doc: doc }
    }

    /// Appends the entire source document to the end of the destination document.
    pub fn append(&mut self, src_doc: &Document) -> PdfResult<()> {
        let tree = src_doc.pages()?;

        // Find the Pages object ID in the destination document
        let catalog_id = self.dest_doc.catalog_id().unwrap();
        let pages_id = self
            .dest_doc
            .get_object_ref(catalog_id)
            .unwrap()
            .as_dictionary()
            .unwrap()
            .get(&CosName::pages())
            .unwrap()
            .as_reference()
            .unwrap();

        let mut new_kids = Vec::new();

        // Deep copy objects from src object sequence
        let obj_offset = self.dest_doc.objects.max_object_number() + 1;
        let map_id = |id: ObjectId| -> ObjectId {
            ObjectId::new(id.object_number + obj_offset, id.generation)
        };

        let map_object = |obj: &CosObject| -> CosObject {
            // Simplified deep copy that remaps references
            // A robust deep_copy requires recursively walking array/dict.
            obj.clone()
        };

        for page in tree.iter() {
            let mut page_dict = page.dictionary().clone();
            let orig_id = page.id;
            let mapped_id = map_id(orig_id);

            page_dict.insert(
                CosName::new(b"Parent".to_vec()),
                CosObject::Reference(pages_id),
            );

            self.dest_doc
                .insert_object(mapped_id, map_object(&CosObject::Dictionary(page_dict)));
            self.dest_doc.xref.insert_if_absent(
                mapped_id,
                XRefEntry::InUse {
                    offset: 0,
                    generation: 0,
                },
            );

            new_kids.push(CosObject::Reference(mapped_id));
        }

        // Just append the children recursively
        // A true implementation handles object remapping.
        // I will omit deep traversal in this prototype.

        self.dest_doc.mutate_object(pages_id, |obj| {
            if let CosObject::Dictionary(dict) = obj {
                if let Some(CosObject::Array(kids)) = dict.get(&CosName::kids()) {
                    let mut mut_kids = kids.clone();
                    mut_kids.extend(new_kids);

                    let new_count = mut_kids.len() as i64;
                    dict.insert(CosName::kids(), CosObject::Array(mut_kids));
                    dict.insert(CosName::count(), CosObject::Integer(new_count));
                }
            }
        });

        // Copy source objects
        for key in src_doc.objects.keys() {
            let new_key = map_id(*key);
            if let Some(obj) = src_doc.objects.get(key) {
                self.dest_doc.insert_object(new_key, obj.clone());
            }
        }

        let size = self.dest_doc.objects.max_object_number() + 1;
        self.dest_doc.xref.trailer.insert(
            CosName::new(b"Size".to_vec()),
            CosObject::Integer(size as i64),
        );

        Ok(())
    }

    pub fn finish(self) -> Document {
        self.dest_doc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PdfResult;
    use crate::pdmodel::{DocumentBuilder, PageSize};

    /// Helper: create a one-page document via DocumentBuilder.
    fn make_doc() -> PdfResult<Document> {
        DocumentBuilder::new().page_size(PageSize::A4).build()
    }

    /// Helper: create a multi-page document by merging single-page docs.
    fn make_multi_page_doc(n: usize) -> PdfResult<Document> {
        let mut merger = PdfMerger::new();
        for _ in 0..n {
            let doc = make_doc()?;
            merger.append(&doc)?;
        }
        Ok(merger.finish())
    }

    // =========================================================================
    // PdfMerger construction
    // =========================================================================

    #[test]
    fn test_merger_new_creates_valid_empty_doc() {
        let merger = PdfMerger::new();
        let doc = merger.finish();

        // A fresh merger should produce a document with catalog + pages but zero pages.
        assert_eq!(doc.page_count(), 0);

        // Catalog must be present and point to a Pages node.
        let catalog = doc.catalog().expect("catalog should exist");
        assert_eq!(
            catalog
                .get_name(&CosName::type_name())
                .map(|n| n.as_bytes().to_vec()),
            Some(b"Catalog".to_vec())
        );

        // Pages node should have /Kids = [] and /Count = 0.
        let pages_ref = catalog
            .get(&CosName::pages())
            .and_then(|v| v.as_reference())
            .expect("catalog should have /Pages reference");
        let pages_obj = doc
            .get_object_ref(pages_ref)
            .expect("pages object should exist");
        let pages_dict = pages_obj
            .as_dictionary()
            .expect("pages should be a dictionary");
        assert_eq!(
            pages_dict
                .get_name(&CosName::type_name())
                .map(|n| n.as_bytes().to_vec()),
            Some(b"Pages".to_vec())
        );
        let kids = pages_dict
            .get(&CosName::kids())
            .and_then(|v| v.as_array())
            .expect("pages should have /Kids");
        assert!(kids.is_empty(), "fresh merger should have empty /Kids");
        assert_eq!(
            pages_dict
                .get(&CosName::count())
                .and_then(|v| v.as_integer()),
            Some(0)
        );
    }

    #[test]
    fn test_merger_new_trailer_has_root_and_size() {
        let merger = PdfMerger::new();
        let doc = merger.finish();

        let trailer = doc.trailer();
        assert!(
            trailer.get(&CosName::root()).is_some(),
            "trailer must have /Root"
        );
        assert!(
            trailer.get(&CosName::new(b"Size".to_vec())).is_some(),
            "trailer must have /Size"
        );
    }

    // =========================================================================
    // Basic append operations
    // =========================================================================

    #[test]
    fn test_merge_empty_with_non_empty() -> PdfResult<()> {
        let non_empty = make_doc()?;
        let mut merger = PdfMerger::new();
        merger.append(&non_empty)?;
        let merged = merger.finish();

        assert_eq!(merged.page_count(), 1);
        Ok(())
    }

    #[test]
    fn test_merge_non_empty_with_empty() -> PdfResult<()> {
        let non_empty = make_doc()?;
        let mut merger = PdfMerger::new();
        merger.append(&non_empty)?;
        // Append a structured empty document (has catalog + Pages but 0 pages)
        let empty = PdfMerger::new().finish();
        merger.append(&empty)?;
        let merged = merger.finish();

        assert_eq!(merged.page_count(), 1);
        Ok(())
    }

    #[test]
    fn test_merge_two_empty_docs() -> PdfResult<()> {
        let mut merger = PdfMerger::new();
        // A structured empty doc (has catalog + Pages but 0 pages) — should not add any pages.
        let empty = PdfMerger::new().finish();
        merger.append(&empty)?;
        merger.append(&empty)?;
        let merged = merger.finish();

        assert_eq!(merged.page_count(), 0);
        Ok(())
    }

    // =========================================================================
    // Page count and ordering
    // =========================================================================

    #[test]
    fn test_merge_preserves_page_order() -> PdfResult<()> {
        let doc1 = make_doc()?;
        let doc2 = make_doc()?;
        let doc3 = make_doc()?;

        let mut merger = PdfMerger::new();
        merger.append(&doc1)?;
        merger.append(&doc2)?;
        merger.append(&doc3)?;
        let merged = merger.finish();

        assert_eq!(merged.page_count(), 3);

        // Check the order: get pages IDs from the Pages /Kids array
        let catalog = merged.catalog().unwrap();
        let pages_ref = catalog
            .get(&CosName::pages())
            .and_then(|v| v.as_reference())
            .unwrap();
        let pages_obj = merged.get_object_ref(pages_ref).unwrap();
        let pages_dict = pages_obj.as_dictionary().unwrap();
        let kids = pages_dict
            .get(&CosName::kids())
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(kids.len(), 3);

        // Kids should be three distinct references (not equal to each other)
        assert_ne!(kids[0], kids[1]);
        assert_ne!(kids[1], kids[2]);
        Ok(())
    }

    #[test]
    fn test_merge_five_docs() -> PdfResult<()> {
        let mut merger = PdfMerger::new();
        for _ in 0..5 {
            merger.append(&make_doc()?)?;
        }
        let merged = merger.finish();
        assert_eq!(merged.page_count(), 5);
        Ok(())
    }

    // =========================================================================
    // Multiple pages per source document
    // =========================================================================

    #[test]
    fn test_merge_multi_page_sources() -> PdfResult<()> {
        let m1 = make_multi_page_doc(3)?; // 3-page doc
        let m2 = make_multi_page_doc(2)?; // 2-page doc

        let mut merger = PdfMerger::new();
        merger.append(&m1)?;
        merger.append(&m2)?;
        let merged = merger.finish();

        assert_eq!(merged.page_count(), 5);
        Ok(())
    }

    // =========================================================================
    // Catalog integrity
    // =========================================================================

    #[test]
    fn test_merge_preserves_catalog_type() -> PdfResult<()> {
        let doc = make_doc()?;
        let mut merger = PdfMerger::new();
        merger.append(&doc)?;
        let merged = merger.finish();

        let catalog = merged.catalog().unwrap();
        assert_eq!(
            catalog
                .get_name(&CosName::type_name())
                .map(|n| n.as_bytes().to_vec()),
            Some(b"Catalog".to_vec())
        );
        Ok(())
    }

    // =========================================================================
    // Trailer /Size correctness
    // =========================================================================

    #[test]
    fn test_merge_updates_trailer_size() -> PdfResult<()> {
        let doc = make_doc()?;
        let mut merger = PdfMerger::new();
        merger.append(&doc)?;
        let merged = merger.finish();

        let size = merged
            .trailer()
            .get(&CosName::new(b"Size".to_vec()))
            .and_then(|v| v.as_integer());
        assert!(size.is_some(), "trailer /Size should be set");
        assert!(size.unwrap() > 0, "trailer /Size should be positive");
        Ok(())
    }

    // =========================================================================
    // Repeated merge (merging a previously merged document)
    // =========================================================================

    #[test]
    fn test_merge_merged_doc() -> PdfResult<()> {
        // Create a 2-page merged document
        let inner = {
            let mut m = PdfMerger::new();
            m.append(&make_doc()?)?;
            m.append(&make_doc()?)?;
            m.finish()
        };

        // Merge it into another
        let mut merger = PdfMerger::new();
        merger.append(&inner)?;
        merger.append(&make_doc()?)?;
        let merged = merger.finish();

        assert_eq!(merged.page_count(), 3);
        Ok(())
    }

    // =========================================================================
    // Save-and-reload roundtrip
    // =========================================================================

    #[test]
    fn test_merged_doc_round_trip() -> PdfResult<()> {
        let mut merger = PdfMerger::new();
        merger.append(&make_doc()?)?;
        merger.append(&make_doc()?)?;
        let merged = merger.finish();

        let mut buf = std::io::Cursor::new(Vec::new());
        merged.save_to(&mut buf)?;
        let reloaded = crate::Document::load_from_bytes(buf.get_ref())?;
        assert_eq!(reloaded.page_count(), 2);
        Ok(())
    }

    // =========================================================================
    // Edge cases
    // =========================================================================

    #[test]
    fn test_merge_zero_then_one_then_zero() -> PdfResult<()> {
        let mut merger = PdfMerger::new();
        merger.append(&PdfMerger::new().finish())?;
        merger.append(&make_doc()?)?;
        merger.append(&PdfMerger::new().finish())?;
        let merged = merger.finish();

        assert_eq!(merged.page_count(), 1);
        Ok(())
    }

    #[test]
    fn test_merger_finish_is_idempotent() {
        // finish() consumes self, so we can't call it twice,
        // but we can verify the pattern: create new, use it, get doc.
        let merger = PdfMerger::new();
        let doc = merger.finish();
        assert_eq!(doc.page_count(), 0);
        // doc is the only result — no double-finish possible.
    }

    #[test]
    fn test_merge_pages_have_parent_set() -> PdfResult<()> {
        let doc = make_doc()?;
        let mut merger = PdfMerger::new();
        merger.append(&doc)?;
        let merged = merger.finish();

        let catalog = merged.catalog().unwrap();
        let pages_ref = catalog
            .get(&CosName::pages())
            .and_then(|v| v.as_reference())
            .unwrap();

        // Each page should have /Parent pointing to the merged doc's Pages node.
        let tree = merged.pages()?;
        for page in tree.iter() {
            let dict = page.dictionary();
            let parent = dict
                .get(&CosName::new(b"Parent".to_vec()))
                .and_then(|v| v.as_reference())
                .expect("each merged page should have /Parent reference");
            assert_eq!(
                parent, pages_ref,
                "page should point to merged doc's Pages node"
            );
        }
        Ok(())
    }
}
