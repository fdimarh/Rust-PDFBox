//!
//! Destination types for document outline items and link annotations.
//!
//! Maps to `PDPageDestination` / `PDPageXYZDestination` etc. in Java PDFBox.

use crate::cos::{CosDictionary, CosName, CosObject};
use crate::ObjectId;

/// How to fit the destination page into the viewer window.
#[derive(Debug, Clone, PartialEq)]
pub enum FitMode {
    /// Display the page with its /MediaBox dimensions (`/Fit`).
    Fit,
    /// Fit the page width to the window (`/FitH`). Optional `top` is the y-coordinate.
    FitH(Option<f64>),
    /// Fit the page height to the window (`/FitV`). Optional `left` is the x-coordinate.
    FitV(Option<f64>),
    /// Fit the rectangle given by `(left, bottom, right, top)` (`/FitR`).
    FitR(f64, f64, f64, f64),
    /// Fit the page's bounding box (`/FitB`).
    FitB,
    /// Fit the bounding box width (`/FitBH`). Optional `top`.
    FitBH(Option<f64>),
    /// Fit the bounding box height (`/FitBV`). Optional `left`.
    FitBV(Option<f64>),
    /// Show page at `(left, top)` with optional zoom (`/XYZ`).
    XYZ(Option<f64>, Option<f64>, Option<f64>),
}

/// A PDF destination — specifies a target page and how to display it.
///
/// Maps to Java PDFBox's `PDPageDestination` hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    /// Go to a page in the same document.
    GoTo {
        page_index: usize,
        fit: FitMode,
    },
    /// Go to a page in an external document (`/GoToR`).
    GoToR {
        file: String,
        page_index: usize,
        fit: FitMode,
    },
    /// Go to a URI (`/URI` action). Usually used with an action dictionary.
    URI(String),
}

impl Destination {
    /// Creates a `GoTo` destination with a specific fit mode.
    pub fn goto_page(page_index: usize, fit: FitMode) -> Self {
        Destination::GoTo { page_index, fit }
    }

    /// Creates a `GoToR` (remote) destination.
    pub fn goto_remote(file: &str, page_index: usize, fit: FitMode) -> Self {
        Destination::GoToR {
            file: file.to_string(),
            page_index,
            fit,
        }
    }

    /// Creates a `URI` destination.
    pub fn uri(url: &str) -> Self {
        Destination::URI(url.to_string())
    }

