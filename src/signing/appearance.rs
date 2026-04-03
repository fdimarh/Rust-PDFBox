//! Signature appearance (visual) stream builder.
//!
//! Builds the `/AP` Form XObject that PDF viewers render inside the
//! signature widget rectangle.
//!
//! ## Layout (mirrors Java PDFBox `PDVisibleSignDesigner`)
//!
//! ```text
//! ┌───────────────────────────────────────┐
//! │  [PNG/JPEG image — if supplied]       │
//! │  Signer: <name>                       │
//! │  Date:   <M value>                    │
//! │  Reason: <reason>                     │
//! └───────────────────────────────────────┘
//! ```
//!
//! When no image is provided the full rectangle is filled with the text
//! block only (matching what most PDF viewers display for a text-only
//! signature appearance).
//!
//! ## PDF object structure produced
//!
//! ```text
//! <ap_id> 0 obj
//!   << /Type /XObject  /Subtype /Form
//!      /BBox [0 0 <w> <h>]
//!      /Resources << /Font << /F1 <font_id> 0 R >>
//!                    /XObject << /Img <img_id> 0 R >>   ← only if image present
//!                 >>
//!      /Length ...
//!   >>
//!   stream
//!     q                              % save graphics state
//!     <w> 0 0 <h> 0 0 cm            % scale image to bbox (if image)
//!     /Img Do                        % paint image (if image)
//!     Q                              % restore
//!     BT
//!       /F1 8 Tf  0.1 0.1 0.1 rg    % dark-grey text
//!       <text lines>
//!     ET
//!   endstream
//! endobj
//! ```

use std::path::Path;

use crate::cos::{CosDictionary, CosName, CosObject, CosStream, ObjectId};
use crate::PdfError;

/// Data describing a rasterised PNG/JPEG signature image.
struct RawImage {
    width:  u32,
    height: u32,
    /// Raw RGB bytes (no alpha, 8-bit per channel).
    rgb:    Vec<u8>,
}

/// Load a PNG or JPEG file and decode it to raw RGB bytes.
fn load_image(path: &Path) -> Result<RawImage, PdfError> {
    let img = image::open(path).map_err(|e| PdfError::Parse {
        offset: None,
        context: format!("cannot open signature image {:?}: {e}", path),
    })?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    Ok(RawImage { width: w, height: h, rgb: rgb.into_raw() })
}

/// Result of [`build_appearance`].
pub struct AppearanceObjects {
    /// The Form XObject (appearance stream itself). Insert at `ap_id`.
    pub ap_id:  ObjectId,
    pub ap_obj: CosObject,

    /// An inline PDF image XObject. `None` when no signature image was used.
    pub img_id:  Option<ObjectId>,
    pub img_obj: Option<CosObject>,

    /// A minimal Type 1 / built-in Helvetica font resource. Insert at `font_id`.
    pub font_id:  ObjectId,
    pub font_obj: CosObject,
}

