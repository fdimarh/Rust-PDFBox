//! Page tree traversal — walks the `/Pages` tree to collect all leaf pages.
//!
//! The PDF page tree is a balanced tree of intermediate nodes (`/Type /Pages`)
//! and leaf nodes (`/Type /Page`). This module resolves the full flat list of
//! pages from the root.
//!
//! # Java PDFBox mapping
//!
//! | Java class / method | Rust type / method |
//! |---|---|
//! | `PDDocumentCatalog.getPages()` | [`PageTree::new`] |
//! | `PDPageTree.iterator()` | [`PageTree::iter`] |
//! | `PDPageTree.getCount()` | [`PageTree::count`] |
//! | `PDPageTree.get(index)` | [`PageTree::get`] |

use crate::cos::{CosDictionary, CosName, ObjectId};
use crate::{ObjectStore, PdfError, PdfResult};

use super::page::Page;

/// A resolved, flat list of page dictionaries built from the PDF page tree.
///
/// After construction, pages can be iterated or accessed by 0-based index.
/// All inherited attributes (MediaBox, Resources, Rotate) are NOT yet merged
/// here — inheritance is handled lazily by [`Page`] accessors consulting
/// ancestor nodes (future work for M2 completion).
pub struct PageTree<'a> {
    /// Flat ordered list of page dictionary references (borrowed from store).
    pages: Vec<(ObjectId, &'a CosDictionary)>,
}

impl<'a> PageTree<'a> {
    /// Builds the page tree by walking from the `/Pages` root found in `catalog`.
    ///
    /// `store` is used to resolve indirect references. Returns an error if the
    /// tree cannot be traversed (missing root, corrupt node, etc.).
    pub fn new(catalog: &'a CosDictionary, store: &'a ObjectStore) -> PdfResult<Self> {
        let pages_ref = catalog
            .get(&CosName::pages())
            .and_then(|v| v.as_reference())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: "catalog missing /Pages reference".to_string(),
            })?;

        let mut pages = Vec::new();
        collect_pages(pages_ref, store, &mut pages, 0)?;

        Ok(Self { pages })
    }

    /// Returns the total number of pages.
    pub fn count(&self) -> usize {
        self.pages.len()
    }

    /// Returns `true` if there are no pages.
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Returns the page at the given 0-based index.
    pub fn get(&self, index: usize) -> Option<Page<'a>> {
        self.pages.get(index).map(|(id, d)| Page::new(*id, d, index))
    }

    /// Returns an iterator over all pages in order.
    pub fn iter(&self) -> impl Iterator<Item = Page<'a>> + '_ {
        self.pages
            .iter()
            .enumerate()
            .map(|(i, (id, d))| Page::new(*id, d, i))
    }
}

