//! Pass 1 — Metadata & dead-resource strip.
//!
//! Sub-passes (all pure COS dict manipulation, no new crates):
//!
//! | Sub-pass | What is stripped | PDF location |
//! |---|---|---|
//! | XMP metadata | `/Metadata` stream | Catalog + each page dict |
//! | Page thumbnails | `/Thumb` image XObject | Each page dict |
//! | Structure tree | `/StructTreeRoot` + `/MarkInfo` | Catalog |
//! | Piece info | `/PieceInfo` dict | Catalog + pages |
//! | Optional content | `/OCProperties` + `/OC` entries | Catalog |
//! | Output intents | `/OutputIntents` array | Catalog |
//! | Embedded files | `/EmbeddedFiles` name tree | Catalog |
//! | Dead resources | Unreferenced `/Font`, `/XObject`, `/ExtGState`, … | Page `/Resources` |
//! | Empty content streams | Zero-byte `/Contents` streams | Page `/Contents` arrays |
//! | Info dict trimming | `/Creator`, `/Producer`, `/Keywords`, `/Subject` | `/Info` in trailer |

use crate::cos::{CosName, CosObject};
use crate::{Document, PdfResult};
use super::CompressOptions;

// ---------------------------------------------------------------------------
// Public report
// ---------------------------------------------------------------------------

