use crate::{Document, PdfResult};
use crate::cos::{CosDictionary, CosName, CosObject};
use crate::parser::xref::XRefEntry;

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
    catalog.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Catalog".to_vec())));
    catalog.insert(CosName::pages(), CosObject::Reference(pages_id));
    new_doc.insert_object(catalog_id, CosObject::Dictionary(catalog));
    new_doc.xref.insert_if_absent(catalog_id, XRefEntry::InUse { offset: 0, generation: 0 });

    let tree = doc.pages()?;
    let mut kids = Vec::new();

    for &idx in page_indices {
        if let Some(page) = tree.get(idx) {
            let mut page_dict = page.dictionary().clone();
            let new_pid = new_doc.allocate_object_id();

            // Adjust parent to the new pages node
            page_dict.insert(CosName::new(b"Parent".to_vec()), CosObject::Reference(pages_id));

            // Note: In a complete implementation we need deep copying of all referenced objects,
            // or merging ObjectStores and remapping ObjectIDs.
            // For now, simpler extract for flat pages (assumes shared store or single pass deep copy).
            // A true deep copy mechanism will be required.
            // This is a minimal placeholder showing structure.

            new_doc.insert_object(new_pid, CosObject::Dictionary(page_dict));
            new_doc.xref.insert_if_absent(new_pid, XRefEntry::InUse { offset: 0, generation: 0 });
            kids.push(CosObject::Reference(new_pid));
        }
    }

    let mut pages = CosDictionary::new();
    pages.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Pages".to_vec())));
    pages.insert(CosName::count(), CosObject::Integer(kids.len() as i64));
    pages.insert(CosName::kids(), CosObject::Array(kids));

    new_doc.insert_object(pages_id, CosObject::Dictionary(pages));
    new_doc.xref.insert_if_absent(pages_id, XRefEntry::InUse { offset: 0, generation: 0 });

    // Copy all objects from the original document that are referenced.
    // For a simplistic extract, we can just copy the entire object store.
    for key in doc.objects.keys().cloned().collect::<Vec<_>>() {
        if let Some(obj) = doc.objects.get(&key) {
            if !new_doc.objects.get(&key).is_some() {
                 new_doc.insert_object(key, obj.clone());
                 new_doc.xref.insert_if_absent(key, XRefEntry::InUse { offset: 0, generation: 0 });
            }
        }
    }

    let size = new_doc.objects.max_object_number() + 1;
    new_doc.xref.trailer.insert(CosName::new(b"Size".to_vec()), CosObject::Integer(size as i64));

    Ok(new_doc)
}