/// Recursively collects leaf page dictionaries from the page tree.
///
/// Max recursion depth is capped at 64 to prevent stack overflow on
/// degenerate or adversarial inputs.
fn collect_pages<'a>(
    node_id: ObjectId,
    store: &'a ObjectStore,
    out: &mut Vec<(ObjectId, &'a CosDictionary)>,
    depth: usize,
) -> PdfResult<()> {
    const MAX_DEPTH: usize = 64;
    if depth > MAX_DEPTH {
        return Err(PdfError::Parse {
            offset: None,
            context: "page tree depth exceeds maximum (64), possible cycle".to_string(),
        });
    }

    let obj = store.get(&node_id).ok_or_else(|| PdfError::Xref {
        object_id: Some(node_id),
    })?;

    let dict = obj.as_dictionary().ok_or_else(|| PdfError::Parse {
        offset: None,
        context: format!("page tree node {} is not a dictionary", node_id),
    })?;

    let node_type = dict.get_name(&CosName::type_name());

    match node_type.map(|n| n.as_bytes()) {
        Some(b"Page") => {
            // Leaf node: emit itself.
            out.push((node_id, dict));
        }
        Some(b"Pages") | None => {
            // Intermediate node: evaluate kids, but fail if we recurse too deep.
            let kids = dict
                .get(&CosName::kids())
                .and_then(|v| v.as_array())
                .ok_or_else(|| PdfError::Parse {
                    offset: None,
                    context: format!("Pages node {} missing /Kids array", node_id),
                })?;

            for kid in kids {
                let kid_ref = kid.as_reference().ok_or_else(|| PdfError::Parse {
                    offset: None,
                    context: "Kids array entry is not an indirect reference".to_string(),
                })?;
                collect_pages(kid_ref, store, out, depth + 1)?;
            }
        }
        Some(other) => {
            return Err(PdfError::Parse {
                offset: None,
                context: format!(
                    "unexpected /Type '{}' in page tree",
                    String::from_utf8_lossy(other)
                ),
            });
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
    use crate::ObjectStore;

    /// Builds a minimal store with a Catalog → Pages → [Page1, Page2] tree.
    fn two_page_store() -> (ObjectStore, ObjectId) {
        let mut store = ObjectStore::new();

        // Page 1
        let mut p1 = CosDictionary::new();
        p1.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Page".to_vec())));
        p1.insert(
            CosName::new(b"MediaBox".to_vec()),
            CosObject::Array(vec![
                CosObject::Integer(0),
                CosObject::Integer(0),
                CosObject::Integer(612),
                CosObject::Integer(792),
            ]),
        );
        let p1_id = ObjectId::new(3, 0);
        store.insert(p1_id, CosObject::Dictionary(p1));

        // Page 2
        let mut p2 = CosDictionary::new();
        p2.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Page".to_vec())));
        p2.insert(
            CosName::new(b"MediaBox".to_vec()),
            CosObject::Array(vec![
                CosObject::Integer(0),
                CosObject::Integer(0),
                CosObject::Integer(595),
                CosObject::Integer(842),
            ]),
        );
        let p2_id = ObjectId::new(4, 0);
        store.insert(p2_id, CosObject::Dictionary(p2));

        // Pages node
        let mut pages = CosDictionary::new();
        pages.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Pages".to_vec())));
        pages.insert(
            CosName::kids(),
            CosObject::Array(vec![
                CosObject::Reference(p1_id),
                CosObject::Reference(p2_id),
            ]),
        );
        pages.insert(CosName::count(), CosObject::Integer(2));
        let pages_id = ObjectId::new(2, 0);
        store.insert(pages_id, CosObject::Dictionary(pages));

        // Catalog
        let mut catalog = CosDictionary::new();
        catalog.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Catalog".to_vec())));
        catalog.insert(CosName::pages(), CosObject::Reference(pages_id));
        let catalog_id = ObjectId::new(1, 0);
        store.insert(catalog_id, CosObject::Dictionary(catalog));

        (store, catalog_id)
    }

    #[test]
    fn page_tree_count() {
        let (store, cat_id) = two_page_store();
        let cat = store.get(&cat_id).unwrap().as_dictionary().unwrap();
        let tree = PageTree::new(cat, &store).unwrap();
        assert_eq!(tree.count(), 2);
    }

    #[test]
    fn page_tree_get_by_index() {
        let (store, cat_id) = two_page_store();
        let cat = store.get(&cat_id).unwrap().as_dictionary().unwrap();
        let tree = PageTree::new(cat, &store).unwrap();

        let page0 = tree.get(0).unwrap();
        assert_eq!(page0.index, 0);
        let mb0 = page0.media_box().unwrap();
        assert_eq!(mb0.width(), 612.0);

        let page1 = tree.get(1).unwrap();
        assert_eq!(page1.index, 1);
        let mb1 = page1.media_box().unwrap();
        assert_eq!(mb1.width(), 595.0);
    }

    #[test]
    fn page_tree_iter() {
        let (store, cat_id) = two_page_store();
        let cat = store.get(&cat_id).unwrap().as_dictionary().unwrap();
        let tree = PageTree::new(cat, &store).unwrap();
        let pages: Vec<_> = tree.iter().collect();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].index, 0);
        assert_eq!(pages[1].index, 1);
    }

    #[test]
    fn page_tree_out_of_range() {
        let (store, cat_id) = two_page_store();
        let cat = store.get(&cat_id).unwrap().as_dictionary().unwrap();
        let tree = PageTree::new(cat, &store).unwrap();
        assert!(tree.get(2).is_none());
    }

    #[test]
    fn page_tree_missing_catalog_pages() {
        let store = ObjectStore::new();
        let mut cat = CosDictionary::new();
        cat.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Catalog".to_vec())));
        // No /Pages entry
        let result = PageTree::new(&cat, &store);
        assert!(result.is_err());
    }
}

