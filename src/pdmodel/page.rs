//! PDF page model — `PDPage` and page-level attribute access.
//!
//! Maps to Java PDFBox `PDPage`. A page is a dictionary (from the page
//! tree) plus helper methods to access its attributes and content streams.
//!
//! # Java PDFBox mapping
//!
//! | Java class / method | Rust type / method |
//! |---|---|
//! | `PDPage` | [`Page`] |
//! | `PDPage.getMediaBox()` | [`Page::media_box`] |
//! | `PDPage.getRotation()` | [`Page::rotation`] |
//! | `PDPage.getResources()` | [`Page::resources`] |
//! | `PDPage.getContents()` | [`Page::content_stream_data`] |

use crate::cos::{CosDictionary, CosName, CosObject};

// ---------------------------------------------------------------------------
// Rectangle
// ---------------------------------------------------------------------------

/// An axis-aligned rectangle used for page boxes (MediaBox, CropBox, etc.).
///
/// All values are in user-space units (points, 1/72 inch by default).
#[derive(Debug, Clone, PartialEq)]
pub struct Rectangle {
    pub lower_left_x: f64,
    pub lower_left_y: f64,
    pub upper_right_x: f64,
    pub upper_right_y: f64,
}

impl Rectangle {
    pub fn new(llx: f64, lly: f64, urx: f64, ury: f64) -> Self {
        Self {
            lower_left_x: llx,
            lower_left_y: lly,
            upper_right_x: urx,
            upper_right_y: ury,
        }
    }

    /// Width of the rectangle.
    #[inline]
    pub fn width(&self) -> f64 {
        (self.upper_right_x - self.lower_left_x).abs()
    }

    /// Height of the rectangle.
    #[inline]
    pub fn height(&self) -> f64 {
        (self.upper_right_y - self.lower_left_y).abs()
    }
}

impl std::fmt::Display for Rectangle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{} {} {} {}]",
            self.lower_left_x,
            self.lower_left_y,
            self.upper_right_x,
            self.upper_right_y
        )
    }
}

/// Parses a `[llx lly urx ury]` COS array into a [`Rectangle`].
pub fn rectangle_from_cos(obj: &CosObject) -> Option<Rectangle> {
    let arr = obj.as_array()?;
    if arr.len() != 4 {
        return None;
    }
    let llx = arr[0].as_number()?;
    let lly = arr[1].as_number()?;
    let urx = arr[2].as_number()?;
    let ury = arr[3].as_number()?;
    Some(Rectangle::new(llx, lly, urx, ury))
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// The resources dictionary for a page or form XObject.
///
/// Maps to Java PDFBox `PDResources`.
#[derive(Debug, Clone)]
pub struct Resources<'a> {
    dict: &'a CosDictionary,
}

impl<'a> Resources<'a> {
    pub fn new(dict: &'a CosDictionary) -> Self {
        Self { dict }
    }

    /// Returns the raw resources dictionary.
    pub fn dictionary(&self) -> &CosDictionary {
        self.dict
    }

    /// Returns the font sub-dictionary if present.
    pub fn font_dict(&self) -> Option<&CosDictionary> {
        self.dict
            .get(&CosName::new(b"Font".to_vec()))
            .and_then(|v| v.as_dictionary())
    }

    /// Returns the XObject sub-dictionary if present.
    pub fn xobject_dict(&self) -> Option<&CosDictionary> {
        self.dict
            .get(&CosName::new(b"XObject".to_vec()))
            .and_then(|v| v.as_dictionary())
    }

    /// Returns the ExtGState sub-dictionary if present.
    pub fn ext_gstate_dict(&self) -> Option<&CosDictionary> {
        self.dict
            .get(&CosName::new(b"ExtGState".to_vec()))
            .and_then(|v| v.as_dictionary())
    }

    /// Returns the ColorSpace sub-dictionary if present.
    pub fn color_space_dict(&self) -> Option<&CosDictionary> {
        self.dict
            .get(&CosName::new(b"ColorSpace".to_vec()))
            .and_then(|v| v.as_dictionary())
    }
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

/// A single PDF page, wrapping its page-node dictionary.
///
/// Maps to Java PDFBox `PDPage`. The page dictionary is borrowed from the
/// object store; content stream bytes are retrieved on demand.
#[derive(Debug, Clone)]
pub struct Page<'a> {
    /// The page's own dictionary (already merged with inherited attributes).
    dict: &'a CosDictionary,
    /// Index of this page within the document (0-based).
    pub index: usize,
}

