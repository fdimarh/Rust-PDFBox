//!
//! Interactive Forms (AcroForm) module.
//!
//! Maps to `org.apache.pdfbox.pdmodel.interactive.form.*` in Java PDFBox.

pub mod appearance;
pub mod export;
pub mod field;
pub mod flatten;
pub mod import;
pub mod widget;
pub mod xfa;

pub use appearance::{generate_all_appearances, generate_field_appearance};
pub use export::{export_fdf, export_xfdf};
pub use field::{PdField, get_field_value_for_export, set_field_value};
pub use flatten::{flatten_all_fields, flatten_fields};
pub use import::{import_fdf, import_xfdf};
pub use widget::PdWidget;
pub use xfa::{XfaForm, XfaPacket};

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
        self.fields()
            .into_iter()
            .find(|f| f.fully_qualified_name() == fully_qualified_name)
    }

    /// Returns true if the AcroForm contains an `/XFA` entry.
    pub fn has_xfa(&self) -> bool {
        self.dict.contains_key(&CosName::new(b"XFA".to_vec()))
    }

    /// Returns a read-only XFA view when `/XFA` is present.
    pub fn xfa(&self) -> Option<XfaForm> {
        XfaForm::from_acro_form_dict(self.dict, self.store)
    }

    /// Returns true for hybrid forms (both AcroForm fields and XFA payload).
    pub fn is_hybrid_xfa(&self) -> bool {
        self.has_xfa() && !self.fields().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
    use crate::ObjectStore;

    #[test]
    fn pd_acro_form_empty_fields() {
        let store = ObjectStore::new();
        let dict = CosDictionary::new();
        let form = PdAcroForm::new(&dict, &store);
        assert!(form.fields().is_empty());
    }

    #[test]
    fn pd_acro_form_has_xfa_true() {
        let mut dict = CosDictionary::new();
        dict.set(CosName::new(b"XFA".to_vec()), CosObject::Null);
        let store = ObjectStore::new();
        let form = PdAcroForm::new(&dict, &store);
        assert!(form.has_xfa());
    }

    #[test]
    fn pd_acro_form_has_xfa_false() {
        let dict = CosDictionary::new();
        let store = ObjectStore::new();
        let form = PdAcroForm::new(&dict, &store);
        assert!(!form.has_xfa());
    }

    #[test]
    fn pd_acro_form_get_field_not_found_empty() {
        let store = ObjectStore::new();
        let dict = CosDictionary::new();
        let form = PdAcroForm::new(&dict, &store);
        let field = form.get_field("NonExistent");
        assert!(field.is_none());
    }

    #[test]
    fn pd_acro_form_is_hybrid_xfa_false_no_fields() {
        let store = ObjectStore::new();
        let mut dict = CosDictionary::new();
        dict.set(CosName::new(b"XFA".to_vec()), CosObject::Null);
        let form = PdAcroForm::new(&dict, &store);
        assert!(!form.is_hybrid_xfa());
    }

    #[test]
    fn pd_acro_form_is_hybrid_xfa_false_no_xfa() {
        let store = ObjectStore::new();
        let mut dict = CosDictionary::new();
        // With Fields but no XFA -> not hybrid
        dict.set(
            CosName::new(b"Fields".to_vec()),
            CosObject::Array(vec![CosObject::Reference(ObjectId::new(1, 0))]),
        );
        let form = PdAcroForm::new(&dict, &store);
        assert!(!form.is_hybrid_xfa());
    }

    #[test]
    fn pd_acro_form_dictionary_accessor() {
        let mut dict = CosDictionary::new();
        dict.set(CosName::new(b"Test".to_vec()), CosObject::Integer(42));
        let store = ObjectStore::new();
        let form = PdAcroForm::new(&dict, &store);
        let d = form.dictionary();
        assert_eq!(
            d.get(&CosName::new(b"Test".to_vec()))
                .and_then(|v| v.as_integer()),
            Some(42)
        );
    }
}
