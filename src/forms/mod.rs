//!
//! Interactive Forms (AcroForm) module.
//!
//! Maps to `org.apache.pdfbox.pdmodel.interactive.form.*` in Java PDFBox.

pub mod field;
pub mod widget;
pub mod appearance;
pub mod flatten;
pub mod xfa;
pub mod export;
pub mod import;

pub use appearance::{generate_all_appearances, generate_field_appearance};
pub use field::{PdField, set_field_value};
pub use flatten::{flatten_all_fields, flatten_fields};
pub use widget::PdWidget;

use crate::ObjectStore;
use crate::cos::{CosDictionary, CosName, CosObject};

/// The interactive form of a document.
///
/// Maps to `PDAcroForm`.
#[derive(Debug, Clone)]
pub struct PdAcroForm<'a> {
    dict: &'a CosDictionary,
    store: &'a ObjectStore,
}

impl<'a> PdAcroForm<'a> {
    /// Creates a new `PdAcroForm` from the `/AcroForm` dictionary.
    pub fn new(dict: &'a CosDictionary, store: &'a ObjectStore) -> Self {
        Self { dict, store }
    }

    /// Returns the raw dictionary.
    pub fn dictionary(&self) -> &CosDictionary {
        self.dict
    }

    /// Returns all root fields in the form.
    pub fn fields(&self) -> Vec<PdField<'a>> {
        let mut fields = Vec::new();
        if let Some(CosObject::Array(kids)) = self.dict.get(&CosName::new(b"Fields".to_vec())) {
            for kid in kids {
                if let Some(kid_ref) = kid.as_reference() {
                    if let Some(obj) = self.store.get(&kid_ref) {
                        if let Some(field_dict) = obj.as_dictionary() {
                            fields.push(PdField::new(kid_ref, field_dict, self.store));
                        }
                    }
                }
            }
        }
        fields
    }

    /// Finds a field by its fully qualified name.
    pub fn get_field(&self, fully_qualified_name: &str) -> Option<PdField<'a>> {
        // Simple linear scan for now. True implementation should climb/descend.
        self.fields().into_iter().find(|f| f.fully_qualified_name() == fully_qualified_name)
    }
}