    /// Parses a `Destination` from a COS value: either a direct array `[page /Fit ...]`
    /// or an action dictionary `{/Type /Action /S /GoTo /D [...]}`.
    ///
    /// `page_id_to_index` is a closure that maps page ObjectIds to 0-based indices.
    pub fn from_cos(
        obj: &CosObject,
        page_id_to_index: &impl Fn(ObjectId) -> Option<usize>,
    ) -> Option<Self> {
        // Try direct destination array: [page_ref /Fit ...]
        if let Some(arr) = obj.as_array() {
            return Self::from_array(arr, page_id_to_index);
        }

        // Try action dictionary with /D entry
        if let Some(dict) = obj.as_dictionary() {
            if let Some(d) = dict.get(&CosName::new(b"D".to_vec())) {
                if let Some(arr) = d.as_array() {
                    return Self::from_array(arr, page_id_to_index);
                }
            }
            // Try /S /URI action
            if let Some(s) = dict.get(&CosName::new(b"S".to_vec())) {
                if let Some(name) = s.as_name() {
                    if name.as_bytes() == b"URI" {
                        if let Some(uri) = dict.get(&CosName::new(b"URI".to_vec())) {
                            if let Some(s) = uri.as_string() {
                                return Some(Destination::URI(String::from_utf8_lossy(s).to_string()));
                            }
                        }
                    }
                    if name.as_bytes() == b"GoToR" {
                        let file = dict
                            .get(&CosName::new(b"F".to_vec()))
                            .and_then(|v| v.as_string())
                            .map(|s| String::from_utf8_lossy(s).to_string())
                            .unwrap_or_default();
                        if let Some(d) = dict.get(&CosName::new(b"D".to_vec())) {
                            if let Some(arr) = d.as_array() {
                                if let Some(dest) = Self::from_array(arr, page_id_to_index) {
                                    match dest {
                                        Destination::GoTo { page_index, fit } => {
                                            return Some(Destination::GoToR { file, page_index, fit });
                                        }
                                        other => return Some(other),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn from_array(arr: &[CosObject], page_id_to_index: &impl Fn(ObjectId) -> Option<usize>) -> Option<Self> {
        if arr.is_empty() {
            return None;
        }

        // First element: page reference
        let page_id = arr[0].as_reference()?;
        let page_index = page_id_to_index(page_id)?;

        if arr.len() < 2 {
            return Some(Destination::GoTo {
                page_index,
                fit: FitMode::Fit,
            });
        }

        let fit_name = arr[1].as_name()?;
        let fit = match fit_name.as_bytes() {
            b"Fit" => FitMode::Fit,
            b"FitH" => FitMode::FitH(arr.get(2).and_then(|v| v.as_real())),
            b"FitV" => FitMode::FitV(arr.get(2).and_then(|v| v.as_real())),
            b"FitR" => FitMode::FitR(
                arr.get(2).and_then(|v| v.as_real()).unwrap_or(0.0),
                arr.get(3).and_then(|v| v.as_real()).unwrap_or(0.0),
                arr.get(4).and_then(|v| v.as_real()).unwrap_or(0.0),
                arr.get(5).and_then(|v| v.as_real()).unwrap_or(0.0),
            ),
            b"FitB" => FitMode::FitB,
            b"FitBH" => FitMode::FitBH(arr.get(2).and_then(|v| v.as_real())),
            b"FitBV" => FitMode::FitBV(arr.get(2).and_then(|v| v.as_real())),
            b"XYZ" => FitMode::XYZ(
                arr.get(2).and_then(|v| v.as_real()),
                arr.get(3).and_then(|v| v.as_real()),
                arr.get(4).and_then(|v| v.as_real()),
            ),
            _ => return None,
        };

        Some(Destination::GoTo { page_index, fit })
    }

    /// Serializes this destination into a COS array suitable for PDF output.
    pub fn to_cos_array(&self, index_to_page_id: &impl Fn(usize) -> Option<ObjectId>) -> CosObject {
        match self {
            Destination::GoTo { page_index, fit } => {
                let page_ref = index_to_page_id(*page_index)
                    .map(|id| CosObject::Reference(id))
                    .unwrap_or(CosObject::Null);
                let mut arr = vec![page_ref];
                match fit {
                    FitMode::Fit => arr.push(CosObject::Name(CosName::new(b"Fit".to_vec()))),
                    FitMode::FitH(top) => {
                        arr.push(CosObject::Name(CosName::new(b"FitH".to_vec())));
                        if let Some(t) = top {
                            arr.push(CosObject::Real(*t));
                        }
                    }
                    FitMode::FitV(left) => {
                        arr.push(CosObject::Name(CosName::new(b"FitV".to_vec())));
                        if let Some(l) = left {
                            arr.push(CosObject::Real(*l));
                        }
                    }
                    FitMode::FitR(l, b, r, t) => {
                        arr.push(CosObject::Name(CosName::new(b"FitR".to_vec())));
                        arr.push(CosObject::Real(*l));
                        arr.push(CosObject::Real(*b));
                        arr.push(CosObject::Real(*r));
                        arr.push(CosObject::Real(*t));
                    }
                    FitMode::FitB => arr.push(CosObject::Name(CosName::new(b"FitB".to_vec()))),
                    FitMode::FitBH(top) => {
                        arr.push(CosObject::Name(CosName::new(b"FitBH".to_vec())));
                        if let Some(t) = top {
                            arr.push(CosObject::Real(*t));
                        }
                    }
                    FitMode::FitBV(left) => {
                        arr.push(CosObject::Name(CosName::new(b"FitBV".to_vec())));
                        if let Some(l) = left {
                            arr.push(CosObject::Real(*l));
                        }
                    }
                    FitMode::XYZ(left, top, zoom) => {
                        arr.push(CosObject::Name(CosName::new(b"XYZ".to_vec())));
                        arr.push(left.map(CosObject::Real).unwrap_or(CosObject::Null));
                        arr.push(top.map(CosObject::Real).unwrap_or(CosObject::Null));
                        arr.push(zoom.map(CosObject::Real).unwrap_or(CosObject::Null));
                    }
                }
                CosObject::Array(arr)
            }
            Destination::GoToR { file, page_index, fit } => {
                // Produces an action dictionary with /S /GoToR
                let page_ref = index_to_page_id(*page_index)
                    .map(|id| CosObject::Reference(id))
                    .unwrap_or(CosObject::Null);
                let mut d_arr = vec![page_ref];
                match fit {
                    FitMode::Fit => d_arr.push(CosObject::Name(CosName::new(b"Fit".to_vec()))),
                    FitMode::FitH(top) => {
                        d_arr.push(CosObject::Name(CosName::new(b"FitH".to_vec())));
                        if let Some(t) = top { d_arr.push(CosObject::Real(*t)); }
                    }
                    FitMode::FitV(left) => {
                        d_arr.push(CosObject::Name(CosName::new(b"FitV".to_vec())));
                        if let Some(l) = left { d_arr.push(CosObject::Real(*l)); }
                    }
                    FitMode::FitR(l, b, r, t) => {
                        d_arr.push(CosObject::Name(CosName::new(b"FitR".to_vec())));
                        d_arr.push(CosObject::Real(*l)); d_arr.push(CosObject::Real(*b));
                        d_arr.push(CosObject::Real(*r)); d_arr.push(CosObject::Real(*t));
                    }
                    FitMode::FitB => d_arr.push(CosObject::Name(CosName::new(b"FitB".to_vec()))),
                    FitMode::FitBH(top) => {
                        d_arr.push(CosObject::Name(CosName::new(b"FitBH".to_vec())));
                        if let Some(t) = top { d_arr.push(CosObject::Real(*t)); }
                    }
                    FitMode::FitBV(left) => {
                        d_arr.push(CosObject::Name(CosName::new(b"FitBV".to_vec())));
                        if let Some(l) = left { d_arr.push(CosObject::Real(*l)); }
                    }
                    FitMode::XYZ(left, top, zoom) => {
                        d_arr.push(CosObject::Name(CosName::new(b"XYZ".to_vec())));
                        d_arr.push(left.map(CosObject::Real).unwrap_or(CosObject::Null));
                        d_arr.push(top.map(CosObject::Real).unwrap_or(CosObject::Null));
                        d_arr.push(zoom.map(CosObject::Real).unwrap_or(CosObject::Null));
                    }
                }
                let mut dict = CosDictionary::new();
                dict.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Action".to_vec())));
                dict.insert(CosName::new(b"S".to_vec()), CosObject::Name(CosName::new(b"GoToR".to_vec())));
                dict.insert(CosName::new(b"F".to_vec()), CosObject::String(file.as_bytes().to_vec()));
                dict.insert(CosName::new(b"D".to_vec()), CosObject::Array(d_arr));
                CosObject::Dictionary(dict)
            }
            Destination::URI(uri) => {
                let mut dict = CosDictionary::new();
                dict.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Action".to_vec())));
                dict.insert(CosName::new(b"S".to_vec()), CosObject::Name(CosName::new(b"URI".to_vec())));
                dict.insert(CosName::new(b"URI".to_vec()), CosObject::String(uri.as_bytes().to_vec()));
                CosObject::Dictionary(dict)
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destination_goto_fit() {
        let dest = Destination::goto_page(0, FitMode::Fit);
        assert_eq!(
            dest,
            Destination::GoTo {
                page_index: 0,
                fit: FitMode::Fit
            }
        );
    }

    #[test]
    fn test_destination_goto_xyz() {
        let dest = Destination::goto_page(2, FitMode::XYZ(Some(100.0), Some(200.0), Some(1.5)));
        if let Destination::GoTo { page_index: 2, fit: FitMode::XYZ(Some(l), Some(t), Some(z)) } = dest {
            assert!((l - 100.0).abs() < f64::EPSILON);
            assert!((t - 200.0).abs() < f64::EPSILON);
            assert!((z - 1.5).abs() < f64::EPSILON);
        } else {
            panic!("unexpected destination: {:?}", dest);
        }
    }

    #[test]
    fn test_destination_remote() {
        let dest = Destination::goto_remote("other.pdf", 1, FitMode::FitH(Some(50.0)));
        if let Destination::GoToR { file, page_index: 1, fit: FitMode::FitH(Some(t)) } = dest {
            assert_eq!(file, "other.pdf");
            assert!((t - 50.0).abs() < f64::EPSILON);
        } else {
            panic!("unexpected destination: {:?}", dest);
        }
    }

    #[test]
    fn test_destination_uri() {
        let dest = Destination::uri("https://example.com");
        assert_eq!(dest, Destination::URI("https://example.com".to_string()));
    }

    #[test]
    fn test_fit_mode_variants() {
        let modes = [
            FitMode::Fit,
            FitMode::FitH(Some(100.0)),
            FitMode::FitV(None),
            FitMode::FitR(0.0, 0.0, 612.0, 792.0),
            FitMode::FitB,
            FitMode::FitBH(None),
            FitMode::FitBV(Some(50.0)),
            FitMode::XYZ(None, None, None),
        ];
        assert_eq!(modes.len(), 8);
    }
}
