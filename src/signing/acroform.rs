//! AcroForm wiring for digital signatures.
//!
//! Mirrors `acro_form.rs` in rust_pdf_signing.
//! Adds a /Sig field to the document's /AcroForm (creating one if absent).

use std::collections::BTreeMap;
use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::Document;

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
    let existing: Option<CosDictionary> = doc.catalog()
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

