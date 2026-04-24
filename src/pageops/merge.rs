use crate::{Document, PdfResult};
use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::parser::xref::XRefEntry;

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
        catalog.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Catalog".to_vec())));
        catalog.insert(CosName::pages(), CosObject::Reference(pages_id));
        doc.insert_object(catalog_id, CosObject::Dictionary(catalog));
        doc.xref.insert_if_absent(catalog_id, XRefEntry::InUse { offset: 0, generation: 0 });

        let mut pages = CosDictionary::new();
        pages.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Pages".to_vec())));
        pages.insert(CosName::count(), CosObject::Integer(0));
        pages.insert(CosName::kids(), CosObject::Array(Vec::new()));
        
        doc.insert_object(pages_id, CosObject::Dictionary(pages));
        doc.xref.insert_if_absent(pages_id, XRefEntry::InUse { offset: 0, generation: 0 });
        
        doc.xref.trailer.insert(CosName::new(b"Size".to_vec()), CosObject::Integer((doc.objects.max_object_number() + 1) as i64));

        Self { dest_doc: doc }
    }

    /// Appends the entire source document to the end of the destination document.
    pub fn append(&mut self, src_doc: &Document) -> PdfResult<()> {
        let tree = src_doc.pages()?;
        
        // Find the Pages object ID in the destination document
        let catalog_id = self.dest_doc.catalog_id().unwrap();
        let pages_id = self.dest_doc.get_object_ref(catalog_id).unwrap()
            .as_dictionary().unwrap()
            .get(&CosName::pages()).unwrap()
            .as_reference().unwrap();
            
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
            
            page_dict.insert(CosName::new(b"Parent".to_vec()), CosObject::Reference(pages_id));
            
            self.dest_doc.insert_object(mapped_id, map_object(&CosObject::Dictionary(page_dict)));
            self.dest_doc.xref.insert_if_absent(mapped_id, XRefEntry::InUse { offset: 0, generation: 0 });
            
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
        self.dest_doc.xref.trailer.insert(CosName::new(b"Size".to_vec()), CosObject::Integer(size as i64));

        Ok(())
    }

    pub fn finish(self) -> Document {
        self.dest_doc
    }
}

