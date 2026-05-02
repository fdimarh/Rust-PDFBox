//!
//! Document outline (bookmarks) — navigation tree from the catalog's `/Outlines` entry.
//!
//! Maps to `org.apache.pdfbox.pdmodel.interactive.documentnavigation.outline.*`
//! in Java PDFBox.

pub mod destination;
pub mod item;

pub use destination::{Destination, FitMode};
pub use item::OutlineItem;

use crate::cos::{CosDictionary, CosName};
use crate::{Document, ObjectStore};

/// The root outline dictionary from the document catalog.
///
/// Maps to `PDDocumentOutline` in Java PDFBox.
///
/// # Example
///
/// ```rust,ignore
/// let outline = doc.outline()?;
/// for item in outline.items() {
///     println!("{}", item.title());
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DocumentOutline<'a> {
    dict: &'a CosDictionary,
    store: &'a ObjectStore,
}

impl<'a> DocumentOutline<'a> {
    /// Creates a new `DocumentOutline` from the `/Outlines` dictionary.
    pub fn new(dict: &'a CosDictionary, store: &'a ObjectStore) -> Self {
        Self { dict, store }
    }

    /// Returns the raw outline dictionary.
    pub fn dictionary(&self) -> &CosDictionary {
        self.dict
    }

    /// Returns the first top-level outline item, if any.
    pub fn first_item(&self) -> Option<OutlineItem<'a>> {
        let id = self
            .dict
            .get(&CosName::new(b"First".to_vec()))
            .and_then(|v| v.as_reference())?;
        let dict = self.store.get(&id)?.as_dictionary()?;
        Some(OutlineItem::new(id, dict, self.store))
    }

    /// Returns the last top-level outline item, if any.
    pub fn last_item(&self) -> Option<OutlineItem<'a>> {
        let id = self
            .dict
            .get(&CosName::new(b"Last".to_vec()))
            .and_then(|v| v.as_reference())?;
        let dict = self.store.get(&id)?.as_dictionary()?;
        Some(OutlineItem::new(id, dict, self.store))
    }

    /// Returns the total number of top-level outline items.
    pub fn count(&self) -> i64 {
        self.dict.get_int(&CosName::count()).unwrap_or(0).abs()
    }

    /// Iterates over all top-level outline items.
    pub fn items(&self) -> Vec<OutlineItem<'a>> {
        let mut result = Vec::new();
        let mut current = self.first_item();
        while let Some(ref item) = current {
            result.push(item.clone());
            current = item.next();
        }
        result
    }

    /// Recursively collects all outline items (depth-first).
    pub fn all_items(&self) -> Vec<OutlineItem<'a>> {
        let top_level = self.items();
        let mut result: Vec<OutlineItem<'a>> = Vec::new();
        for item in top_level {
            let desc = item.descendants();
            result.push(item);
            result.extend(desc);
        }
        result
    }
}

// ── Document extension methods ─────────────────────────────────────────────

/// Extension trait adding outline accessors to [`Document`].
///
/// These are intended to be called as `doc.outline()` etc.
pub trait OutlineExt {
    /// Returns the document outline (bookmarks), if present.
    fn outline(&self) -> Option<DocumentOutline<'_>>;
}

impl OutlineExt for Document {
    fn outline(&self) -> Option<DocumentOutline<'_>> {
        let catalog = self.catalog()?;
        let outlines_dict = catalog
            .get(&CosName::new(b"Outlines".to_vec()))
            .and_then(|v| {
                if let Some(id) = v.as_reference() {
                    self.get_object_ref(id)?.as_dictionary()
                } else {
                    v.as_dictionary()
                }
            })?;
        Some(DocumentOutline::new(outlines_dict, &self.objects))
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::Document;

    /// Build a minimal document with an outline containing 2 top-level items.
    fn doc_with_outline() -> Document {
        let bytes = b"%PDF-1.7\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
            4 0 obj\n<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>\nendobj\n\
            5 0 obj\n<< /Title (Chapter 1) /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>\nendobj\n\
            6 0 obj\n<< /Title (Chapter 2) /Parent 4 0 R /Prev 5 0 R /Dest [3 0 R /FitH 400] >>\nendobj\n\
            xref\n0 7\n0000000000 65535 f \n0000000009 00000 n \n0000000075 00000 n \n\
            0000000150 00000 n \n0000000260 00000 n \n0000000341 00000 n \n\
            0000000447 00000 n \ntrailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n552\n%%EOF";
        let (doc, _report) = Document::load_lenient(bytes);
        doc
    }

    #[test]
    fn test_outline_exists() {
        let doc = doc_with_outline();
        let outline = doc.outline();
        assert!(outline.is_some(), "outline should be present");
    }

    #[test]
    fn test_outline_top_level_items() {
        let doc = doc_with_outline();
        let outline = doc.outline().unwrap();
        let items = outline.items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title(), "Chapter 1");
        assert_eq!(items[1].title(), "Chapter 2");
    }

    #[test]
    fn test_outline_first_last() {
        let doc = doc_with_outline();
        let outline = doc.outline().unwrap();
        let first = outline.first_item().unwrap();
        let last = outline.last_item().unwrap();
        assert_eq!(first.title(), "Chapter 1");
        assert_eq!(last.title(), "Chapter 2");
    }

    #[test]
    fn test_outline_count() {
        let doc = doc_with_outline();
        let outline = doc.outline().unwrap();
        assert_eq!(outline.count(), 2);
    }

    #[test]
    fn test_no_outline() {
        let bytes = b"%PDF-1.7\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
            xref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000054 00000 n \n\
            0000000129 00000 n \ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n194\n%%EOF";
        let (doc, _report) = Document::load_lenient(bytes);
        assert!(doc.outline().is_none());
    }

    #[test]
    fn test_outline_item_navigation() {
        let doc = doc_with_outline();
        let outline = doc.outline().unwrap();
        let first = outline.first_item().unwrap();
        let next = first.next().unwrap();
        assert_eq!(next.title(), "Chapter 2");
        let prev = next.prev().unwrap();
        assert_eq!(prev.title(), "Chapter 1");
    }

    #[test]
    fn test_all_items() {
        let doc = doc_with_outline();
        let outline = doc.outline().unwrap();
        let all = outline.all_items();
        assert_eq!(all.len(), 2);
    }
}
