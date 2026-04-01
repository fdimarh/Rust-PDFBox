//! Simple font parsing — Type1, MMType1, TrueType, Type3.
//!
//! Maps to Java PDFBox `PDSimpleFont` and its subtypes:
//! `PDType1Font`, `PDMMType1Font`, `PDTrueTypeFont`, `PDType3Font`.
//!
//! PDF §9.6 — Simple fonts use a single-byte character code per glyph.

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use super::descriptor::FontDescriptor;
use super::encoding::Encoding;
use super::cmap::{ToUnicodeCMap, parse_to_unicode_cmap};

// ---------------------------------------------------------------------------
// Simple font subtype
// ---------------------------------------------------------------------------

/// Identifies the `/Subtype` of a simple font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleFontSubtype {
    Type1,
    MMType1,
    TrueType,
    Type3,
}

impl SimpleFontSubtype {
    pub fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"Type1"    => Some(Self::Type1),
            b"MMType1"  => Some(Self::MMType1),
            b"TrueType" => Some(Self::TrueType),
            b"Type3"    => Some(Self::Type3),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Type1   => "Type1",
            Self::MMType1 => "MMType1",
            Self::TrueType=> "TrueType",
            Self::Type3   => "Type3",
        }
    }
}

// ---------------------------------------------------------------------------
// Width array
// ---------------------------------------------------------------------------

/// Per-glyph widths (in text space units, 1/1000 of a text unit by default).
/// PDF §9.6.2 — `FirstChar`, `LastChar`, `Widths`.
#[derive(Debug, Clone, Default)]
pub struct GlyphWidths {
    pub first_char: u8,
    pub last_char: u8,
    pub widths: Vec<f64>,
    pub missing_width: f64,
}

impl GlyphWidths {
    /// Parse widths from the font dictionary.
    pub fn from_dict(dict: &CosDictionary) -> Self {
        let first_char = dict.get_int(&CosName::new(b"FirstChar".to_vec()))
            .unwrap_or(0) as u8;
        let last_char  = dict.get_int(&CosName::new(b"LastChar".to_vec()))
            .unwrap_or(0) as u8;
        let widths: Vec<f64> = dict
            .get_array(&CosName::new(b"Widths".to_vec()))
            .map(|arr| arr.iter().filter_map(|v| v.as_number()).collect())
            .unwrap_or_default();
        Self { first_char, last_char, widths, missing_width: 0.0 }
    }

    /// Get the width for a character code.
    pub fn width_for_code(&self, code: u8) -> f64 {
        if code < self.first_char || code > self.last_char {
            return self.missing_width;
        }
        let idx = (code - self.first_char) as usize;
        self.widths.get(idx).copied().unwrap_or(self.missing_width)
    }
}

// ---------------------------------------------------------------------------
// SimpleFont
// ---------------------------------------------------------------------------

/// A parsed simple (single-byte) PDF font.
///
/// Maps to Java PDFBox `PDSimpleFont`.
///
/// Provides:
/// - Subtype identification (Type1, MMType1, TrueType, Type3)
/// - Base font name
/// - Per-character encoding (decoded via `Encoding`)
/// - Per-character widths
/// - Optional `ToUnicodeCMap` for accurate Unicode extraction
/// - Optional `FontDescriptor` for metrics
#[derive(Debug, Clone)]
pub struct SimpleFont {
    /// Font subtype.
    pub subtype: SimpleFontSubtype,
    /// Base font PostScript name (`/BaseFont`).
    pub base_font: String,
    /// Character encoding.
    pub encoding: Encoding,
    /// Per-character widths.
    pub widths: GlyphWidths,
    /// ToUnicode CMap (parsed from an embedded stream), if present.
    pub to_unicode: Option<ToUnicodeCMap>,
    /// Font descriptor, if present.
    pub descriptor: Option<FontDescriptor>,
    /// Object ID of the font descriptor (`/FontDescriptor` reference).
    pub descriptor_ref: Option<ObjectId>,
}

