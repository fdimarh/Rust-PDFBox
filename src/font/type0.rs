//! Type0 (composite) font parsing.
//!
//! Maps to Java PDFBox `PDType0Font`.
//!
//! A Type0 (composite) font uses multi-byte character codes via a CMap.
//! It contains a `DescendantFonts` array with exactly one CIDFont.
//!
//! PDF §9.7.

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use super::cmap::{ToUnicodeCMap, parse_to_unicode_cmap};

// ---------------------------------------------------------------------------
// CIDFont type
// ---------------------------------------------------------------------------

/// Identifies the CIDFont subtype embedded in a Type0 font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CidFontType {
    /// Type2 — CIDFont with TrueType outline data.
    CidFontType2,
    /// Type0 — CIDFont with CFF/Type1 outline data.
    CidFontType0,
}

impl CidFontType {
    pub fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"CIDFontType2" => Some(Self::CidFontType2),
            b"CIDFontType0" => Some(Self::CidFontType0),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// CidSystemInfo
// ---------------------------------------------------------------------------

/// `/CIDSystemInfo` dictionary from a CIDFont.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CidSystemInfo {
    pub registry: String,
    pub ordering: String,
    pub supplement: i32,
}

impl CidSystemInfo {
    pub fn from_dict(dict: &CosDictionary) -> Self {
        let registry = dict
            .get(&CosName::new(b"Registry".to_vec()))
            .and_then(|v| v.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string())
            .unwrap_or_default();
        let ordering = dict
            .get(&CosName::new(b"Ordering".to_vec()))
            .and_then(|v| v.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string())
            .unwrap_or_default();
        let supplement = dict
            .get_int(&CosName::new(b"Supplement".to_vec()))
            .unwrap_or(0) as i32;
        Self { registry, ordering, supplement }
    }

    /// Returns `"Registry-Ordering-Supplement"` identifier.
    pub fn ros_string(&self) -> String {
        format!("{}-{}-{}", self.registry, self.ordering, self.supplement)
    }
}

// ---------------------------------------------------------------------------
// DescendantFont (CIDFont dictionary)
// ---------------------------------------------------------------------------

/// The CIDFont embedded as the sole entry in `DescendantFonts`.
#[derive(Debug, Clone)]
pub struct DescendantFont {
    /// CIDFont subtype.
    pub cid_font_type: CidFontType,
    /// PostScript name (`/BaseFont`).
    pub base_font: String,
    /// CIDSystemInfo.
    pub cid_system_info: CidSystemInfo,
    /// Default width for glyphs not in `/W` table.
    pub default_width: f64,
    /// Horizontal glyph widths: CID → width (in 1/1000 units).
    pub widths: std::collections::HashMap<u32, f64>,
}

impl DescendantFont {
    pub fn from_dict(dict: &CosDictionary, _get_object: &dyn Fn(ObjectId) -> Option<CosObject>) -> Option<Self> {
        let subtype = dict.get_name(&CosName::subtype())?;
        let cid_font_type = CidFontType::from_name(subtype.as_bytes())?;

        let base_font = dict.get_name(&CosName::new(b"BaseFont".to_vec()))
            .map(|n| String::from_utf8_lossy(n.as_bytes()).to_string())
            .unwrap_or_default();

        let cid_system_info = dict
            .get(&CosName::new(b"CIDSystemInfo".to_vec()))
            .and_then(|v| v.as_dictionary())
            .map(CidSystemInfo::from_dict)
            .unwrap_or_default();

        let default_width = dict
            .get_number(&CosName::new(b"DW".to_vec()))
            .unwrap_or(1000.0);

        // Parse /W array: [ cid width cid width ... ] or [ cid [w0 w1 ...] ... ]
        let mut widths = std::collections::HashMap::new();
        if let Some(w_arr) = dict.get_array(&CosName::new(b"W".to_vec())) {
            let mut j = 0;
            while j < w_arr.len() {
                let start_cid = match w_arr[j].as_integer() {
                    Some(n) => n as u32,
                    None => { j += 1; continue; }
                };
                j += 1;
                if j >= w_arr.len() { break; }
                match &w_arr[j] {
                    CosObject::Array(sub) => {
                        // [cid [w0 w1 ...]] form
                        for (off, w) in sub.iter().enumerate() {
                            if let Some(width) = w.as_number() {
                                widths.insert(start_cid + off as u32, width);
                            }
                        }
                        j += 1;
                    }
                    CosObject::Integer(end_cid) => {
                        // [cid_first cid_last w] form
                        let end = *end_cid as u32;
                        j += 1;
                        if j < w_arr.len() {
                            if let Some(w) = w_arr[j].as_number() {
                                for cid in start_cid..=end {
                                    widths.insert(cid, w);
                                }
                            }
                            j += 1;
                        }
                    }
                    _ => { j += 1; }
                }
            }
        }

        Some(Self { cid_font_type, base_font, cid_system_info, default_width, widths })
    }