impl<'a> Page<'a> {
    /// Creates a new page from its dictionary and 0-based index.
    pub fn new(dict: &'a CosDictionary, index: usize) -> Self {
        Self { dict, index }
    }

    /// Returns the raw page dictionary.
    pub fn dictionary(&self) -> &CosDictionary {
        self.dict
    }

    /// Returns the MediaBox for this page.
    ///
    /// In a well-formed PDF the MediaBox is always present (inherited or
    /// direct). Returns `None` only for malformed pages.
    pub fn media_box(&self) -> Option<Rectangle> {
        self.dict
            .get(&CosName::new(b"MediaBox".to_vec()))
            .and_then(rectangle_from_cos)
    }

    /// Returns the CropBox if defined, otherwise the MediaBox.
    pub fn crop_box(&self) -> Option<Rectangle> {
        self.dict
            .get(&CosName::new(b"CropBox".to_vec()))
            .and_then(rectangle_from_cos)
            .or_else(|| self.media_box())
    }

    /// Returns the page rotation in degrees (0, 90, 180, or 270).
    pub fn rotation(&self) -> i64 {
        self.dict
            .get_int(&CosName::new(b"Rotate".to_vec()))
            .unwrap_or(0)
            % 360
    }

    /// Returns the Resources wrapper if the page has a `/Resources` dictionary.
    pub fn resources(&self) -> Option<Resources<'_>> {
        self.dict
            .get(&CosName::resources())
            .and_then(|v| v.as_dictionary())
            .map(Resources::new)
    }

    /// Returns the raw `/Contents` object (reference, array of refs, or stream).
    pub fn contents_object(&self) -> Option<&CosObject> {
        self.dict.get(&CosName::contents())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject};

    fn make_page_dict(media_box: &[f64; 4], rotation: Option<i64>) -> CosDictionary {
        let mut d = CosDictionary::new();
        d.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Page".to_vec())));
        d.insert(
            CosName::new(b"MediaBox".to_vec()),
            CosObject::Array(vec![
                CosObject::Real(media_box[0]),
                CosObject::Real(media_box[1]),
                CosObject::Real(media_box[2]),
                CosObject::Real(media_box[3]),
            ]),
        );
        if let Some(r) = rotation {
            d.insert(CosName::new(b"Rotate".to_vec()), CosObject::Integer(r));
        }
        d
    }

    #[test]
    fn page_media_box() {
        let d = make_page_dict(&[0.0, 0.0, 612.0, 792.0], None);
        let page = Page::new(&d, 0);
        let mb = page.media_box().unwrap();
        assert_eq!(mb.width(), 612.0);
        assert_eq!(mb.height(), 792.0);
    }

    #[test]
    fn page_rotation_default() {
        let d = make_page_dict(&[0.0, 0.0, 612.0, 792.0], None);
        let page = Page::new(&d, 0);
        assert_eq!(page.rotation(), 0);
    }

    #[test]
    fn page_rotation_explicit() {
        let d = make_page_dict(&[0.0, 0.0, 612.0, 792.0], Some(90));
        let page = Page::new(&d, 0);
        assert_eq!(page.rotation(), 90);
    }

    #[test]
    fn page_crop_box_falls_back_to_media_box() {
        let d = make_page_dict(&[0.0, 0.0, 612.0, 792.0], None);
        let page = Page::new(&d, 0);
        // No CropBox in dict → falls back to MediaBox.
        let cb = page.crop_box().unwrap();
        assert_eq!(cb.width(), 612.0);
    }

    #[test]
    fn rectangle_dimensions() {
        let r = Rectangle::new(10.0, 20.0, 110.0, 120.0);
        assert_eq!(r.width(), 100.0);
        assert_eq!(r.height(), 100.0);
    }

    #[test]
    fn resources_font_dict() {
        let mut font_dict = CosDictionary::new();
        font_dict.insert(CosName::new(b"F1".to_vec()), CosObject::Null);
        let mut res_dict = CosDictionary::new();
        res_dict.insert(
            CosName::new(b"Font".to_vec()),
            CosObject::Dictionary(font_dict),
        );
        let res = Resources::new(&res_dict);
        assert!(res.font_dict().is_some());
        assert_eq!(res.font_dict().unwrap().len(), 1);
    }
}

