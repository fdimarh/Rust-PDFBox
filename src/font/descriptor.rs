//! Font descriptor — `PDFontDescriptor`.
//!
//! Parses the `/FontDescriptor` dictionary attached to simple and CID fonts.
//! Maps to Java PDFBox `PDFontDescriptor`.
//!
//! PDF §9.8 Table 122.

use crate::cos::{CosDictionary, CosName};

// ---------------------------------------------------------------------------
// FontFlags (PDF §9.8.3.1, Table 123)
// ---------------------------------------------------------------------------

/// Bitfield of font flags from the `/Flags` entry of a font descriptor.
///
/// Corresponds to Java PDFBox `PDFontDescriptor.getFontFlag(int)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FontFlags(pub u32);

impl FontFlags {
    pub const FIXED_PITCH:      u32 = 1 << 0;  // bit 1
    pub const SERIF:            u32 = 1 << 1;  // bit 2
    pub const SYMBOLIC:         u32 = 1 << 2;  // bit 3
    pub const SCRIPT:           u32 = 1 << 3;  // bit 4
    pub const NON_SYMBOLIC:     u32 = 1 << 5;  // bit 6
    pub const ITALIC:           u32 = 1 << 6;  // bit 7
    pub const ALL_CAP:          u32 = 1 << 16; // bit 17
    pub const SMALL_CAP:        u32 = 1 << 17; // bit 18
    pub const FORCE_BOLD:       u32 = 1 << 18; // bit 19

    pub fn is_fixed_pitch(self)   -> bool { self.0 & Self::FIXED_PITCH   != 0 }
    pub fn is_serif(self)         -> bool { self.0 & Self::SERIF         != 0 }
    pub fn is_symbolic(self)      -> bool { self.0 & Self::SYMBOLIC      != 0 }
    pub fn is_italic(self)        -> bool { self.0 & Self::ITALIC        != 0 }
    pub fn is_non_symbolic(self)  -> bool { self.0 & Self::NON_SYMBOLIC  != 0 }
    pub fn is_all_cap(self)       -> bool { self.0 & Self::ALL_CAP       != 0 }
    pub fn is_small_cap(self)     -> bool { self.0 & Self::SMALL_CAP     != 0 }
    pub fn is_force_bold(self)    -> bool { self.0 & Self::FORCE_BOLD    != 0 }
}

// ---------------------------------------------------------------------------
// BoundingBox
// ---------------------------------------------------------------------------

/// Font bounding box from `/FontBBox` (four numbers: llx lly urx ury).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontBBox {
    pub llx: f64,
    pub lly: f64,
    pub urx: f64,
    pub ury: f64,
}

impl FontBBox {
    pub fn width(&self)  -> f64 { (self.urx - self.llx).abs() }
    pub fn height(&self) -> f64 { (self.ury - self.lly).abs() }
}

// ---------------------------------------------------------------------------
// FontDescriptor
// ---------------------------------------------------------------------------

/// Parsed `/FontDescriptor` dictionary.
///
/// Maps to Java PDFBox `PDFontDescriptor`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontDescriptor {
    /// PostScript name (`/FontName`).
    pub font_name: String,
    /// Font family name (`/FontFamily`), optional.
    pub font_family: Option<String>,
    /// Font flags bitfield (`/Flags`).
    pub flags: FontFlags,
    /// Font bounding box (`/FontBBox`).
    pub font_bbox: FontBBox,
    /// Italic angle in degrees (`/ItalicAngle`).
    pub italic_angle: f64,
    /// Ascent in glyph-space units (`/Ascent`).
    pub ascent: f64,
    /// Descent (negative) in glyph-space units (`/Descent`).
    pub descent: f64,
    /// Leading (`/Leading`).
    pub leading: f64,
    /// Cap height (`/CapHeight`).
    pub cap_height: f64,
    /// X-height (`/XHeight`).
    pub x_height: f64,
    /// Stem-V thickness (`/StemV`).
    pub stem_v: f64,
    /// Stem-H thickness (`/StemH`).
    pub stem_h: f64,
    /// Average glyph width (`/AvgWidth`).
    pub avg_width: f64,
    /// Maximum glyph width (`/MaxWidth`).
    pub max_width: f64,
    /// Missing width default (`/MissingWidth`).
    pub missing_width: f64,
}

impl FontDescriptor {
    /// Parse a `FontDescriptor` from a COS dictionary.
    pub fn from_dict(dict: &CosDictionary) -> Self {
        let mut desc = Self::default();

        if let Some(n) = dict.get_name(&CosName::new(b"FontName".to_vec())) {
            desc.font_name = String::from_utf8_lossy(n.as_bytes()).to_string();
        }
        if let Some(v) = dict.get(&CosName::new(b"FontFamily".to_vec())) {
            if let Some(s) = v.as_string() {
                desc.font_family = Some(String::from_utf8_lossy(s).to_string());
            }
        }
        if let Some(f) = dict.get_int(&CosName::new(b"Flags".to_vec())) {
            desc.flags = FontFlags(f as u32);
        }
        if let Some(arr) = dict.get_array(&CosName::new(b"FontBBox".to_vec())) {
            let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_number()).collect();
            if nums.len() >= 4 {
                desc.font_bbox = FontBBox { llx: nums[0], lly: nums[1], urx: nums[2], ury: nums[3] };
            }
        }

