//!
//! Form flattening — burns interactive form fields into page content and
//! removes the AcroForm structure.
//!
//! After flattening, form field values appear as regular page content (text,
//! graphics) and the interactive form controls (widget annotations, field
//! dictionaries, AcroForm catalog entry) are removed.
//!
//! Maps to Java PDFBox's `flatten()` on `PDAcroForm`.

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::forms::appearance::generate_field_appearance;
use crate::{Document, PdfResult};

/// Flattens a specific set of form fields into their respective pages.
///
/// For each field:
/// 1. Generates an appearance stream if one doesn't exist
/// 2. Merges the appearance Form XObject content into the page's content stream
/// 3. Removes the widget annotation from the page's `/Annots` array
/// 4. Removes the field from the `/AcroForm`
///
/// After all fields are flattened, if no fields remain, the `/AcroForm` entry
/// is removed from the document catalog.
///
/// # Arguments
///
/// * `doc` - Mutable reference to the document.
/// * `field_ids` - Object IDs of the form fields to flatten.
pub fn flatten_fields(doc: &mut Document, field_ids: &[ObjectId]) -> PdfResult<()> {
    if field_ids.is_empty() {
        return Ok(());
    }

    // Generate appearances for all fields first
    for field_id in field_ids {
        let _ = generate_field_appearance(doc, *field_id);
    }

    // Track which AcroForm fields to remove
    let mut fields_to_remove: Vec<ObjectId> = Vec::new();

    for field_id in field_ids {
        // Get the widget's appearance and page info
        let field_dict = doc
            .get_object_ref(*field_id)
            .and_then(|o| o.as_dictionary())
            .cloned();

        let field_dict = match field_dict {
            Some(d) => d,
            None => continue,
        };

        // Find the page this widget belongs to (via /P entry)
        let page_id = field_dict
            .get(&CosName::new(b"P".to_vec()))
            .and_then(|v| v.as_reference());

        // Get the appearance stream (AP/N Form XObject)
        let ap_content = field_dict
            .get(&CosName::new(b"AP".to_vec()))
            .and_then(|v| v.as_dictionary())
            .and_then(|ap| ap.get(&CosName::new(b"N".to_vec())))
            .and_then(|n| {
                // N can be a reference to a Form XObject or a dictionary of named appearances
                if let Some(form_ref) = n.as_reference() {
                    doc.get_object_ref(form_ref)
                        .and_then(|o| o.as_stream())
                        .map(|s| (form_ref, s.data.clone()))
                } else if let Some(n_dict) = n.as_dictionary() {
                    // For checkboxes/radios with sub-dictionary, get the "on" state
                    // Use the first non-Off entry
                    for (key, val) in n_dict.iter() {
                        if key.as_bytes() != b"Off" {
                            if let Some(form_ref) = val.as_reference() {
                                if let Some(content) = doc
                                    .get_object_ref(form_ref)
                                    .and_then(|o| o.as_stream())
                                    .map(|s| (form_ref, s.data.clone()))
                                {
                                    return Some(content);
                                }
                            }
                        }
                    }
                    None
                } else {
                    None
                }
            });

        // Get the widget rectangle (Rect) for positioning
        let rect = field_dict
            .get(&CosName::new(b"Rect".to_vec()))
            .and_then(|v| v.as_array())
            .map(|arr| {
                let llx = arr.first().and_then(|v| v.as_real()).unwrap_or(0.0);
                let lly = arr.get(1).and_then(|v| v.as_real()).unwrap_or(0.0);
                let urx = arr.get(2).and_then(|v| v.as_real()).unwrap_or(0.0);
                let ury = arr.get(3).and_then(|v| v.as_real()).unwrap_or(0.0);
                (llx, lly, urx, ury)
            });

        // Merge appearance content into the page's content stream
        if let (Some(page_id), Some((_form_ref, content)), Some((llx, lly, urx, ury))) =
            (page_id, ap_content, rect)
        {
            if !content.is_empty() {
                let w = urx - llx;
                let h = ury - lly;
                merge_into_page_content(doc, page_id, &content, llx, lly, w, h)?;
            }
        }

        // Remove the widget annotation from the page's /Annots
        if let Some(page_id) = page_id {
            remove_widget_from_page(doc, page_id, *field_id);
        }

        fields_to_remove.push(*field_id);
    }

    // Remove fields from AcroForm
    if !fields_to_remove.is_empty() {
        remove_fields_from_acroform(doc, &fields_to_remove);
    }

    // If AcroForm is now empty, remove it from catalog
    cleanup_acroform(doc);

    Ok(())
}