impl SimpleFont {
    /// Parse a `SimpleFont` from the font dictionary.
    ///
    /// `get_stream` is called with an `ObjectId` to resolve stream objects
    /// (used for the ToUnicode CMap stream). Pass `None` if you don't need
    /// ToUnicode resolution.
    pub fn from_dict(
        dict: &CosDictionary,
        get_object: &dyn Fn(ObjectId) -> Option<CosObject>,
    ) -> Option<Self> {
        let subtype = dict.get_name(&CosName::subtype())?;
        let subtype = SimpleFontSubtype::from_name(subtype.as_bytes())?;

        let base_font = dict.get_name(&CosName::new(b"BaseFont".to_vec()))
            .map(|n| String::from_utf8_lossy(n.as_bytes()).to_string())
            .unwrap_or_default();

        // Encoding
        let encoding = dict
            .get(&CosName::new(b"Encoding".to_vec()))
            .map(|enc_obj| Encoding::from_cos(enc_obj))
            .unwrap_or_else(Encoding::win_ansi);

        // Widths
        let mut widths = GlyphWidths::from_dict(dict);

        // ToUnicode — may be an inline stream or an indirect reference
        let to_unicode = resolve_to_unicode(dict, get_object);

        // FontDescriptor
        let descriptor_ref = dict
            .get(&CosName::new(b"FontDescriptor".to_vec()))
            .and_then(|v| v.as_reference());
        let descriptor = descriptor_ref
            .and_then(|id| get_object(id))
            .and_then(|obj| obj.into_dictionary())
            .map(|d| {
                // Also pick up MissingWidth from descriptor
                let desc = FontDescriptor::from_dict(&d);
                widths.missing_width = desc.missing_width;
                desc
            });

        Some(Self { subtype, base_font, encoding, widths, to_unicode, descriptor, descriptor_ref })
    }

    /// Decode a byte sequence to Unicode using CMap → Encoding → Latin-1
    /// fallback (in that priority order).
    pub fn decode_bytes(&self, bytes: &[u8]) -> String {
        if let Some(cmap) = &self.to_unicode {
            let mut out = String::new();
            for &b in bytes {
                if let Some(s) = cmap.to_unicode(b as u32) {
                    out.push_str(&s);
                } else {
                    out.push(self.encoding.decode_byte(b));
                }
            }
            return out;
        }
        self.encoding.decode_bytes(bytes)
    }

    /// Returns the width (in text space, before font size scaling) for a code.
    pub fn width(&self, code: u8) -> f64 {
        self.widths.width_for_code(code)
    }

    /// Returns the PostScript name of the font.
    pub fn base_font_name(&self) -> &str {
        &self.base_font
    }

    /// Returns `true` if a ToUnicode CMap is available.
    pub fn has_to_unicode(&self) -> bool {
        self.to_unicode.is_some()
    }

    /// Returns the font descriptor, if available.
    pub fn descriptor(&self) -> Option<&FontDescriptor> {
        self.descriptor.as_ref()
    }
}

// ---------------------------------------------------------------------------
// Resolve ToUnicode stream
// ---------------------------------------------------------------------------