/// Statistics returned by [`run`].
#[derive(Debug, Default)]
pub struct CleanupReport {
    /// Number of indirect objects removed from the document.
    pub objects_removed: usize,
    /// Approximate bytes freed (sum of `data.len()` for removed streams +
    /// per-entry overhead for removed dict entries).
    pub bytes_saved: usize,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Execute all enabled cleanup sub-passes on `doc`.
pub fn run(doc: &mut Document, opts: &CompressOptions) -> PdfResult<CleanupReport> {
    let mut report = CleanupReport::default();

    if opts.remove_metadata {
        strip_catalog_key(doc, "Metadata", &mut report);
        strip_page_key(doc, "Metadata", &mut report);
    }

    if opts.remove_thumbnails {
        strip_page_key(doc, "Thumb", &mut report);
    }

    if opts.remove_structure_tree {
        strip_catalog_key(doc, "StructTreeRoot", &mut report);
        strip_catalog_key(doc, "MarkInfo", &mut report);
    }

    if opts.remove_piece_info {
        strip_catalog_key(doc, "PieceInfo", &mut report);
        strip_page_key(doc, "PieceInfo", &mut report);
    }

    if opts.remove_optional_content {
        strip_catalog_key(doc, "OCProperties", &mut report);
    }

    // Always strip OutputIntents + EmbeddedFiles when metadata removal is on —
    // these are typically large and provide no viewing value.
    if opts.remove_metadata {
        strip_catalog_key(doc, "OutputIntents", &mut report);
        strip_catalog_key(doc, "EmbeddedFiles", &mut report);
        trim_info_dict(doc, &mut report);
    }

    if opts.clean_dead_resources {
        remove_dead_resources(doc, &mut report)?;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Helpers — catalog
// ---------------------------------------------------------------------------

/// Remove `key` from the document catalog dictionary, recording savings.
fn strip_catalog_key(doc: &mut Document, key: &str, report: &mut CleanupReport) {
    let catalog_id = match doc.catalog_id() {
        Some(id) => id,
        None => return,
    };
    let name = CosName::new(key.as_bytes().to_vec());

    // Check whether the key holds a Reference we can remove entirely.
    let ref_id = {
        let obj = doc.get_object_ref(catalog_id);
        match obj {
            Some(CosObject::Dictionary(dict)) => {
                if let Some(CosObject::Reference(id)) = dict.get(&name) {
                    Some(*id)
                } else {
                    None
                }
            }
            _ => None,
        }
    };

    // Remove the referenced object if it exists.
    if let Some(id) = ref_id {
        if let Some(removed) = doc.remove_object(id) {
            report.objects_removed += 1;
            report.bytes_saved += stream_or_dict_size(&removed);
        }
    }

    // Remove the key from the catalog dict.
    doc.mutate_object(catalog_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.remove(&name);
        }
    });
}

// ---------------------------------------------------------------------------
// Helpers — pages
// ---------------------------------------------------------------------------

/// Remove `key` from every page dictionary, recording savings.
fn strip_page_key(doc: &mut Document, key: &str, report: &mut CleanupReport) {
    let page_ids: Vec<_> = doc.page_object_ids().collect();
    let name = CosName::new(key.as_bytes().to_vec());

    for page_id in page_ids {
        // Collect any Reference values to remove.
        let ref_id = {
            let obj = doc.get_object_ref(page_id);
            match obj {
                Some(CosObject::Dictionary(dict)) => {
                    if let Some(CosObject::Reference(id)) = dict.get(&name) {
                        Some(*id)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };

        if let Some(id) = ref_id {
            if let Some(removed) = doc.remove_object(id) {
                report.objects_removed += 1;
                report.bytes_saved += stream_or_dict_size(&removed);
            }
        }

        doc.mutate_object(page_id, |obj| {
            if let CosObject::Dictionary(dict) = obj {
                dict.remove(&name);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Helpers — trailer /Info dict trimming
// ---------------------------------------------------------------------------

fn trim_info_dict(doc: &mut Document, report: &mut CleanupReport) {
    let info_id = match doc.info_id() {
        Some(id) => id,
        None => return,
    };
    let keys_to_remove = [
        "Creator", "Producer", "Keywords", "Subject",
        "Company", "SourceModified",
    ];
    doc.mutate_object(info_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            for key in &keys_to_remove {
                if dict.remove(&CosName::new(key.as_bytes().to_vec())).is_some() {
                    report.bytes_saved += 32; // rough per-entry overhead
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Helpers — dead resource removal
// ---------------------------------------------------------------------------

/// Collect all resource names actually used in every page content stream,
/// then remove entries from `/Resources` that are never referenced.
fn remove_dead_resources(doc: &mut Document, report: &mut CleanupReport) -> PdfResult<()> {
    use std::collections::HashSet;

    // Collect used resource names per resource type by scanning content streams.
    let mut used_fonts: HashSet<String> = HashSet::new();
    let mut used_xobjects: HashSet<String> = HashSet::new();
    let mut used_extgstate: HashSet<String> = HashSet::new();
    let mut used_colorspace: HashSet<String> = HashSet::new();

    let page_ids: Vec<_> = doc.page_object_ids().collect();
    for page_id in &page_ids {
        let content_bytes = match doc.page_content_bytes(*page_id) {
            Ok(b) => b,
            Err(_) => continue,
        };
        collect_used_resources(
            &content_bytes,
            &mut used_fonts,
            &mut used_xobjects,
            &mut used_extgstate,
            &mut used_colorspace,
        );
    }

    // For each page, prune the /Resources dict.
    for page_id in &page_ids {
        let resources_id = match doc.page_resources_id(*page_id) {
            Some(id) => id,
            None => continue,
        };

        prune_resource_subdict(
            doc, resources_id, "Font", &used_fonts, report,
        );
        prune_resource_subdict(
            doc, resources_id, "XObject", &used_xobjects, report,
        );
        prune_resource_subdict(
            doc, resources_id, "ExtGState", &used_extgstate, report,
        );
        prune_resource_subdict(
            doc, resources_id, "ColorSpace", &used_colorspace, report,
        );
    }

    Ok(())
}

/// Scan raw content stream bytes for resource-referencing operators.
/// This is a lightweight token scan — not a full content parser — for speed.
fn collect_used_resources(
    bytes: &[u8],
    fonts: &mut std::collections::HashSet<String>,
    xobjects: &mut std::collections::HashSet<String>,
    extgstate: &mut std::collections::HashSet<String>,
    colorspace: &mut std::collections::HashSet<String>,
) {
    // Tokenise on whitespace.
    let text = std::str::from_utf8(bytes).unwrap_or("");
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();

    // Scan for operator patterns:
    //   /FontName <size> Tf      → font (name is 2 before operator)
    //   /Name Do               → xobject (name is 1 before operator)
    //   /Name gs               → extgstate (name is 1 before operator)
    //   /Name cs / /Name CS    → colorspace (name is 1 before operator)
    for i in 0..tokens.len() {
        let op = tokens[i];
        match op {
            "Tf" => {
                // Pattern: /FontName <size> Tf
                if i >= 2 && tokens[i - 2].starts_with('/') {
                    fonts.insert(tokens[i - 2][1..].to_string());
                }
            }
            "Do" | "gs" | "cs" | "CS" | "scn" | "SCN" => {
                // Pattern: /Name <op>
                if i >= 1 && tokens[i - 1].starts_with('/') {
                    let name = tokens[i - 1][1..].to_string();
                    match op {
                        "Do" => { xobjects.insert(name); }
                        "gs" => { extgstate.insert(name); }
                        "cs" | "CS" | "scn" | "SCN" => { colorspace.insert(name); }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// Remove entries from a resource sub-dictionary (e.g. `/Font`) that are not in `used`.
fn prune_resource_subdict(
    doc: &mut Document,
    resources_id: crate::cos::ObjectId,
    subdict_key: &str,
    used: &std::collections::HashSet<String>,
    report: &mut CleanupReport,
) {
    let subdict_id = {
        let obj = doc.get_object_ref(resources_id);
        match obj {
            Some(CosObject::Dictionary(dict)) => {
                match dict.get(&CosName::new(subdict_key.as_bytes().to_vec())) {
                    Some(CosObject::Reference(id)) => Some(*id),
                    _ => None,
                }
            }
            _ => None,
        }
    };

    let id = match subdict_id {
        Some(id) => id,
        None => return,
    };

    // Collect keys to remove.
    let to_remove: Vec<(String, Option<crate::cos::ObjectId>)> = {
        let obj = doc.get_object_ref(id);
        match obj {
            Some(CosObject::Dictionary(dict)) => {
                dict.entries()
                    .filter(|(k, _)| {
                        let key_str = k.as_str().unwrap_or("");
                        !used.contains(key_str)
                    })
                    .map(|(k, v)| {
                        let ref_id = if let CosObject::Reference(rid) = v {
                            Some(*rid)
                        } else {
                            None
                        };
                        (k.as_str().unwrap_or("").to_string(), ref_id)
                    })
                    .collect()
            }
            _ => return,
        }
    };

    for (key, ref_id) in to_remove {
        doc.mutate_object(id, |obj| {
            if let CosObject::Dictionary(dict) = obj {
                dict.remove(&CosName::new(key.as_bytes().to_vec()));
            }
        });
        if let Some(rid) = ref_id {
            if let Some(removed) = doc.remove_object(rid) {
                report.objects_removed += 1;
                report.bytes_saved += stream_or_dict_size(&removed);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn stream_or_dict_size(obj: &CosObject) -> usize {
    match obj {
        CosObject::Stream(s) => s.data.len() + 64,
        CosObject::Dictionary(d) => d.entries().count() * 32 + 8,
        CosObject::String(s) => s.len() + 4,
        _ => 16,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::CompressOptions;

    fn make_doc() -> Document {
        let pdf = crate::tests::minimal_pdf();
        Document::load_from_bytes(&pdf).unwrap()
    }

    #[test]
    fn cleanup_run_on_minimal_pdf_no_panic() {
        let mut doc = make_doc();
        let opts = CompressOptions::for_mode(crate::compress::CompressionMode::Recommended);
        let result = run(&mut doc, &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn cleanup_report_default_zero() {
        let r = CleanupReport::default();
        assert_eq!(r.objects_removed, 0);
        assert_eq!(r.bytes_saved, 0);
    }

    #[test]
    fn collect_used_resources_finds_font() {
        use std::collections::HashSet;
        let content = b"/F1 12 Tf (Hello) Tj";
        let mut fonts = HashSet::new();
        let mut xobjects = HashSet::new();
        let mut extgstate = HashSet::new();
        let mut colorspace = HashSet::new();
        collect_used_resources(content, &mut fonts, &mut xobjects, &mut extgstate, &mut colorspace);
        assert!(fonts.contains("F1"));
    }

    #[test]
    fn collect_used_resources_finds_xobject() {
        use std::collections::HashSet;
        let content = b"/Im1 Do";
        let mut fonts = HashSet::new();
        let mut xobjects = HashSet::new();
        let mut extgstate = HashSet::new();
        let mut colorspace = HashSet::new();
        collect_used_resources(content, &mut fonts, &mut xobjects, &mut extgstate, &mut colorspace);
        assert!(xobjects.contains("Im1"));
    }

    #[test]
    fn collect_used_resources_finds_extgstate() {
        use std::collections::HashSet;
        let content = b"/GS1 gs";
        let mut fonts = HashSet::new();
        let mut xobjects = HashSet::new();
        let mut extgstate = HashSet::new();
        let mut colorspace = HashSet::new();
        collect_used_resources(content, &mut fonts, &mut xobjects, &mut extgstate, &mut colorspace);
        assert!(extgstate.contains("GS1"));
    }
}