/// Merges content bytes into a page's content stream, wrapping them in a
/// transformation so they appear at the correct position.
///
/// The content is appended after wrapping in `q`/`Q` with a `cm` transform.
fn merge_into_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    content: &[u8],
    llx: f64,
    lly: f64,
    width: f64,
    height: f64,
) -> PdfResult<()> {
    // Build the wrapper content: q ...transform... [original content] Q
    let mut wrapper = Vec::new();
    wrapper.extend_from_slice(b"q\n");
    // Transform to position the appearance at the widget's rect
    wrapper.extend_from_slice(format!("{} 0 0 {} {} {} cm\n", width, height, llx, lly).as_bytes());
    wrapper.extend_from_slice(content);
    wrapper.extend_from_slice(b"\nQ\n");

    // Append to existing content or create new content stream
    let existing_content = doc
        .get_object_ref(page_id)
        .and_then(|o| o.as_dictionary())
        .and_then(|d| d.get(&CosName::new(b"Contents".to_vec())))
        .cloned();

    match existing_content {
        Some(CosObject::Reference(content_id)) => {
            // Append to existing single content stream
            doc.mutate_object(content_id, |obj| {
                if let CosObject::Stream(stream) = obj {
                    stream.data.extend_from_slice(&wrapper);
                    stream.dictionary.insert(CosName::new(b"Length".to_vec()), CosObject::Integer(stream.data.len() as i64));
                }
            });
        }
        Some(CosObject::Array(content_refs)) => {
            // Append to the last content stream in the array
            if let Some(last_ref) = content_refs.last().and_then(|v| v.as_reference()) {
                doc.mutate_object(last_ref, |obj| {
                    if let CosObject::Stream(stream) = obj {
                        stream.data.extend_from_slice(&wrapper);
                        stream.dictionary.insert(
                            CosName::new(b"Length".to_vec()),
                            CosObject::Integer(stream.data.len() as i64),
                        );
                    }
                });
            }
        }
        None | Some(_) => {
            // Create a new content stream for the page
            let content_id = doc.allocate_object_id();
            let mut dict = CosDictionary::new();
            dict.insert(
                CosName::new(b"Length".to_vec()),
                CosObject::Integer(wrapper.len() as i64),
            );
            let stream = crate::cos::CosStream::new(dict, wrapper);
            doc.insert_object(content_id, CosObject::Stream(stream));
            doc.xref.insert_if_absent(
                content_id,
                crate::parser::xref::XRefEntry::InUse {
                    offset: 0,
                    generation: 0,
                },
            );

            doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(page_dict) = obj {
                    page_dict
                        .insert(CosName::new(b"Contents".to_vec()), CosObject::Reference(content_id));
                }
            });
        }
    }

    Ok(())
}