    /// Get horizontal advance width for a CID (in 1/1000 units).
    pub fn width_for_cid(&self, cid: u32) -> f64 {
        self.widths.get(&cid).copied().unwrap_or(self.default_width)
    }
}

// ---------------------------------------------------------------------------
// Type0Font
// ---------------------------------------------------------------------------

/// A parsed Type0 (composite) PDF font.
///
/// Maps to Java PDFBox `PDType0Font`.
#[derive(Debug, Clone)]
pub struct Type0Font {
    /// Base font name (`/BaseFont` of the Type0 font dict).
    pub base_font: String,
    /// The encoding/CMap name (`/Encoding`, e.g. `"Identity-H"`).
    pub encoding_name: String,
    /// The embedded CIDFont.
    pub descendant: Option<DescendantFont>,
    /// ToUnicode CMap, if present.
    pub to_unicode: Option<ToUnicodeCMap>,
}

impl Type0Font {
    /// Parse a `Type0Font` from the font dictionary.
    pub fn from_dict(
        dict: &CosDictionary,
        get_object: &dyn Fn(ObjectId) -> Option<CosObject>,
    ) -> Option<Self> {
        let base_font = dict.get_name(&CosName::new(b"BaseFont".to_vec()))
            .map(|n| String::from_utf8_lossy(n.as_bytes()).to_string())
            .unwrap_or_default();

        let encoding_name = dict.get_name(&CosName::new(b"Encoding".to_vec()))
            .map(|n| String::from_utf8_lossy(n.as_bytes()).to_string())
            .unwrap_or_else(|| "Identity-H".to_string());

        // DescendantFonts — exactly one CIDFont
        let descendant = dict
            .get_array(&CosName::new(b"DescendantFonts".to_vec()))
            .and_then(|arr| arr.first().cloned())
            .and_then(|obj| {
                let dict = match obj {
                    CosObject::Dictionary(d) => Some(d),
                    CosObject::Reference(id) => get_object(id)?.into_dictionary(),
                    _ => None,
                }?;
                DescendantFont::from_dict(&dict, get_object)
            });

        // ToUnicode
        let to_unicode = dict
            .get(&CosName::new(b"ToUnicode".to_vec()))
            .and_then(|tu| match tu {
                CosObject::Stream(s) => Some(parse_to_unicode_cmap(&s.data)),
                CosObject::Reference(id) => {
                    let obj = get_object(*id)?;
                    if let CosObject::Stream(s) = obj { Some(parse_to_unicode_cmap(&s.data)) } else { None }
                }
                _ => None,
            });

        Some(Self { base_font, encoding_name, descendant, to_unicode })
    }

    /// Decode a 2-byte big-endian CID to Unicode using the ToUnicode CMap.
    /// Falls back to the CID value itself (as a char) if no mapping.
    pub fn decode_bytes(&self, bytes: &[u8]) -> String {
        // Two-byte codes for Identity-H / Identity-V
        if let Some(cmap) = &self.to_unicode {
            let mut out = String::new();
            let mut i = 0;
            while i < bytes.len() {
                let code = if i + 1 < bytes.len() {
                    let c = u32::from(bytes[i]) << 8 | u32::from(bytes[i + 1]);
                    i += 2; c
                } else {
                    let c = u32::from(bytes[i]); i += 1; c
                };
                if let Some(s) = cmap.to_unicode(code) {
                    out.push_str(&s);
                } else if let Some(c) = char::from_u32(code) {
                    out.push(c);
                }
            }
            return out;
        }

        // No CMap — best effort: treat pairs as UCS-2
        let mut out = String::new();
        let mut i = 0;
        while i + 1 < bytes.len() {
            let code = u32::from(bytes[i]) << 8 | u32::from(bytes[i + 1]);
            if let Some(c) = char::from_u32(code) { out.push(c); }
            i += 2;
        }
        out
    }

    /// `true` if the encoding is Identity-H or Identity-V.
    pub fn is_identity_encoding(&self) -> bool {
        matches!(self.encoding_name.as_str(), "Identity-H" | "Identity-V")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, CosStream};

    fn no_object(_: ObjectId) -> Option<CosObject> { None }

