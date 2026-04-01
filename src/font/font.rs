//! Unified `PdfFont` enum and `FontResolver`.
//!
//! Maps to Java PDFBox `PDFont` (abstract base) and `PDResources.getFont`.
//!
//! This module unifies all font variants (`SimpleFont`, `Type0Font`) under
//! one enum and provides a `FontResolver` that resolves font references
//! from a page's `/Resources` dictionary.

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use super::simple::SimpleFont;
use super::type0::Type0Font;

// ---------------------------------------------------------------------------
// PdfFont
// ---------------------------------------------------------------------------

/// A parsed PDF font — either a simple (single-byte) font or a composite
/// (multi-byte) Type0 font.
///
/// Maps to Java PDFBox `PDFont`.
#[derive(Debug, Clone)]
pub enum PdfFont {
    Simple(SimpleFont),
    Type0(Type0Font),
}

impl PdfFont {
    /// Try to parse a font from a COS dictionary.
    pub fn from_dict(
        dict: &CosDictionary,
        get_object: &dyn Fn(ObjectId) -> Option<CosObject>,
    ) -> Option<Self> {
        let subtype = dict.get_name(&CosName::subtype())?;
        match subtype.as_bytes() {
            b"Type0" => {
                Type0Font::from_dict(dict, get_object).map(Self::Type0)
            }
            b"Type1" | b"MMType1" | b"TrueType" | b"Type3" => {
                SimpleFont::from_dict(dict, get_object).map(Self::Simple)
            }
            _ => None,
        }
    }

    /// Decode bytes to Unicode using this font's encoding / CMap.
    ///
    /// For simple fonts: 1 byte per character.
    /// For Type0 fonts: typically 2 bytes per character (Identity-H).
    pub fn decode_bytes(&self, bytes: &[u8]) -> String {
        match self {
            Self::Simple(f) => f.decode_bytes(bytes),
            Self::Type0(f)  => f.decode_bytes(bytes),
        }
    }

    /// Returns the base font name (PostScript name).
    pub fn base_font_name(&self) -> &str {
        match self {
            Self::Simple(f) => f.base_font_name(),
            Self::Type0(f)  => &f.base_font,
        }
    }

    /// Returns `true` if this is a simple (single-byte) font.
    pub fn is_simple(&self) -> bool {
        matches!(self, Self::Simple(_))
    }

    /// Returns `true` if this is a Type0 composite font.
    pub fn is_type0(&self) -> bool {
        matches!(self, Self::Type0(_))
    }

    /// Returns the `SimpleFont` if this is one.
    pub fn as_simple(&self) -> Option<&SimpleFont> {
        match self { Self::Simple(f) => Some(f), _ => None }
    }

    /// Returns the `Type0Font` if this is one.
    pub fn as_type0(&self) -> Option<&Type0Font> {
        match self { Self::Type0(f) => Some(f), _ => None }
    }

    /// Returns `true` if a ToUnicode CMap is available.
    pub fn has_to_unicode(&self) -> bool {
        match self {
            Self::Simple(f) => f.has_to_unicode(),
            Self::Type0(f)  => f.to_unicode.is_some(),
        }
    }
}

// ---------------------------------------------------------------------------
// FontResolver
// ---------------------------------------------------------------------------

/// Resolves font references from a page's `/Resources/Font` dictionary.
///
/// Maps to `PDResources.getFont(PDFontName)` in Java PDFBox.
///
/// Usage:
/// ```text
/// let resolver = FontResolver::from_resources(resources_dict, &get_object);
/// if let Some(font) = resolver.get_font("F1") {
///     let text = font.decode_bytes(bytes);
/// }
/// ```
pub struct FontResolver {
    fonts: std::collections::HashMap<String, PdfFont>,
}

impl FontResolver {
    /// Build a `FontResolver` from a `/Resources` dictionary.
    ///
    /// All fonts in `/Resources/Font` are parsed eagerly.
    pub fn from_resources(
        resources: &CosDictionary,
        get_object: &dyn Fn(ObjectId) -> Option<CosObject>,
    ) -> Self {
        let mut fonts = std::collections::HashMap::new();

        let font_dict = match resources
            .get(&CosName::new(b"Font".to_vec()))
            .and_then(|v| v.as_dictionary())
        {
            Some(d) => d.clone(),
            None => return Self { fonts },
        };

        for (name, val) in font_dict.iter() {
            let font_name = String::from_utf8_lossy(name.as_bytes()).to_string();
            let dict = match val {
                CosObject::Dictionary(d) => Some(d.clone()),
                CosObject::Reference(id) => {
                    get_object(*id).and_then(|o| o.into_dictionary())
                }
                _ => None,
            };
            if let Some(d) = dict {
                if let Some(font) = PdfFont::from_dict(&d, get_object) {
                    fonts.insert(font_name, font);
                }
            }
        }

        Self { fonts }
    }

    /// Look up a font by its resource name (the name used in `Tf` operator).
    pub fn get_font(&self, name: &str) -> Option<&PdfFont> {
        self.fonts.get(name)
    }