/// Removes a widget annotation from a page's `/Annots` array.
fn remove_widget_from_page(doc: &mut Document, page_id: ObjectId, widget_id: ObjectId) {
    doc.mutate_object(page_id, |obj| {
        if let CosObject::Dictionary(page_dict) = obj {
            if let Some(CosObject::Array(annots)) = page_dict.get(&CosName::new(b"Annots".to_vec())) {
                let new_annots: Vec<CosObject> = annots
                    .iter()
                    .filter(|a| {
                        if let Some(ref_id) = a.as_reference() {
                            ref_id != widget_id
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect();
                if new_annots.is_empty() {
                    page_dict.remove(&CosName::new(b"Annots".to_vec()));
                } else {
                    page_dict.insert(CosName::new(b"Annots".to_vec()), CosObject::Array(new_annots));
                }
            }
        }
    });
}

/// Removes specific field references from the AcroForm's `/Fields` array.
fn remove_fields_from_acroform(doc: &mut Document, field_ids: &[ObjectId]) {
    let acro_id = doc
        .catalog()
        .and_then(|c| c.get(&CosName::new(b"AcroForm".to_vec())))
        .and_then(|v| v.as_reference());

    if let Some(acro_id) = acro_id {
        doc.mutate_object(acro_id, |obj| {
            if let CosObject::Dictionary(acro_dict) = obj {
                if let Some(CosObject::Array(fields)) = acro_dict.get(&CosName::new(b"Fields".to_vec())) {
                    let new_fields: Vec<CosObject> = fields
                        .iter()
                        .filter(|f| {
                            if let Some(ref_id) = f.as_reference() {
                                !field_ids.contains(&ref_id)
                            } else {
                                true
                            }
                        })
                        .cloned()
                        .collect();
                    if new_fields.is_empty() {
                        acro_dict.remove(&CosName::new(b"Fields".to_vec()));
                    } else {
                        acro_dict.insert(CosName::new(b"Fields".to_vec()), CosObject::Array(new_fields));
                    }
                }
            }
        });
    }
}

/// Removes the `/AcroForm` from the catalog if the Fields array is empty
/// or missing.
fn cleanup_acroform(doc: &mut Document) {
    let catalog_id = match doc.catalog_id() {
        Some(id) => id,
        None => return,
    };

    let should_remove = doc
        .get_object_ref(catalog_id)
        .and_then(|o| o.as_dictionary())
        .and_then(|cat| cat.get(&CosName::new(b"AcroForm".to_vec())))
        .and_then(|af| af.as_reference())
        .and_then(|af_id| doc.get_object_ref(af_id))
        .and_then(|o| o.as_dictionary())
        .map(|acro| {
            // Remove if no Fields, or Fields is an empty array
            match acro.get(&CosName::new(b"Fields".to_vec())) {
                None => true,
                Some(CosObject::Array(arr)) => arr.is_empty(),
                _ => false,
            }
        })
        .unwrap_or(false);

    if should_remove {
        doc.mutate_object(catalog_id, |obj| {
            if let CosObject::Dictionary(cat_dict) = obj {
                cat_dict.remove(&CosName::new(b"AcroForm".to_vec()));
            }
        });
    }
}

/// Flattens all fields in the document's AcroForm.
///
/// Convenience wrapper that collects all root field IDs from the AcroForm
/// and flattens them.
pub fn flatten_all_fields(doc: &mut Document) -> PdfResult<()> {
    let field_ids: Vec<ObjectId> = {
        let catalog = match doc.catalog() {
            Some(c) => c.clone(),
            None => return Ok(()),
        };

        let acro_dict = catalog
            .get(&CosName::new(b"AcroForm".to_vec()))
            .and_then(|v| v.as_reference())
            .and_then(|id| doc.get_object_ref(id))
            .and_then(|o| o.as_dictionary())
            .cloned();

        let acro_dict = match acro_dict {
            Some(d) => d,
            None => return Ok(()),
        };

        let mut ids = Vec::new();
        if let Some(CosObject::Array(fields)) = acro_dict.get(&CosName::new(b"Fields".to_vec())) {
            for field in fields {
                if let Some(ref_id) = field.as_reference() {
                    ids.push(ref_id);
                }
            }
        }
        ids
    };

    if field_ids.is_empty() {
        return Ok(());
    }

    flatten_fields(doc, &field_ids)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosName, ObjectId};
    use crate::Document;

    fn create_doc_with_acroform() -> Document {
        // Create a document with a single page and a minimal AcroForm
        let bytes = b"%PDF-1.7\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [5 0 R] >>\nendobj\n\
            4 0 obj\n<< /Fields [5 0 R] /DA (/Helv 10 Tf 0 g) >>\nendobj\n\
            5 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Tx /T (test) /Rect [100 700 300 720] /P 3 0 R /V (hello) >>\nendobj\n\
            xref\n0 6\n0000000000 65535 f \n0000000009 00000 n \n0000000081 00000 n \n\
            0000000143 00000 n \n0000000237 00000 n \n0000000299 00000 n \n\
            trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n419\n%%EOF";
        let (doc, _report) = Document::load_lenient(bytes);
        doc
    }

    #[test]
    fn test_remove_widget_from_page() {
        let mut doc = create_doc_with_acroform();
        let widget_id = ObjectId::new(5, 0);

        // Verify annots exists before
        let page = doc.pages().unwrap().get(0).unwrap();
        let page_dict = page.dictionary().clone();
        assert!(page_dict.get(&CosName::new(b"Annots".to_vec())).is_some());

        remove_widget_from_page(&mut doc, ObjectId::new(3, 0), widget_id);

        // Verify annots is removed
        let page = doc.pages().unwrap().get(0).unwrap();
        let annots = page.dictionary().get(&CosName::new(b"Annots".to_vec()));
        assert!(annots.is_none(), "Annots should be removed after removing last widget");
    }

    #[test]
    fn test_remove_fields_from_acroform() {
        let mut doc = create_doc_with_acroform();
        let field_id = ObjectId::new(5, 0);

        remove_fields_from_acroform(&mut doc, &[field_id]);

        let catalog = doc.catalog().unwrap().clone();
        let acro_id = catalog.get(&CosName::new(b"AcroForm".to_vec()))
            .and_then(|v| v.as_reference())
            .unwrap();
        let acro = doc.get_object_ref(acro_id).unwrap().as_dictionary().unwrap();
        assert!(acro.get(&CosName::new(b"Fields".to_vec())).is_none(),
            "Fields should be removed");
    }

    #[test]
    fn test_cleanup_acroform_empty_fields() {
        let mut doc = create_doc_with_acroform();
        let field_id = ObjectId::new(5, 0);

        remove_fields_from_acroform(&mut doc, &[field_id]);
        cleanup_acroform(&mut doc);

        let catalog = doc.catalog().unwrap().clone();
        let acro = catalog.get(&CosName::new(b"AcroForm".to_vec()));
        assert!(acro.is_none(), "AcroForm should be removed from catalog when empty");
    }

    #[test]
    fn test_flatten_all_fields_no_acroform() {
        // Document without AcroForm should not panic
        let bytes = b"%PDF-1.7\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
            xref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000037 00000 n \n\
            0000000090 00000 n \ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n143\n%%EOF";
        let (mut doc, _report) = Document::load_lenient(bytes);
        let result = flatten_all_fields(&mut doc);
        assert!(result.is_ok());
    }
}
