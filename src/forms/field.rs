use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, ObjectStore};

/// Represents an interactive form field.
///
/// Maps to `PDField` in Java PDFBox.
#[derive(Debug, Clone)]
pub enum PdField<'a> {
    TextField {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
    CheckBox {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
    RadioButton {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
    ComboBox {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
    ListBox {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
    PushButton {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
    SignatureField {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
    Unknown {
        id: ObjectId,
        dict: &'a CosDictionary,
        store: &'a ObjectStore,
    },
}

impl<'a> PdField<'a> {
    pub fn new(id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore) -> Self {
        // Resolve field type (FT)
        // If not found directly, should climb Parent. For now, simple check.
        let ft = dict
            .get(&CosName::new(b"FT".to_vec()))
            .and_then(|v| v.as_name())
            .map(|n| n.as_str());
        let flags = dict.get_int(&CosName::new(b"Ff".to_vec())).unwrap_or(0);

        match ft {
            Some(Some("Tx")) => Self::TextField { id, dict, store },
            Some(Some("Btn")) => {
                if (flags & 0x10000) != 0 {
                    Self::PushButton { id, dict, store }
                } else if (flags & 0x8000) != 0 {
                    Self::RadioButton { id, dict, store }
                } else {
                    Self::CheckBox { id, dict, store }
                }
            }
            Some(Some("Ch")) => {
                if (flags & 0x20000) != 0 {
                    Self::ComboBox { id, dict, store }
                } else {
                    Self::ListBox { id, dict, store }
                }
            }
            Some(Some("Sig")) => Self::SignatureField { id, dict, store },
            _ => Self::Unknown { id, dict, store },
        }
    }

    pub fn id(&self) -> ObjectId {
        match self {
            Self::TextField { id, .. } => *id,
            Self::CheckBox { id, .. } => *id,
            Self::RadioButton { id, .. } => *id,
            Self::ComboBox { id, .. } => *id,
            Self::ListBox { id, .. } => *id,
            Self::PushButton { id, .. } => *id,
            Self::SignatureField { id, .. } => *id,
            Self::Unknown { id, .. } => *id,
        }
    }

    pub fn dictionary(&self) -> &'a CosDictionary {
        match self {
            Self::TextField { dict, .. } => *dict,
            Self::CheckBox { dict, .. } => *dict,
            Self::RadioButton { dict, .. } => *dict,
            Self::ComboBox { dict, .. } => *dict,
            Self::ListBox { dict, .. } => *dict,
            Self::PushButton { dict, .. } => *dict,
            Self::SignatureField { dict, .. } => *dict,
            Self::Unknown { dict, .. } => *dict,
        }
    }

    /// Returns the fully qualified name (T entry).
    pub fn fully_qualified_name(&self) -> String {
        // Real implementation should climb looking for Parent T
        self.dictionary()
            .get(&CosName::new(b"T".to_vec()))
            .and_then(|v| v.as_string())
            .map(|s| {
                let parsed = String::from_utf8_lossy(s).into_owned();
                // Depending on string generation it might have parens if the parser included them
                parsed.trim_matches(|c| c == '(' || c == ')').to_string()
            })
            .unwrap_or_default()
    }

    /// Returns the value of the field (/V entry).
    pub fn value(&self) -> Option<&CosObject> {
        self.dictionary().get(&CosName::new(b"V".to_vec()))
    }
}

/// Helper function to set a field's value globally in the Document
/// and mark `NeedAppearances` = true so the PDF viewer will regenerate text.
pub fn set_field_value(doc: &mut Document, field_id: ObjectId, string_value: &str) {
    // 1. Update the field's /V object
    doc.mutate_object(field_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.insert(
                CosName::new(b"V".to_vec()),
                CosObject::String(string_value.as_bytes().to_vec()),
            );
        }
    });

    // 2. Set NeedAppearances = true on the /AcroForm dictionary
    let acro_id = doc
        .catalog()
        .and_then(|c| c.get(&CosName::new(b"AcroForm".to_vec())))
        .and_then(|v| v.as_reference());

    if let Some(id) = acro_id {
        doc.mutate_object(id, |obj| {
            if let CosObject::Dictionary(dict) = obj {
                dict.insert(
                    CosName::new(b"NeedAppearances".to_vec()),
                    CosObject::Bool(true),
                );
            }
        });
    }
}

/// Extracts a field's current value as an exportable string.
///
/// Handles `/V` strings, names (for checkboxes/radios), and arrays (for
/// multi-select list boxes).
pub fn get_field_value_for_export(field_dict: &CosDictionary) -> Option<String> {
    let v = field_dict.get(&CosName::new(b"V".to_vec()))?;
    match v {
        CosObject::String(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
        CosObject::Name(name) => {
            // For checkboxes/radio buttons, return the selected state name
            let s = name.as_str().unwrap_or("Off");
            if s == "Off" {
                None
            } else {
                Some(s.to_string())
            }
        }
        CosObject::Array(arr) => {
            // Multi-select: join values with newlines
            let values: Vec<String> = arr
                .iter()
                .filter_map(|item| {
                    item.as_string()
                        .map(|s| String::from_utf8_lossy(s).to_string())
                })
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(values.join("\n"))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::ObjectId;
    use crate::cos::{CosDictionary, CosName, CosObject};
    use crate::forms::PdAcroForm;

    fn make_test_doc_with_field(ft: &[u8], ff_flags: i64) -> Document {
        let cat_id = ObjectId::new(1, 0);
        let pages_id = ObjectId::new(2, 0);
        let page_id = ObjectId::new(3, 0);
        let acro_id = ObjectId::new(4, 0);
        let field_id = ObjectId::new(5, 0);

        let mut doc = Document::empty();

        // AcroForm
        let mut acro_dict = CosDictionary::new();
        acro_dict.insert(
            CosName::new(b"Fields".to_vec()),
            CosObject::Array(vec![CosObject::Reference(field_id)]),
        );
        doc.insert_object(acro_id, CosObject::Dictionary(acro_dict));

        // Field
        let mut field_dict = CosDictionary::new();
        if !ft.is_empty() {
            field_dict.insert(
                CosName::new(b"FT".to_vec()),
                CosObject::Name(CosName::new(ft.to_vec())),
            );
            field_dict.insert(
                CosName::new(b"T".to_vec()),
                CosObject::String(b"TestField".to_vec()),
            );
            field_dict.insert(
                CosName::new(b"V".to_vec()),
                CosObject::String(b"Hello".to_vec()),
            );
        }
        if ff_flags != 0 {
            field_dict.insert(CosName::new(b"Ff".to_vec()), CosObject::Integer(ff_flags));
        }
        doc.insert_object(field_id, CosObject::Dictionary(field_dict));

        // Catalog
        doc.insert_object(
            cat_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Catalog".to_vec())),
                );
                d.insert(
                    CosName::new(b"Pages".to_vec()),
                    CosObject::Reference(pages_id),
                );
                d.insert(
                    CosName::new(b"AcroForm".to_vec()),
                    CosObject::Reference(acro_id),
                );
                d
            }),
        );
        // Pages
        doc.insert_object(
            pages_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Pages".to_vec())),
                );
                d.insert(CosName::new(b"Count".to_vec()), CosObject::Integer(1));
                d.insert(
                    CosName::new(b"Kids".to_vec()),
                    CosObject::Array(vec![CosObject::Reference(page_id)]),
                );
                d
            }),
        );
        // Page
        doc.insert_object(
            page_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Page".to_vec())),
                );
                d.insert(
                    CosName::new(b"Parent".to_vec()),
                    CosObject::Reference(pages_id),
                );
                d.insert(
                    CosName::new(b"MediaBox".to_vec()),
                    CosObject::Array(vec![
                        CosObject::Integer(0),
                        CosObject::Integer(0),
                        CosObject::Integer(612),
                        CosObject::Integer(792),
                    ]),
                );
                d
            }),
        );
        doc.xref
            .trailer
            .insert(CosName::new(b"Root".to_vec()), CosObject::Reference(cat_id));
        doc
    }

    #[test]
    fn test_field_type_text() {
        let doc = make_test_doc_with_field(b"Tx", 0);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::TextField { .. }));
    }

    #[test]
    fn test_field_type_checkbox() {
        let doc = make_test_doc_with_field(b"Btn", 0);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::CheckBox { .. }));
    }

    #[test]
    fn test_field_type_pushbutton() {
        let doc = make_test_doc_with_field(b"Btn", 0x10000);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::PushButton { .. }));
    }

    #[test]
    fn test_field_type_radio() {
        let doc = make_test_doc_with_field(b"Btn", 0x8000);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::RadioButton { .. }));
    }

    #[test]
    fn test_field_type_combobox() {
        let doc = make_test_doc_with_field(b"Ch", 0x20000);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::ComboBox { .. }));
    }

    #[test]
    fn test_field_type_listbox() {
        let doc = make_test_doc_with_field(b"Ch", 0);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::ListBox { .. }));
    }

    #[test]
    fn test_field_type_signature() {
        let doc = make_test_doc_with_field(b"Sig", 0);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::SignatureField { .. }));
    }

    #[test]
    fn test_field_type_unknown() {
        let doc = make_test_doc_with_field(b"Xxx", 0);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert!(matches!(f, PdField::Unknown { .. }));
    }

    #[test]
    fn test_field_name() {
        let doc = make_test_doc_with_field(b"Tx", 0);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        assert_eq!(f.fully_qualified_name(), "TestField");
    }

    #[test]
    fn test_field_value_accessor() {
        let doc = make_test_doc_with_field(b"Tx", 0);
        let field_id = ObjectId::new(5, 0);
        let dict = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let f = PdField::new(field_id, dict, &doc.objects);
        let v = f.value();
        assert!(v.is_some());
        assert_eq!(v.unwrap().as_string(), Some(&b"Hello"[..]));
    }

    #[test]
    fn test_set_field_value() {
        let mut doc = make_test_doc_with_field(b"Tx", 0);
        let field_id = ObjectId::new(5, 0);
        let acro_id = ObjectId::new(4, 0);
        set_field_value(&mut doc, field_id, "NewVal");

        let fd = doc
            .objects
            .get(&field_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        assert_eq!(
            fd.get(&CosName::new(b"V".to_vec()))
                .and_then(|o| o.as_string()),
            Some(&b"NewVal"[..])
        );
        let ad = doc
            .objects
            .get(&acro_id)
            .and_then(|o| o.as_dictionary())
            .unwrap();
        assert_eq!(
            ad.get(&CosName::new(b"NeedAppearances".to_vec())),
            Some(&CosObject::Bool(true))
        );
    }

    #[test]
    fn test_export_string() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"V".to_vec()),
            CosObject::String(b"Hello".to_vec()),
        );
        assert_eq!(get_field_value_for_export(&dict).as_deref(), Some("Hello"));
    }

    #[test]
    fn test_export_name_selected() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"V".to_vec()),
            CosObject::Name(CosName::new(b"Yes".to_vec())),
        );
        assert_eq!(get_field_value_for_export(&dict).as_deref(), Some("Yes"));
    }

    #[test]
    fn test_export_name_off() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"V".to_vec()),
            CosObject::Name(CosName::new(b"Off".to_vec())),
        );
        assert!(get_field_value_for_export(&dict).is_none());
    }

    #[test]
    fn test_export_array() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"V".to_vec()),
            CosObject::Array(vec![
                CosObject::String(b"A".to_vec()),
                CosObject::String(b"B".to_vec()),
            ]),
        );
        assert_eq!(get_field_value_for_export(&dict).as_deref(), Some("A\nB"));
    }

    #[test]
    fn test_export_missing() {
        let dict = CosDictionary::new();
        assert!(get_field_value_for_export(&dict).is_none());
    }

    #[test]
    fn test_acro_form_fields() {
        let doc = make_test_doc_with_field(b"Tx", 0);
        let cat = doc.catalog().unwrap();
        let ad = cat
            .get(&CosName::new(b"AcroForm".to_vec()))
            .and_then(|v| doc.objects.resolve(v))
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let acro = PdAcroForm::new(ad, &doc.objects);
        assert_eq!(acro.fields().len(), 1);
    }

    #[test]
    fn test_acro_form_get_field() {
        let doc = make_test_doc_with_field(b"Tx", 0);
        let cat = doc.catalog().unwrap();
        let ad = cat
            .get(&CosName::new(b"AcroForm".to_vec()))
            .and_then(|v| doc.objects.resolve(v))
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let acro = PdAcroForm::new(ad, &doc.objects);
        assert!(acro.get_field("TestField").is_some());
    }

    #[test]
    fn test_acro_form_no_xfa() {
        let doc = make_test_doc_with_field(b"Tx", 0);
        let cat = doc.catalog().unwrap();
        let ad = cat
            .get(&CosName::new(b"AcroForm".to_vec()))
            .and_then(|v| doc.objects.resolve(v))
            .and_then(|o| o.as_dictionary())
            .unwrap();
        let acro = PdAcroForm::new(ad, &doc.objects);
        assert!(!acro.has_xfa());
    }
}