fn resolve_to_unicode(
    dict: &CosDictionary,
    get_object: &dyn Fn(ObjectId) -> Option<CosObject>,
) -> Option<ToUnicodeCMap> {
    let tu = dict.get(&CosName::new(b"ToUnicode".to_vec()))?;
    match tu {
        CosObject::Stream(s) => Some(parse_to_unicode_cmap(&s.data)),
        CosObject::Reference(id) => {
            let obj = get_object(*id)?;
            match obj {
                CosObject::Stream(s) => Some(parse_to_unicode_cmap(&s.data)),
                _ => None,
            }
        }
        _ => None,
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

    fn make_type1_dict() -> CosDictionary {
        let mut d = CosDictionary::new();
        d.set(CosName::type_name(), CosObject::Name(CosName::new(b"Font".to_vec())));
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"Type1".to_vec())));
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(b"Helvetica".to_vec())));
        d.set(CosName::new(b"Encoding".to_vec()), CosObject::Name(CosName::new(b"WinAnsiEncoding".to_vec())));
        d.set(CosName::new(b"FirstChar".to_vec()), CosObject::Integer(32));
        d.set(CosName::new(b"LastChar".to_vec()), CosObject::Integer(122));
        let widths: Vec<CosObject> = (32u8..=122u8)
            .map(|_| CosObject::Integer(556))
            .collect();
        d.set(CosName::new(b"Widths".to_vec()), CosObject::Array(widths));
        d
    }

    #[test]
    fn simple_font_subtype_type1() {
        let d = make_type1_dict();
        let font = SimpleFont::from_dict(&d, &no_object).unwrap();
        assert_eq!(font.subtype, SimpleFontSubtype::Type1);
    }

    #[test]
    fn simple_font_base_font_name() {
        let d = make_type1_dict();
        let font = SimpleFont::from_dict(&d, &no_object).unwrap();
        assert_eq!(font.base_font_name(), "Helvetica");
    }

    #[test]
    fn simple_font_decode_ascii() {
        let d = make_type1_dict();
        let font = SimpleFont::from_dict(&d, &no_object).unwrap();
        assert_eq!(font.decode_bytes(b"Hello"), "Hello");
    }

    #[test]
    fn simple_font_width_for_code() {
        let d = make_type1_dict();
        let font = SimpleFont::from_dict(&d, &no_object).unwrap();
        // Code 32 (space) is at index 0 in widths array = 556
        assert_eq!(font.width(32), 556.0);
        // Code below first_char → missing_width = 0
        assert_eq!(font.width(0), 0.0);
    }

    #[test]
    fn simple_font_no_to_unicode_by_default() {
        let d = make_type1_dict();
        let font = SimpleFont::from_dict(&d, &no_object).unwrap();
        assert!(!font.has_to_unicode());
    }

    #[test]
    fn simple_font_to_unicode_inline_stream() {
        let cmap_data = b"begincmap\n1 beginbfchar\n<41><0041>\nendbfchar\nendcmap\n".to_vec();
        let mut d = make_type1_dict();
        d.set(
            CosName::new(b"ToUnicode".to_vec()),
            CosObject::Stream(CosStream::new(CosDictionary::new(), cmap_data)),
        );
        let font = SimpleFont::from_dict(&d, &no_object).unwrap();
        assert!(font.has_to_unicode());
        assert_eq!(font.decode_bytes(b"\x41"), "A");
    }

    #[test]
    fn simple_font_to_unicode_via_reference() {
        let cmap_data = b"begincmap\n1 beginbfchar\n<42><0042>\nendbfchar\nendcmap\n".to_vec();
        let stream = CosObject::Stream(CosStream::new(CosDictionary::new(), cmap_data));
        let ref_id = ObjectId::new(10, 0);

        let mut d = make_type1_dict();
        d.set(CosName::new(b"ToUnicode".to_vec()), CosObject::Reference(ref_id));

        let get_object = |id: ObjectId| -> Option<CosObject> {
            if id == ref_id { Some(stream.clone()) } else { None }
        };
        let font = SimpleFont::from_dict(&d, &get_object).unwrap();
        assert!(font.has_to_unicode());
        assert_eq!(font.decode_bytes(b"\x42"), "B");
    }

    #[test]
    fn simple_font_subtype_truetype() {
        let mut d = make_type1_dict();
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"TrueType".to_vec())));
        let font = SimpleFont::from_dict(&d, &no_object).unwrap();
        assert_eq!(font.subtype, SimpleFontSubtype::TrueType);
    }

    #[test]
    fn simple_font_unknown_subtype_returns_none() {
        let mut d = make_type1_dict();
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"Unknown".to_vec())));
        assert!(SimpleFont::from_dict(&d, &no_object).is_none());
    }

    #[test]
    fn glyph_widths_missing_width() {
        let mut d = CosDictionary::new();
        d.set(CosName::new(b"FirstChar".to_vec()), CosObject::Integer(65));
        d.set(CosName::new(b"LastChar".to_vec()),  CosObject::Integer(65));
        d.set(CosName::new(b"Widths".to_vec()),    CosObject::Array(vec![CosObject::Integer(500)]));
        let gw = GlyphWidths::from_dict(&d);
        assert_eq!(gw.width_for_code(65), 500.0);
        assert_eq!(gw.width_for_code(66), 0.0); // outside range → missing_width
    }

    #[test]
    fn subtype_as_str() {
        assert_eq!(SimpleFontSubtype::Type1.as_str(), "Type1");
        assert_eq!(SimpleFontSubtype::TrueType.as_str(), "TrueType");
    }
}