    /// Returns the number of fonts loaded.
    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    /// Returns an iterator over `(name, font)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &PdfFont)> {
        self.fonts.iter().map(|(k, v)| (k.as_str(), v))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject};

    fn no_object(_: ObjectId) -> Option<CosObject> { None }

    fn make_type1_font_dict(name: &str) -> CosDictionary {
        let mut d = CosDictionary::new();
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"Type1".to_vec())));
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(name.as_bytes().to_vec())));
        d.set(CosName::new(b"Encoding".to_vec()), CosObject::Name(CosName::new(b"WinAnsiEncoding".to_vec())));
        d
    }

    fn make_type0_font_dict(name: &str) -> CosDictionary {
        let mut d = CosDictionary::new();
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"Type0".to_vec())));
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(name.as_bytes().to_vec())));
        d.set(CosName::new(b"Encoding".to_vec()), CosObject::Name(CosName::new(b"Identity-H".to_vec())));
        let mut cid_d = CosDictionary::new();
        cid_d.set(CosName::subtype(), CosObject::Name(CosName::new(b"CIDFontType2".to_vec())));
        cid_d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(name.as_bytes().to_vec())));
        let mut csi = CosDictionary::new();
        csi.set(CosName::new(b"Registry".to_vec()), CosObject::String(b"Adobe".to_vec()));
        csi.set(CosName::new(b"Ordering".to_vec()), CosObject::String(b"Identity".to_vec()));
        csi.set(CosName::new(b"Supplement".to_vec()), CosObject::Integer(0));
        cid_d.set(CosName::new(b"CIDSystemInfo".to_vec()), CosObject::Dictionary(csi));
        d.set(CosName::new(b"DescendantFonts".to_vec()),
            CosObject::Array(vec![CosObject::Dictionary(cid_d)]));
        d
    }

    #[test]
    fn pdf_font_from_dict_type1() {
        let d = make_type1_font_dict("Times-Roman");
        let font = PdfFont::from_dict(&d, &no_object).unwrap();
        assert!(font.is_simple());
        assert!(!font.is_type0());
        assert_eq!(font.base_font_name(), "Times-Roman");
    }

    #[test]
    fn pdf_font_from_dict_type0() {
        let d = make_type0_font_dict("Arial");
        let font = PdfFont::from_dict(&d, &no_object).unwrap();
        assert!(font.is_type0());
        assert!(!font.is_simple());
        assert_eq!(font.base_font_name(), "Arial");
    }

    #[test]
    fn pdf_font_decode_bytes_simple() {
        let d = make_type1_font_dict("Courier");
        let font = PdfFont::from_dict(&d, &no_object).unwrap();
        assert_eq!(font.decode_bytes(b"Hi"), "Hi");
    }

    #[test]
    fn pdf_font_unknown_subtype_returns_none() {
        let mut d = CosDictionary::new();
        d.set(CosName::subtype(), CosObject::Name(CosName::new(b"Unknown".to_vec())));
        assert!(PdfFont::from_dict(&d, &no_object).is_none());
    }

    #[test]
    fn pdf_font_as_simple() {
        let d = make_type1_font_dict("Courier");
        let font = PdfFont::from_dict(&d, &no_object).unwrap();
        assert!(font.as_simple().is_some());
        assert!(font.as_type0().is_none());
    }

    #[test]
    fn pdf_font_as_type0() {
        let d = make_type0_font_dict("NotoSans");
        let font = PdfFont::from_dict(&d, &no_object).unwrap();
        assert!(font.as_type0().is_some());
        assert!(font.as_simple().is_none());
    }

    #[test]
    fn font_resolver_from_resources() {
        let f1_dict = make_type1_font_dict("Helvetica");
        let f2_dict = make_type0_font_dict("Arial");

        let mut font_subdict = CosDictionary::new();
        font_subdict.set(CosName::new(b"F1".to_vec()), CosObject::Dictionary(f1_dict));
        font_subdict.set(CosName::new(b"F2".to_vec()), CosObject::Dictionary(f2_dict));

        let mut res = CosDictionary::new();
        res.set(CosName::new(b"Font".to_vec()), CosObject::Dictionary(font_subdict));

        let resolver = FontResolver::from_resources(&res, &no_object);
        assert_eq!(resolver.font_count(), 2);
        assert!(resolver.get_font("F1").is_some());
        assert!(resolver.get_font("F2").is_some());
        assert!(resolver.get_font("F3").is_none());
    }

    #[test]
    fn font_resolver_empty_resources() {
        let res = CosDictionary::new();
        let resolver = FontResolver::from_resources(&res, &no_object);
        assert_eq!(resolver.font_count(), 0);
    }

    #[test]
    fn font_resolver_iter() {
        let f1_dict = make_type1_font_dict("Symbol");
        let mut font_subdict = CosDictionary::new();
        font_subdict.set(CosName::new(b"F1".to_vec()), CosObject::Dictionary(f1_dict));
        let mut res = CosDictionary::new();
        res.set(CosName::new(b"Font".to_vec()), CosObject::Dictionary(font_subdict));
        let resolver = FontResolver::from_resources(&res, &no_object);
        let pairs: Vec<_> = resolver.iter().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "F1");
    }

    #[test]
    fn pdf_font_has_to_unicode_false_by_default() {
        let d = make_type1_font_dict("Courier");
        let font = PdfFont::from_dict(&d, &no_object).unwrap();
        assert!(!font.has_to_unicode());
    }
}

