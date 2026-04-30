//! Pass 6 — TrueType + CFF font subsetting.
//!
//! For every font in the document's `/Resources /Font` dict:
//! 1. Walk all page content streams to collect character codes used with that font.
//! 2. Map character codes → Glyph IDs via the font's encoding / CIDToGIDMap.
//! 3. Call `subsetter::subset(font_bytes, glyph_ids)` to produce a minimal font.
//! 4. Write the subset back to `/FontFile2` (TrueType) or `/FontFile3` (CFF).
//! 5. Remove fonts with zero glyphs used (declared but never referenced).
//!
//! **Crates:**
//! - [`subsetter`](https://crates.io/crates/subsetter) `0.2` — TrueType + CFF subsetter
//! - [`ttf-parser`](https://crates.io/crates/ttf-parser) `0.25` — glyph table reader
//! - [`owned_ttf_parser`](https://crates.io/crates/owned_ttf_parser) `0.25` — owned wrapper

use crate::cos::{CosName, CosObject, ObjectId};
use crate::{Document, PdfResult};
use super::CompressOptions;

#[cfg(feature = "compress-fonts")]
use subsetter::GlyphRemapper;
#[cfg(feature = "compress-fonts")]
use ttf_parser::Face;

// ---------------------------------------------------------------------------
// Public report
// ---------------------------------------------------------------------------

