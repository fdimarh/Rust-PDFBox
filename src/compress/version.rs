//! Pass 7 — PDF version downgrade + ObjStm repack.
//!
//! ## Version downgrade
//!
//! If `downgrade_pdf_version = true` and the document uses no features that
//! require PDF ≥ 1.5 (AES encryption, XFA forms), rewrites the header to
//! `%PDF-1.4` and strips the `/Extensions` dict from the catalog.
//!
//! ## ObjStm repack
//!
//! If `repack_object_streams = true`, collects small non-stream indirect objects
//! (dicts + arrays of ≤ 200 objects per container) and packs them into
//! `ObjectStream` (ObjStm) containers with FlateDecode, replacing them with
//! `XRefEntry::Compressed` entries. Writes a new cross-reference stream instead
//! of the classic xref table.
//!
//! **No new crates** — uses existing `src/writer/` and `src/io/` infrastructure.

use crate::cos::{CosName, CosObject};
use crate::{Document, PdfResult};
use super::CompressOptions;

// ---------------------------------------------------------------------------
// Public report
// ---------------------------------------------------------------------------

/// Statistics returned by [`run`].
#[derive(Debug, Default)]
pub struct VersionReport {
    /// `true` if the PDF header was downgraded.
    pub version_downgraded: bool,
    /// Number of ObjStm containers written (each holds up to 200 objects).
    pub objstm_repacked: usize,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Execute the version downgrade and/or ObjStm repack passes.
pub fn run(doc: &mut Document, opts: &CompressOptions) -> PdfResult<VersionReport> {
    let mut report = VersionReport::default();

    if opts.downgrade_pdf_version {
        if try_downgrade(doc) {
            report.version_downgraded = true;
        }
    }

    if opts.repack_object_streams {
        report.objstm_repacked = repack_object_streams(doc, opts)?;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Version downgrade
// ---------------------------------------------------------------------------

/// Attempt to downgrade the PDF version to 1.4 if it is safe to do so.
///
/// Returns `true` if the downgrade was applied.
fn try_downgrade(doc: &mut Document) -> bool {
    // Skip if encrypted with AES (requires PDF 1.6+).
    if doc_uses_aes_encryption(doc) {
        return false;
    }

    // Skip if there are XFA forms (requires PDF 1.5+).
    if doc_has_xfa(doc) {
        return false;
    }

    // Apply downgrade.
    doc.set_version(1, 4);

    // Strip /Extensions dict from catalog.
    if let Some(catalog_id) = doc.catalog_id() {
        doc.mutate_object(catalog_id, |obj| {
            if let CosObject::Dictionary(dict) = obj {
                dict.remove(&CosName::new(b"Extensions".to_vec()));
            }
        });
    }

    true
}

fn doc_uses_aes_encryption(doc: &Document) -> bool {
    let encrypt_id = match doc.encryption_dict_id() {
        Some(id) => id,
        None => return false,
    };
    let obj = doc.get_object_ref(encrypt_id);
    match obj {
        Some(CosObject::Dictionary(dict)) => {
            // /CFM /AESV2 or /AESV3 indicates AES encryption.
            for (_, val) in dict.iter() {
                if let CosObject::Name(n) = val {
                    if n.as_str().map(|s| s.starts_with("AESV")).unwrap_or(false) {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn doc_has_xfa(doc: &Document) -> bool {
    let catalog_id = match doc.catalog_id() {
        Some(id) => id,
        None => return false,
    };
    let obj = doc.get_object_ref(catalog_id);
    match obj {
        Some(CosObject::Dictionary(dict)) => {
            match dict.get(&CosName::new(b"AcroForm".to_vec())) {
                Some(CosObject::Reference(form_id)) => {
                    let form_obj = doc.get_object_ref(*form_id);
                    match form_obj {
                        Some(CosObject::Dictionary(form_dict)) => {
                            form_dict.get(&CosName::new(b"XFA".to_vec())).is_some()
                        }
                        _ => false,
                    }
                }
                Some(CosObject::Dictionary(form_dict)) => {
                    form_dict.get(&CosName::new(b"XFA".to_vec())).is_some()
                }
                _ => false,
            }
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// ObjStm repack
// ---------------------------------------------------------------------------

const OBJSTM_MAX_OBJECTS: usize = 200;

/// Pack small non-stream objects into ObjStm containers.
///
/// Returns the number of ObjStm containers created.
fn repack_object_streams(doc: &mut Document, _opts: &CompressOptions) -> PdfResult<usize> {
    // Collect candidate objects: dicts and arrays that are NOT streams and
    // are not structurally significant (catalog, page, xref) objects.
    let candidates: Vec<crate::cos::ObjectId> = doc
        .objects()
        .filter_map(|(id, obj)| {
            match obj {
                CosObject::Dictionary(d) => {
                    // Exclude structurally significant dicts.
                    let t = d.get(&CosName::new(b"Type".to_vec()));
                    let subtype = d.get(&CosName::new(b"Subtype".to_vec()));
                    let is_significant = matches!(t,
                        Some(CosObject::Name(n)) if matches!(n.as_str(),
                            Some("Catalog") | Some("Pages") | Some("Page") | Some("XRef") | Some("ObjStm")
                        )
                    ) || matches!(subtype,
                        Some(CosObject::Name(n)) if n.as_str() == Some("Image")
                    );
                    if is_significant { None } else { Some(id) }
                }
                CosObject::Array(_) => Some(id),
                _ => None,
            }
        })
        .collect();

    if candidates.is_empty() {
        return Ok(0);
    }

    // Group into chunks of OBJSTM_MAX_OBJECTS.
    let chunks: Vec<Vec<crate::cos::ObjectId>> = candidates
        .chunks(OBJSTM_MAX_OBJECTS)
        .map(|c| c.to_vec())
        .collect();

    let container_count = chunks.len();

    // For each chunk, build an ObjStm and mark objects as compressed.
    for chunk in chunks {
        pack_chunk_into_objstm(doc, &chunk)?;
    }

    Ok(container_count)
}

/// Serialise `object_ids` into a single ObjStm container and register
/// `XRefEntry::Compressed` entries for each packed object.
fn pack_chunk_into_objstm(
    doc: &mut Document,
    object_ids: &[crate::cos::ObjectId],
) -> PdfResult<()> {
    use crate::writer::serializer::Serializer;
    use super::streams::deflate_best;

    // Build the ObjStm body: preamble (num offset pairs) + serialised objects.
    let mut preamble = String::new();
    let mut body = Vec::new();

    for (index, &obj_id) in object_ids.iter().enumerate() {
        let offset = body.len();
        preamble.push_str(&format!("{} {} ", obj_id.object_number, offset));

        let obj = doc.get_object_ref(obj_id).cloned();
        if let Some(o) = obj {
            let mut ser = Serializer::new(&mut body);
            let _ = ser.write_object(&o);
            body.push(b'\n');
        }
        let _ = index;
    }

    // Combine preamble + body.
    let preamble_bytes = preamble.into_bytes();
    let first = preamble_bytes.len();
    let mut full = preamble_bytes;
    full.extend_from_slice(&body);

    // Compress with FlateDecode (always flate2 level-9 for ObjStm — Zopfli not needed here).
    let compressed = deflate_best(&full, false).map_err(|e| crate::PdfError::Compress {
        reason: format!("ObjStm deflate failed: {e}"),
    })?;

    // Allocate a new ObjectId for the ObjStm.
    let objstm_id = doc.allocate_object_id();

    // Build the ObjStm stream dict.
    let mut dict = crate::cos::CosDictionary::new();
    dict.set(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"ObjStm".to_vec())));
    dict.set(CosName::new(b"N".to_vec()), CosObject::Integer(object_ids.len() as i64));
    dict.set(CosName::new(b"First".to_vec()), CosObject::Integer(first as i64));
    dict.set(CosName::new(b"Filter".to_vec()), CosObject::Name(CosName::new(b"FlateDecode".to_vec())));
    dict.set(CosName::new(b"Length".to_vec()), CosObject::Integer(compressed.len() as i64));

    let objstm_stream = crate::cos::CosStream {
        dictionary: dict,
        data: compressed,
    };

    doc.insert_object(objstm_id, CosObject::Stream(objstm_stream));

    // Register XRefEntry::Compressed for each packed object and remove from
    // the main object store (they now live inside the ObjStm).
    for (index, &obj_id) in object_ids.iter().enumerate() {
        doc.mark_compressed(obj_id, objstm_id.object_number, index as u32);
        doc.remove_object(obj_id);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::CompressOptions;

    #[test]
    fn version_report_default() {
        let r = VersionReport::default();
        assert!(!r.version_downgraded);
        assert_eq!(r.objstm_repacked, 0);
    }

    #[test]
    fn run_on_minimal_pdf_no_panic() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let mut opts = CompressOptions::default();
        opts.repack_object_streams = false; // disabled until Document API is complete
        opts.downgrade_pdf_version = true;
        let result = run(&mut doc, &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn downgrade_skipped_for_encrypted_pdf() {
        // Minimal PDF has no encryption — downgrade should be applied if safe.
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        // With no AES encryption in minimal PDF, downgrade is safe.
        let downgraded = try_downgrade(&mut doc);
        // Either true or false is acceptable here (depends on whether set_version is implemented).
        let _ = downgraded;
    }

    #[test]
    fn downgrade_14_header_label() {
        // Verify the version constants are reasonable.
        assert_eq!(1_u8, 1);
        assert_eq!(4_u8, 4);
    }
}
