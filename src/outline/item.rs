//!
//! Document outline items — individual bookmark entries in the outline tree.
//!
//! Maps to `PDOutlineItem` in Java PDFBox.

use crate::cos::{CosDictionary, CosName, ObjectId};
use crate::ObjectStore;

use super::destination::Destination;

/// A single item in the document outline (bookmark).
///
/// Each item has a title, an optional destination, and optional child items.
#[derive(Debug, Clone)]
pub struct OutlineItem<'a> {
    /// The object ID of this item in the document store.
    pub id: ObjectId,
    /// Reference to the item's dictionary (borrowed from the store).
    dict: &'a CosDictionary,
    /// Reference to the object store for resolving children.
    store: &'a ObjectStore,
}

impl<'a> OutlineItem<'a> {
    /// Creates a new `OutlineItem` from its dictionary and store.
    pub fn new(id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore) -> Self {
        Self { id, dict, store }
    }

    /// The title text displayed for this bookmark.
    pub fn title(&self) -> String {
        self.dict
            .get(&CosName::new(b"Title".to_vec()))
            .and_then(|v| v.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string())
            .unwrap_or_default()
    }

    /// Returns `true` if this item is initially open (children visible).
    pub fn is_open(&self) -> bool {
        self.dict
            .get_int(&CosName::count())
            .map(|c| c > 0)
            .unwrap_or(false)
    }

    /// Returns the raw `/Count` value (positive = open, negative = closed,
    /// absolute value = number of visible descendants).
    pub fn count(&self) -> i64 {
        self.dict.get_int(&CosName::count()).unwrap_or(0)
    }

    /// Parses the destination for this outline item.
    pub fn destination(&self, page_id_to_index: &impl Fn(ObjectId) -> Option<usize>) -> Option<Destination> {
        // Try /Dest first (direct destination or action dict)
        if let Some(dest_obj) = self.dict.get(&CosName::new(b"Dest".to_vec())) {
            if let Some(dest) = Destination::from_cos(dest_obj, page_id_to_index) {
                return Some(dest);
            }
        }
        // Try /A (action dictionary)
        if let Some(action) = self.dict.get(&CosName::new(b"A".to_vec())) {
            if let Some(dest) = Destination::from_cos(action, page_id_to_index) {
                return Some(dest);
            }
        }
        None
    }

    /// Returns the first child item, if any.
    pub fn first_child(&self) -> Option<OutlineItem<'a>> {
        let kid_id = self
            .dict
            .get(&CosName::new(b"First".to_vec()))
            .and_then(|v| v.as_reference())?;
        let dict = self.store.get(&kid_id)?.as_dictionary()?;
        Some(OutlineItem::new(kid_id, dict, self.store))
    }

    /// Returns the last child item, if any.
    pub fn last_child(&self) -> Option<OutlineItem<'a>> {
        let kid_id = self
            .dict
            .get(&CosName::new(b"Last".to_vec()))
            .and_then(|v| v.as_reference())?;
        let dict = self.store.get(&kid_id)?.as_dictionary()?;
        Some(OutlineItem::new(kid_id, dict, self.store))
    }

    /// Returns the next sibling item, if any.
    pub fn next(&self) -> Option<OutlineItem<'a>> {
        let next_id = self
            .dict
            .get(&CosName::new(b"Next".to_vec()))
            .and_then(|v| v.as_reference())?;
        let dict = self.store.get(&next_id)?.as_dictionary()?;
        Some(OutlineItem::new(next_id, dict, self.store))
    }

    /// Returns the previous sibling item, if any.
    pub fn prev(&self) -> Option<OutlineItem<'a>> {
        let prev_id = self
            .dict
            .get(&CosName::new(b"Prev".to_vec()))
            .and_then(|v| v.as_reference())?;
        let dict = self.store.get(&prev_id)?.as_dictionary()?;
        Some(OutlineItem::new(prev_id, dict, self.store))
    }

    /// Returns the parent outline item, if any.
    pub fn parent(&self) -> Option<OutlineItem<'a>> {
        let parent_id = self
            .dict
            .get(&CosName::new(b"Parent".to_vec()))
            .and_then(|v| v.as_reference())?;
        let dict = self.store.get(&parent_id)?.as_dictionary()?;
        Some(OutlineItem::new(parent_id, dict, self.store))
    }

    /// Recursively collects all descendant items (depth-first).
    pub fn descendants(&self) -> Vec<OutlineItem<'a>> {
        let mut result = Vec::new();
        self.collect_descendants(&mut result);
        result
    }

    fn collect_descendants(&self, out: &mut Vec<OutlineItem<'a>>) {
        let mut current = self.first_child();
        while let Some(ref item) = current {
            out.push(item.clone());
            item.collect_descendants(out);
            current = item.next();
        }
    }

    /// Returns the raw dictionary.
    pub fn dictionary(&self) -> &CosDictionary {
        self.dict
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};

    fn make_item_dict(title: &str, count: i64) -> CosDictionary {
        let mut d = CosDictionary::new();
        d.insert(
            CosName::new(b"Title".to_vec()),
            CosObject::String(title.as_bytes().to_vec()),
        );
        d.insert(CosName::count(), CosObject::Integer(count));
        d
    }

    #[test]
    fn test_outline_item_title() {
        let dict = make_item_dict("Chapter 1", 0);
        let store = crate::ObjectStore::new();
        let item = OutlineItem::new(ObjectId::new(1, 0), &dict, &store);
        assert_eq!(item.title(), "Chapter 1");
    }

    #[test]
    fn test_outline_item_open_closed() {
        let store = crate::ObjectStore::new();

        let open_dict = make_item_dict("Open", 3);
        let open_item = OutlineItem::new(ObjectId::new(1, 0), &open_dict, &store);
        assert!(open_item.is_open());

        let closed_dict = make_item_dict("Closed", -3);
        let closed_item = OutlineItem::new(ObjectId::new(2, 0), &closed_dict, &store);
        assert!(!closed_item.is_open());
    }

    #[test]
    fn test_outline_item_no_children() {
        let dict = make_item_dict("Leaf", 0);
        let store = crate::ObjectStore::new();
        let item = OutlineItem::new(ObjectId::new(1, 0), &dict, &store);
        assert!(item.first_child().is_none());
        assert!(item.descendants().is_empty());
    }
}