    fn make_descendant_dict(base_font: &str) -> CosDictionary {
        let mut d = CosDictionary::new();
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"CIDFontType2".to_vec())));
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(base_font.as_bytes().to_vec())));
        let mut csi = CosDictionary::new();
        csi.set(CosName::new(b"Registry".to_vec()), CosObject::String(b"Adobe".to_vec()));
        csi.set(CosName::new(b"Ordering".to_vec()), CosObject::String(b"Identity".to_vec()));
        csi.set(CosName::new(b"Supplement".to_vec()), CosObject::Integer(0));
        d.set(CosName::new(b"CIDSystemInfo".to_vec()), CosObject::Dictionary(csi));
        d.set(CosName::new(b"DW".to_vec()), CosObject::Integer(1000));
        d
    }

    fn make_type0_dict() -> CosDictionary {
        let mut d = CosDictionary::new();
        d.set(CosName::type_name(), CosObject::Name(CosName::new(b"Font".to_vec())));
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"Type0".to_vec())));
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(b"Arial-Bold".to_vec())));
        d.set(CosName::new(b"Encoding".to_vec()), CosObject::Name(CosName::new(b"Identity-H".to_vec())));
        let desc_dict = make_descendant_dict("Arial-Bold");
        d.set(CosName::new(b"DescendantFonts".to_vec()),
            CosObject::Array(vec![CosObject::Dictionary(desc_dict)]));
        d
    }

    #[test]
    fn type0_base_font() {
        let d = make_type0_dict();
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        assert_eq!(font.base_font, "Arial-Bold");
    }

    #[test]
    fn type0_identity_encoding() {
        let d = make_type0_dict();
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        assert!(font.is_identity_encoding());
    }

    #[test]
    fn type0_descendant_base_font() {
        let d = make_type0_dict();
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        assert_eq!(font.descendant.as_ref().unwrap().base_font, "Arial-Bold");
    }

    #[test]
    fn type0_cid_system_info() {
        let d = make_type0_dict();
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        let desc = font.descendant.unwrap();
        assert_eq!(desc.cid_system_info.registry, "Adobe");
        assert_eq!(desc.cid_system_info.ordering, "Identity");
        assert_eq!(desc.cid_system_info.ros_string(), "Adobe-Identity-0");
    }

    #[test]
    fn type0_default_width() {
        let d = make_type0_dict();
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        let desc = font.descendant.unwrap();
        assert_eq!(desc.width_for_cid(999), 1000.0); // default
    }

    #[test]
    fn type0_w_array_range_form() {
        let mut d = make_type0_dict();
        let desc_d = {
            let mut dd = make_descendant_dict("Test");
            // [10 12 600] — CIDs 10,11,12 all have width 600
            dd.set(CosName::new(b"W".to_vec()), CosObject::Array(vec![
                CosObject::Integer(10),
                CosObject::Integer(12),
                CosObject::Integer(600),
            ]));
            dd
        };
        d.set(CosName::new(b"DescendantFonts".to_vec()),
            CosObject::Array(vec![CosObject::Dictionary(desc_d)]));
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        let desc = font.descendant.unwrap();
        assert_eq!(desc.width_for_cid(10), 600.0);
        assert_eq!(desc.width_for_cid(11), 600.0);
        assert_eq!(desc.width_for_cid(12), 600.0);
    }

    #[test]
    fn type0_w_array_list_form() {
        let mut d = make_type0_dict();
        let desc_d = {
            let mut dd = make_descendant_dict("Test");
            // [5 [400 500 600]] — CIDs 5→400, 6→500, 7→600
            dd.set(CosName::new(b"W".to_vec()), CosObject::Array(vec![
                CosObject::Integer(5),
                CosObject::Array(vec![
                    CosObject::Integer(400),
                    CosObject::Integer(500),
                    CosObject::Integer(600),
                ]),
            ]));
            dd
        };
        d.set(CosName::new(b"DescendantFonts".to_vec()),
            CosObject::Array(vec![CosObject::Dictionary(desc_d)]));
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        let desc = font.descendant.unwrap();
        assert_eq!(desc.width_for_cid(5), 400.0);
        assert_eq!(desc.width_for_cid(6), 500.0);
        assert_eq!(desc.width_for_cid(7), 600.0);
    }

    #[test]
    fn type0_to_unicode_decode() {
        let cmap = b"begincmap\n1 beginbfchar\n<0048><0048>\nendbfchar\nendcmap\n".to_vec();
        let mut d = make_type0_dict();
        d.set(
            CosName::new(b"ToUnicode".to_vec()),
            CosObject::Stream(CosStream::new(CosDictionary::new(), cmap)),
        );
        let font = Type0Font::from_dict(&d, &no_object).unwrap();
        // 0x0048 = 'H'
        let text = font.decode_bytes(&[0x00, 0x48]);
        assert_eq!(text, "H");
    }

    #[test]
    fn cid_font_type_from_name() {
        assert_eq!(CidFontType::from_name(b"CIDFontType2"), Some(CidFontType::CidFontType2));
        assert_eq!(CidFontType::from_name(b"Unknown"), None);
    }
}