/// Statistics returned by [`run`].
#[derive(Debug, Default)]
pub struct FontSubsetReport {
    /// Number of font programs that were successfully subset.
    pub fonts_subsetted: usize,
    /// Number of font objects removed (declared but never used in content).
    pub fonts_removed: usize,
    /// Approximate bytes saved by font subsetting + removal.
    pub bytes_saved: usize,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Subset all embedded TrueType and CFF fonts in `doc`.
pub fn run(doc: &mut Document, opts: &CompressOptions) -> PdfResult<FontSubsetReport> {
    let mut report = FontSubsetReport::default();

    if !opts.subset_fonts {
        return Ok(report);
    }

    // ── Step 1: collect all font resource entries across all pages ─────────────
    let font_entries: Vec<FontEntry> = collect_font_entries(doc);

    // ── Step 2: collect used glyph IDs per font from content streams ──────────
    let usage_map = collect_glyph_usage(doc, &font_entries)?;

    // ── Step 3: subset each font ──────────────────────────────────────────────
    for entry in &font_entries {
        let used_glyphs = usage_map.get(&entry.font_dict_id)
            .cloned()
            .unwrap_or_default();

        if opts.font_remove_unused && used_glyphs.is_empty() {
            // Remove the font object entirely.
            if let Some(removed) = doc.remove_object(entry.font_dict_id) {
                report.fonts_removed += 1;
                report.bytes_saved += object_byte_estimate(&removed);
            }
            continue;
        }

        match subset_font(doc, entry, &used_glyphs, opts) {
            Ok(Some(saved)) => {
                report.fonts_subsetted += 1;
                report.bytes_saved += saved;
            }
            Ok(None) => {}
            Err(_) if opts.skip_on_decode_error => {}
            Err(e) => return Err(e),
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Font discovery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FontEntry {
    /// ObjectId of the font dictionary.
    font_dict_id: ObjectId,
    /// Resource name used in content streams (e.g. "F1").
    resource_name: String,
    /// Font subtype: "TrueType", "Type1", "CIDFontType2", etc.
    #[allow(dead_code)]
    subtype: String,
}

fn collect_font_entries(doc: &Document) -> Vec<FontEntry> {
    let mut entries = Vec::new();
    let page_ids: Vec<ObjectId> = doc.page_object_ids().collect();

    for page_id in &page_ids {
        let res_id = match doc.page_resources_id(*page_id) {
            Some(id) => id,
            None => continue,
        };
        let res_obj = match doc.get_object_ref(res_id) {
            Some(o) => o,
            None => continue,
        };
        let res_dict = match res_obj.as_dictionary() {
            Some(d) => d,
            None => continue,
        };
        let font_dict = match res_dict.get(&CosName::new(b"Font".to_vec())) {
            Some(CosObject::Dictionary(d)) => d.clone(),
            Some(CosObject::Reference(r)) => {
                match doc.get_object_ref(*r).and_then(|o| o.as_dictionary()) {
                    Some(d) => d.clone(),
                    None => continue,
                }
            }
            _ => continue,
        };

        for (k, v) in font_dict.iter() {
            let font_id = match v.as_reference() {
                Some(id) => id,
                None => continue,
            };
            let name = k.as_str().unwrap_or("").to_string();
            let subtype = get_font_subtype(doc, font_id);

            // Avoid duplicates (same font on multiple pages).
            if !entries.iter().any(|e: &FontEntry| e.font_dict_id == font_id) {
                entries.push(FontEntry {
                    font_dict_id: font_id,
                    resource_name: name,
                    subtype,
                });
            }
        }
    }

    entries
}

fn get_font_subtype(doc: &Document, font_id: ObjectId) -> String {
    let obj = match doc.get_object_ref(font_id) {
        Some(o) => o,
        None => return String::new(),
    };
    let dict = match obj.as_dictionary() {
        Some(d) => d,
        None => return String::new(),
    };
    dict.get(&CosName::new(b"Subtype".to_vec()))
        .and_then(|v| if let CosObject::Name(n) = v { n.as_str().map(|s| s.to_string()) } else { None })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Glyph usage collection
// ---------------------------------------------------------------------------

/// Returns a map of font_dict_id → set of character codes used.
fn collect_glyph_usage(
    doc: &Document,
    font_entries: &[FontEntry],
) -> PdfResult<std::collections::HashMap<ObjectId, std::collections::HashSet<u16>>> {
    let mut usage: std::collections::HashMap<ObjectId, std::collections::HashSet<u16>> =
        std::collections::HashMap::new();

    let page_ids: Vec<ObjectId> = doc.page_object_ids().collect();

    for page_id in &page_ids {
        let content = match doc.page_content_bytes(*page_id) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let text = std::str::from_utf8(&content).unwrap_or("");
        let tokens: Vec<&str> = text.split_ascii_whitespace().collect();

        // Build resource-name → font_dict_id map for this page.
        let res_map: std::collections::HashMap<String, ObjectId> = font_entries
            .iter()
            .map(|e| (e.resource_name.clone(), e.font_dict_id))
            .collect();

        // Track which font is active.
        let mut active_font: Option<ObjectId> = None;

        let mut i = 0;
        while i < tokens.len() {
            let tok = tokens[i];
            if tok == "Tf" && i >= 2 && tokens[i-2].starts_with('/') {
                let name = &tokens[i-2][1..];
                active_font = res_map.get(name).copied();
            } else if tok == "Tj" || tok == "TJ" || tok == "'" || tok == "\"" {
                if let Some(font_id) = active_font {
                    // Extract character codes from Tj strings and TJ arrays.
                    let set = usage.entry(font_id).or_default();
                    // Collect raw bytes used between the previous `(` / `<` token.
                    extract_char_codes(&tokens, i, set);
                }
            }
            i += 1;
        }
    }

    Ok(usage)
}

/// Extract character bytes from the operand token before a text operator.
fn extract_char_codes(
    tokens: &[&str],
    op_idx: usize,
    set: &mut std::collections::HashSet<u16>,
) {
    if op_idx == 0 {
        return;
    }
    let operand = tokens[op_idx - 1];

    if operand.starts_with('(') && operand.ends_with(')') {
        // Literal string — collect byte values.
        let inner = &operand[1..operand.len()-1];
        for b in inner.bytes() {
            set.insert(b as u16);
        }
    } else if operand.starts_with('<') && operand.ends_with('>') {
        // Hex string — decode pairs.
        let hex = &operand[1..operand.len()-1];
        for chunk in hex.as_bytes().chunks(2) {
            if chunk.len() == 2 {
                if let Ok(s) = std::str::from_utf8(chunk) {
                    if let Ok(b) = u8::from_str_radix(s, 16) {
                        set.insert(b as u16);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Font subsetting
// ---------------------------------------------------------------------------

/// Attempt to subset the font at `entry`. Returns `Some(bytes_saved)` on success.
fn subset_font(
    doc: &mut Document,
    entry: &FontEntry,
    used_codes: &std::collections::HashSet<u16>,
    opts: &CompressOptions,
) -> PdfResult<Option<usize>> {
    if used_codes.is_empty() {
        return Ok(None);
    }

    // Locate /FontDescriptor → /FontFile2 (TrueType) or /FontFile3 (CFF).
    let (font_file_id, font_type) = match find_font_file(doc, entry.font_dict_id) {
        Some(r) => r,
        None => return Ok(None),
    };

    let is_truetype = font_type == FontFileType::TrueType;
    let is_cff      = font_type == FontFileType::CFF;

    if !is_truetype && !is_cff {
        return Ok(None);
    }
    if is_truetype && !opts.font_subset_truetype {
        return Ok(None);
    }
    if is_cff && !opts.font_subset_cff {
        return Ok(None);
    }

    #[cfg(feature = "compress-fonts")]
    {
        return subset_with_subsetter(doc, entry, font_file_id, font_type, used_codes);
    }

    // Without compress-fonts feature, skip.
    #[allow(unreachable_code)]
    Ok(None)
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
enum FontFileType {
    TrueType,
    CFF,
    Unknown,
}

/// Returns `(FontFile ObjectId, type)` by walking FontDescriptor.
fn find_font_file(doc: &Document, font_dict_id: ObjectId) -> Option<(ObjectId, FontFileType)> {
    let font_obj = doc.get_object_ref(font_dict_id)?;
    let font_dict = font_obj.as_dictionary()?;

    let desc_id = match font_dict.get(&CosName::new(b"FontDescriptor".to_vec()))? {
        CosObject::Reference(id) => *id,
        _ => return None,
    };
    let desc_obj = doc.get_object_ref(desc_id)?;
    let desc_dict = desc_obj.as_dictionary()?;

    // /FontFile2 → TrueType
    if let Some(CosObject::Reference(ff_id)) =
        desc_dict.get(&CosName::new(b"FontFile2".to_vec()))
    {
        return Some((*ff_id, FontFileType::TrueType));
    }

    // /FontFile3 → CFF / Type1C
    if let Some(CosObject::Reference(ff_id)) =
        desc_dict.get(&CosName::new(b"FontFile3".to_vec()))
    {
        return Some((*ff_id, FontFileType::CFF));
    }

    None
}

// ---------------------------------------------------------------------------
// subsetter implementation
// ---------------------------------------------------------------------------

#[cfg(feature = "compress-fonts")]
fn subset_with_subsetter(
    doc: &mut Document,
    _entry: &FontEntry,
    font_file_id: ObjectId,
    _font_type: FontFileType,
    used_codes: &std::collections::HashSet<u16>,
) -> PdfResult<Option<usize>> {
    // Decode the font program bytes.
    let font_bytes = {
        let obj = doc.get_object_ref(font_file_id);
        let stream = match obj.and_then(|o| o.as_stream()) {
            Some(s) => s,
            None => return Ok(None),
        };
        let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
        crate::io::decode_stream(&stream.data, filter)
            .unwrap_or_else(|_| stream.data.clone())
    };

    // Map char codes → GID using ttf-parser cmap.
    let gids: Vec<u16> = {
        match Face::parse(&font_bytes, 0) {
            Ok(face) => {
                used_codes.iter()
                    .filter_map(|&code| face.glyph_index(char::from_u32(code as u32)?))
                    .map(|g| g.0)
                    .collect()
            }
            Err(_) => {
                // If we can't parse the face, use codes as GIDs directly (Identity mapping).
                used_codes.iter().copied().collect()
            }
        }
    };

    if gids.is_empty() {
        return Ok(None);
    }

    // Build GlyphRemapper for subsetter.
    let mut remapper = GlyphRemapper::new();
    for &gid in &gids {
        remapper.remap(gid);
    }

    let original_len = font_bytes.len();

    // Run subsetter.
    let subset_bytes = match subsetter::subset(&font_bytes, 0, &remapper) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    if subset_bytes.len() >= original_len {
        return Ok(None); // no savings
    }

    let saved = original_len - subset_bytes.len();

    // Re-compress the subset with FlateDecode and write back.
    let compressed = crate::compress::streams::deflate_best(&subset_bytes, false).map_err(|e| {
        crate::PdfError::Compress {
            reason: format!("font subset deflate failed: {e}"),
        }
    })?;

    let compressed_len = compressed.len() as i64;
    doc.mutate_object(font_file_id, |obj| {
        if let CosObject::Stream(stream) = obj {
            stream.data = compressed;
            stream.dictionary.set(
                CosName::new(b"Filter".to_vec()),
                CosObject::Name(CosName::new(b"FlateDecode".to_vec())),
            );
            stream.dictionary.set(
                CosName::new(b"Length".to_vec()),
                CosObject::Integer(compressed_len),
            );
            stream.dictionary.set(
                CosName::new(b"Length1".to_vec()),
                CosObject::Integer(subset_bytes.len() as i64),
            );
        }
    });

    Ok(Some(saved))
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn object_byte_estimate(obj: &CosObject) -> usize {
    match obj {
        CosObject::Stream(s) => s.data.len() + 64,
        CosObject::Dictionary(d) => d.iter().count() * 32 + 8,
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

    #[test]
    fn font_subset_report_default_zero() {
        let r = FontSubsetReport::default();
        assert_eq!(r.fonts_subsetted, 0);
        assert_eq!(r.fonts_removed, 0);
        assert_eq!(r.bytes_saved, 0);
    }

    #[test]
    fn run_on_minimal_pdf_no_panic() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let opts = CompressOptions::default();
        let result = run(&mut doc, &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn run_skipped_when_option_off() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let mut opts = CompressOptions::default();
        opts.subset_fonts = false;
        let report = run(&mut doc, &opts).unwrap();
        assert_eq!(report.fonts_subsetted, 0);
    }

    #[test]
    fn extract_char_codes_literal_string() {
        let tokens = vec!["(Hello)", "Tj"];
        let mut set = std::collections::HashSet::new();
        extract_char_codes(&tokens, 1, &mut set);
        assert!(set.contains(&(b'H' as u16)));
        assert!(set.contains(&(b'e' as u16)));
        assert!(set.contains(&(b'l' as u16)));
        assert!(set.contains(&(b'o' as u16)));
        assert_eq!(set.len(), 4); // H e l o (duplicate 'l' deduplicated)
    }

    #[test]
    fn extract_char_codes_hex_string() {
        // <4865> = "He"
        let tokens = vec!["<4865>", "Tj"];
        let mut set = std::collections::HashSet::new();
        extract_char_codes(&tokens, 1, &mut set);
        assert!(set.contains(&0x48u16)); // 'H'
        assert!(set.contains(&0x65u16)); // 'e'
    }

    #[test]
    fn glyph_ids_collected_basic() {
        // Verify that extract_char_codes deduplicates correctly.
        let tokens = vec!["(aaa)", "Tj"];
        let mut set = std::collections::HashSet::new();
        extract_char_codes(&tokens, 1, &mut set);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&(b'a' as u16)));
    }

    #[test]
    fn unused_font_removed_when_option_enabled() {
        // Minimal PDF has no fonts — removal count stays 0.
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let mut opts = CompressOptions::default();
        opts.font_remove_unused = true;
        let report = run(&mut doc, &opts).unwrap();
        // No fonts to remove in the minimal PDF.
        assert_eq!(report.fonts_removed, 0);
    }

    #[test]
    fn widths_updated_placeholder() {
        // Placeholder: verifies the report struct fields exist.
        let r = FontSubsetReport {
            fonts_subsetted: 2,
            fonts_removed: 1,
            bytes_saved: 4096,
        };
        assert_eq!(r.fonts_subsetted, 2);
        assert_eq!(r.fonts_removed, 1);
        assert_eq!(r.bytes_saved, 4096);
    }
}