        let num = |key: &[u8]| -> f64 {
            dict.get_number(&CosName::new(key.to_vec())).unwrap_or(0.0)
        };

        desc.italic_angle  = num(b"ItalicAngle");
        desc.ascent        = num(b"Ascent");
        desc.descent       = num(b"Descent");
        desc.leading       = num(b"Leading");
        desc.cap_height    = num(b"CapHeight");
        desc.x_height      = num(b"XHeight");
        desc.stem_v        = num(b"StemV");
        desc.stem_h        = num(b"StemH");
        desc.avg_width     = num(b"AvgWidth");
        desc.max_width     = num(b"MaxWidth");
        desc.missing_width = num(b"MissingWidth");
        desc
    }

    /// `true` if the font is fixed-pitch (monospace).
    pub fn is_fixed_pitch(&self) -> bool { self.flags.is_fixed_pitch() }
    /// `true` if the font has serifs.
    pub fn is_serif(&self) -> bool { self.flags.is_serif() }
    /// `true` if the font uses symbolic encoding.
    pub fn is_symbolic(&self) -> bool { self.flags.is_symbolic() }
    /// `true` if the font is italic or oblique.
    pub fn is_italic(&self) -> bool { self.flags.is_italic() }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject};

    fn make_desc_dict() -> CosDictionary {
        let mut d = CosDictionary::new();
        d.set(CosName::new(b"FontName".to_vec()), CosObject::Name(CosName::new(b"Helvetica".to_vec())));
        d.set(CosName::new(b"Flags".to_vec()), CosObject::Integer(32)); // NON_SYMBOLIC bit 6
        d.set(CosName::new(b"FontBBox".to_vec()),
            CosObject::Array(vec![
                CosObject::Integer(-166), CosObject::Integer(-225),
                CosObject::Integer(1000), CosObject::Integer(931),
            ]));
        d.set(CosName::new(b"ItalicAngle".to_vec()), CosObject::Integer(0));
        d.set(CosName::new(b"Ascent".to_vec()), CosObject::Integer(718));
        d.set(CosName::new(b"Descent".to_vec()), CosObject::Integer(-207));
        d.set(CosName::new(b"CapHeight".to_vec()), CosObject::Integer(718));
        d.set(CosName::new(b"StemV".to_vec()), CosObject::Integer(88));
        d.set(CosName::new(b"MissingWidth".to_vec()), CosObject::Integer(278));
        d
    }

    #[test]
    fn descriptor_font_name() {
        let d = FontDescriptor::from_dict(&make_desc_dict());
        assert_eq!(d.font_name, "Helvetica");
    }

    #[test]
    fn descriptor_flags_non_symbolic() {
        let d = FontDescriptor::from_dict(&make_desc_dict());
        assert!(d.is_non_symbolic());
        assert!(!d.is_symbolic());
    }

    #[test]
    fn descriptor_bbox() {
        let d = FontDescriptor::from_dict(&make_desc_dict());
        assert_eq!(d.font_bbox.llx, -166.0);
        assert_eq!(d.font_bbox.ury, 931.0);
    }

    #[test]
    fn descriptor_metrics() {
        let d = FontDescriptor::from_dict(&make_desc_dict());
        assert_eq!(d.ascent, 718.0);
        assert_eq!(d.descent, -207.0);
        assert_eq!(d.cap_height, 718.0);
        assert_eq!(d.stem_v, 88.0);
        assert_eq!(d.missing_width, 278.0);
    }

    #[test]
    fn descriptor_missing_keys_default() {
        let d = FontDescriptor::from_dict(&CosDictionary::new());
        assert_eq!(d.font_name, "");
        assert_eq!(d.ascent, 0.0);
        assert_eq!(d.flags.0, 0);
    }

    #[test]
    fn font_flags_fixed_pitch() {
        let f = FontFlags(FontFlags::FIXED_PITCH);
        assert!(f.is_fixed_pitch());
        assert!(!f.is_italic());
    }

    #[test]
    fn font_flags_italic() {
        let f = FontFlags(FontFlags::ITALIC);
        assert!(f.is_italic());
        assert!(!f.is_serif());
    }

    #[test]
    fn font_bbox_dimensions() {
        let bb = FontBBox { llx: -100.0, lly: -200.0, urx: 900.0, ury: 800.0 };
        assert_eq!(bb.width(), 1000.0);
        assert_eq!(bb.height(), 1000.0);
    }
}

// Extra method needed in tests
impl FontDescriptor {
    pub fn is_non_symbolic(&self) -> bool { self.flags.is_non_symbolic() }
}