/// Build the complete `/AP /N` Form XObject for a visible signature widget.
///
/// # Arguments
/// * `rect`        — `[x1 y1 x2 y2]` widget rectangle (page user-space).
/// * `image_path`  — optional path to a PNG or JPEG signature image.
/// * `signer_name` — text shown on the signature appearance.
/// * `reason`      — signing reason text.
/// * `date_str`    — signing date string (PDF `M` format).
/// * `ap_id`       — object ID to assign the Form XObject.
/// * `img_id`      — object ID to assign the image XObject (consumed only when `image_path` is `Some`).
/// * `font_id`     — object ID for the Helvetica font resource.
pub fn build_appearance(
    rect:        [f64; 4],
    image_path:  Option<&Path>,
    signer_name: &str,
    reason:      &str,
    date_str:    &str,
    ap_id:       ObjectId,
    img_id:      ObjectId,
    font_id:     ObjectId,
) -> Result<AppearanceObjects, PdfError> {

    let w = (rect[2] - rect[0]).abs();
    let h = (rect[3] - rect[1]).abs();

    // ── Optional image XObject ─────────────────────────────────────────
    let image_result: Option<(CosObject, u32, u32)> = match image_path {
        Some(p) => {
            let raw = load_image(p)?;
            let img_obj = build_image_xobject(&raw);
            Some((img_obj, raw.width, raw.height))
        }
        None => None,
    };

    // Helper — escape parentheses in PDF literal strings
    let esc = |s: &str| s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");

    // ── Text lines to render ───────────────────────────────────────────
    let mut lines: Vec<String> = Vec::new();
    if !signer_name.is_empty() {
        lines.push(format!("Signer: {}", signer_name));
    }
    if !reason.is_empty() {
        lines.push(format!("Reason: {}", esc(reason)));
    }
    if !date_str.is_empty() {
        let readable = if date_str.starts_with("D:") && date_str.len() >= 16 {
            format!("Date: {}-{}-{} {}:{}:{}",
                &date_str[2..6], &date_str[6..8], &date_str[8..10],
                &date_str[10..12], &date_str[12..14], &date_str[14..16])
        } else {
            date_str.to_string()
        };
        lines.push(format!("Date: {}", readable));
    }
    lines.push("Digitally Signed".into());

    // ── Sizing ────────────────────────────────────────────────────────
    // When image present: top 55% = image, bottom 45% = text.
    // When text only: full height.
    let text_area_h = if image_result.is_some() { h * 0.45 } else { h };
    let font_size   = (text_area_h / (lines.len() as f64 + 1.0) * 0.85)
        .max(5.0).min(12.0);
    let line_gap    = font_size * 1.35;

    // ── Build content stream ──────────────────────────────────────────
    let mut content = String::new();

    // Paint image (if any)
    if let Some((_, iw, ih)) = &image_result {
        let img_area_h = h * 0.55;
        let aspect     = *iw as f64 / *ih as f64;
        let (fw, fh)   = if w / aspect <= img_area_h {
            (w, w / aspect)
        } else {
            (img_area_h * aspect, img_area_h)
        };
        let iy = h - fh; // image placed at top
        content.push_str(&format!(
            "q {fw:.4} 0 0 {fh:.4} 0.0000 {iy:.4} cm /Img Do Q\n"
        ));
    }

    // Paint text lines
    // First line: absolute Td from origin
    let text_top_y = text_area_h - font_size * 0.8;
    content.push_str("BT\n");
    content.push_str(&format!("/F1 {font_size:.4} Tf\n"));
    content.push_str("0.1 0.1 0.1 rg\n");

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            content.push_str(&format!("2.0 {text_top_y:.4} Td ({line}) Tj\n"));
        } else {
            content.push_str(&format!("0 {neg_gap:.4} Td ({line}) Tj\n",
                neg_gap = -line_gap));
        }
    }
    content.push_str("ET\n");

    let content_bytes = content.into_bytes();
    let content_len   = content_bytes.len();

    // ── Form XObject dictionary ────────────────────────────────────────
    let mut ap_dict = CosDictionary::new();
    ap_dict.set(CosName::type_name(),    CosObject::Name(CosName::new(b"XObject")));
    ap_dict.set(CosName::new(b"Subtype"), CosObject::Name(CosName::new(b"Form")));
    ap_dict.set(CosName::new(b"BBox"),    CosObject::Array(vec![
        CosObject::Real(0.0), CosObject::Real(0.0),
        CosObject::Real(w),   CosObject::Real(h),
    ]));
    ap_dict.set(CosName::new(b"Length"),  CosObject::Integer(content_len as i64));

    // /Resources << /Font << /F1 R >> [/XObject << /Img R >>] >>
    let mut font_res = CosDictionary::new();
    font_res.set(CosName::new(b"F1"), CosObject::Reference(font_id));

    let mut res_dict = CosDictionary::new();
    res_dict.set(CosName::new(b"Font"), CosObject::Dictionary(font_res));

    if image_result.is_some() {
        let mut xobj_res = CosDictionary::new();
        xobj_res.set(CosName::new(b"Img"), CosObject::Reference(img_id));
        res_dict.set(CosName::new(b"XObject"), CosObject::Dictionary(xobj_res));
    }
    ap_dict.set(CosName::new(b"Resources"), CosObject::Dictionary(res_dict));

    let ap_stream = CosStream::new(ap_dict, content_bytes);
    let ap_obj    = CosObject::Stream(ap_stream);

    // ── Helvetica font resource ────────────────────────────────────────
    let mut font_dict = CosDictionary::new();
    font_dict.set(CosName::type_name(),      CosObject::Name(CosName::new(b"Font")));
    font_dict.set(CosName::new(b"Subtype"),  CosObject::Name(CosName::new(b"Type1")));
    font_dict.set(CosName::new(b"BaseFont"), CosObject::Name(CosName::new(b"Helvetica")));
    font_dict.set(CosName::new(b"Encoding"), CosObject::Name(CosName::new(b"WinAnsiEncoding")));
    let font_obj = CosObject::Dictionary(font_dict);

    Ok(AppearanceObjects {
        ap_id,  ap_obj,
        img_id:  image_result.as_ref().map(|_| img_id),
        img_obj: image_result.map(|(obj, _, _)| obj),
        font_id, font_obj,
    })
}

/// Build an inline PDF Image XObject from raw RGB bytes.
fn build_image_xobject(img: &RawImage) -> CosObject {
    let mut dict = CosDictionary::new();
    dict.set(CosName::type_name(),           CosObject::Name(CosName::new(b"XObject")));
    dict.set(CosName::new(b"Subtype"),        CosObject::Name(CosName::new(b"Image")));
    dict.set(CosName::new(b"Width"),          CosObject::Integer(img.width  as i64));
    dict.set(CosName::new(b"Height"),         CosObject::Integer(img.height as i64));
    dict.set(CosName::new(b"ColorSpace"),     CosObject::Name(CosName::new(b"DeviceRGB")));
    dict.set(CosName::new(b"BitsPerComponent"), CosObject::Integer(8));
    dict.set(CosName::new(b"Length"),         CosObject::Integer(img.rgb.len() as i64));
    CosObject::Stream(CosStream::new(dict, img.rgb.clone()))
}

