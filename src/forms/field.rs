use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, ObjectStore};

/// Represents an interactive form field.
///
/// Maps to `PDField` in Java PDFBox.
#[derive(Debug, Clone)]
pub enum PdField<'a> {
    TextField { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
    CheckBox { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
    RadioButton { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
    ComboBox { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
    ListBox { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
    PushButton { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
    SignatureField { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
    Unknown { id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore },
}

impl<'a> PdField<'a> {
    pub fn new(id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore) -> Self {
        // Resolve field type (FT)
        // If not found directly, should climb Parent. For now, simple check.
        let ft = dict.get(&CosName::new(b"FT".to_vec())).and_then(|v| v.as_name()).map(|n| n.as_str());
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
            dict.insert(CosName::new(b"V".to_vec()), CosObject::String(string_value.as_bytes().to_vec()));
        }
    });

    // 2. Set NeedAppearances = true on the /AcroForm dictionary
    let acro_id = doc.catalog()
        .and_then(|c| c.get(&CosName::new(b"AcroForm".to_vec())))
        .and_then(|v| v.as_reference());

    if let Some(id) = acro_id {
        doc.mutate_object(id, |obj| {
            if let CosObject::Dictionary(dict) = obj {
                dict.insert(CosName::new(b"NeedAppearances".to_vec()), CosObject::Bool(true));
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
        CosObject::String(bytes) => {
            Some(String::from_utf8_lossy(bytes).to_string())
        }
        CosObject::Name(name) => {
            // For checkboxes/radio buttons, return the selected state name
            let s = name.as_str().unwrap_or("Off");
            if s == "Off" { None } else { Some(s.to_string()) }
        }
        CosObject::Array(arr) => {
            // Multi-select: join values with newlines
            let values: Vec<String> = arr.iter().filter_map(|item| {
                item.as_string().map(|s| String::from_utf8_lossy(s).to_string())
            }).collect();
            if values.is_empty() { None } else { Some(values.join("\n")) }
        }
        _ => None,
    }
}
