//! AcroForm wiring for digital signatures.
//!
//! Mirrors `acro_form.rs` in rust_pdf_signing.
//! Adds a /Sig field to the document's /AcroForm (creating one if absent).

use crate::Document;
use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use std::collections::BTreeMap;

/// Build or update the `/AcroForm` dictionary to include `widget_id` in `/Fields`.
///
/// Returns the new AcroForm object to be stored at `acroform_id`.
///
/// If the document already has an `/AcroForm`, its existing `/Fields` array
/// is preserved and `widget_id` is appended.
pub fn build_acroform(
    doc: &Document,
    widget_id: ObjectId,
    _acroform_id: ObjectId,
    _changed: &mut BTreeMap<ObjectId, CosObject>,
) -> CosObject {
    // Retrieve existing AcroForm if present
    let existing: Option<CosDictionary> = doc
        .catalog()
        .and_then(|cat| cat.get(&CosName::new(b"AcroForm")))
        .and_then(|v| match v {
            CosObject::Reference(r) => doc.objects.get(r)?.as_dictionary().cloned(),
            CosObject::Dictionary(d) => Some(d.clone()),
            _ => None,
        });

    let mut acroform = existing.unwrap_or_else(CosDictionary::new);

    // Build updated /Fields array
    let mut fields: Vec<CosObject> = acroform
        .get_array(&CosName::new(b"Fields"))
        .map(|arr| arr.to_vec())
        .unwrap_or_default();
    fields.push(CosObject::Reference(widget_id));

    acroform.set(CosName::new(b"Fields"), CosObject::Array(fields));
    // SigFlags: 3 = AppendOnly | SignaturesExist
    acroform.set(CosName::new(b"SigFlags"), CosObject::Integer(3));

    CosObject::Dictionary(acroform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
    use crate::Document;
    use std::collections::BTreeMap;

    fn doc_with_fields() -> Document {
        // Create a minimal document with an AcroForm
        let bytes = b"%PDF-1.7\n\
            1 0 obj<< /Type /Catalog /Pages 2 0 R /AcroForm 3 0 R >>\nendobj\n\
            2 0 obj<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
            3 0 obj<< /Fields [4 0 R] >>\nendobj\n\
            4 0 obj<< /FT /Btn /T (Sig1) >>\nendobj\n\
            xref\n0 5\n\
            0000000000 65535 f \n\
            0000000009 00000 n \n\
            0000000081 00000 n \n\
            0000000143 00000 n \n\
            0000000237 00000 n \n\
            trailer\n<< /Size 5 /Root 1 0 R >>\n\
            startxref\n316\n%%EOF";
        let (doc, _) = Document::load_lenient(bytes);
        doc
    }

    #[test]
    fn test_build_acroform_existing_acroform() {
        let doc = doc_with_fields();
        let widget_id = ObjectId::new(10, 0);
        let mut changed = BTreeMap::new();
        let result = build_acroform(&doc, widget_id, ObjectId::new(3, 0), &mut changed);
        let dict = result.as_dictionary().unwrap();
        let fields = dict.get(&CosName::new(b"Fields")).unwrap();
        let arr = fields.as_array().unwrap();
        assert_eq!(arr.len(), 2, "should have original field + new widget");
        assert_eq!(arr[1].as_reference(), Some(widget_id));
        let sigflags = dict.get(&CosName::new(b"SigFlags")).unwrap();
        assert_eq!(sigflags.as_integer(), Some(3));
    }

    #[test]
    fn test_build_acroform_no_existing_acroform() {
        let bytes = b"%PDF-1.7\n\
            1 0 obj<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
            xref\n0 3\n\
            0000000000 65535 f \n\
            0000000009 00000 n \n\
            0000000081 00000 n \n\
            trailer\n<< /Size 3 /Root 1 0 R >>\n\
            startxref\n155\n%%EOF";
        let (doc, _) = Document::load_lenient(bytes);
        let widget_id = ObjectId::new(5, 0);
        let mut changed = BTreeMap::new();
        let result = build_acroform(&doc, widget_id, ObjectId::new(10, 0), &mut changed);
        let dict = result.as_dictionary().unwrap();
        let fields = dict.get(&CosName::new(b"Fields")).unwrap();
        let arr = fields.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].as_reference(), Some(widget_id));
        assert_eq!(dict.get(&CosName::new(b"SigFlags")), Some(&CosObject::Integer(3)));
    }

    #[test]
    fn test_build_acroform_with_changed_fields() {
        let doc = doc_with_fields();
        let widget_id = ObjectId::new(10, 0);
        let mut changed = BTreeMap::new();
        let result = build_acroform(&doc, widget_id, ObjectId::new(3, 0), &mut changed);
        let dict = result.as_dictionary().unwrap();
        let fields = dict.get(&CosName::new(b"Fields")).unwrap();
        let arr = fields.as_array().unwrap();
        assert_eq!(arr.len(), 2, "should have original field + new widget");
    }

    #[test]
    fn test_build_acroform_result_type() {
        let bytes = b"%PDF-1.7\n\
            1 0 obj<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
            xref\n0 3\n\
            0000000000 65535 f \n\
            0000000009 00000 n \n\
            0000000081 00000 n \n\
            trailer\n<< /Size 3 /Root 1 0 R >>\n\
            startxref\n155\n%%EOF";
        let (doc, _) = Document::load_lenient(bytes);
        let mut changed = BTreeMap::new();
        let result = build_acroform(&doc, ObjectId::new(5, 0), ObjectId::new(10, 0), &mut changed);
        let dict = result.as_dictionary().expect("should be a dictionary");
        assert!(dict.contains_key(&CosName::new(b"Fields".to_vec())));
        assert!(dict.contains_key(&CosName::new(b"SigFlags".to_vec())));
    }
}
