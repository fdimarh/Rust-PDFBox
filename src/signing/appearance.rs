//! Signature appearance (visual) stream builder.
//!
//! Implements the **two-layer `/n0` + `/n2` XObject structure** used by
//! Java PDFBox, Adobe Acrobat and `rust_pdf_signing`:
//!
//! ```text
//! AP/N  outer Form XObject
//!   /Matrix [1 0 0 1 0 0]
//!   /FormType 1
//!   /BBox [0 0 W H]
//!   /Resources /XObject << /n0 <n0_id>  /n2 <n2_id> >>
//!   stream:  q /n0 Do Q\nq /n2 Do Q\n
//!
//! /n0  empty background Form XObject ("DSBlank" layer)
//!   /BBox [0 0 W H]
//!   stream:  % DSBlank\n
//!
//! /n2  foreground Form XObject
//!   /BBox [0 0 W H]
//!   /Resources /XObject << /Img <img_id> >>   ← image mode
//!   /Resources /Font    << /F1 <font_id> >>   ← text-only mode
//!   stream:  q Dw 0 0 Dh tx ty cm /Img Do Q   ← image mode
//!            BT /F1 fs Tf … ET                ← text-only mode
//! ```

use std::path::Path;

use crate::cos::{CosDictionary, CosName, CosObject, CosStream, ObjectId};
use crate::PdfError;

// ─── Public return type ───────────────────────────────────────────────────────

/// All PDF objects produced by [`build_appearance`].
///
/// `mod.rs` inserts all of these into the incremental `changed` map.
pub struct AppearanceObjects {
    /// Outer AP/N Form XObject (`q /n0 Do Q  q /n2 Do Q`).
    pub ap_id:   ObjectId,
    pub ap_obj:  CosObject,

    /// `/n0` — empty background sub-Form ("% DSBlank").
    pub n0_id:   ObjectId,
    pub n0_obj:  CosObject,

    /// `/n2` — foreground sub-Form (image or text).
    pub n2_id:   ObjectId,
    pub n2_obj:  CosObject,

    /// Image XObject (present only in image mode).
    pub img_id:  Option<ObjectId>,
    pub img_obj: Option<CosObject>,

    /// Helvetica font resource (present only in text-only mode).
    pub font_id:  ObjectId,
    pub font_obj: CosObject,
}

// ─── Entry point ─────────────────────────────────────────────────────────────

/// Build the complete two-layer `/AP /N` appearance for a visible signature.
///
/// Object IDs are pre-allocated by the caller (`mod.rs`) and passed in so the
/// incremental writer can place each object at a known ID.
///
/// # ID layout (caller assigns sequentially)
/// ```text
/// ap_id   outer Form  (AP/N)
/// n0_id   /n0 empty background sub-Form
/// n2_id   /n2 foreground sub-Form
/// img_id  image XObject        (image mode only; ignored in text-only mode)
/// font_id Helvetica font dict  (text-only mode only; ignored in image mode)
/// ```
#[allow(clippy::too_many_arguments)]
pub fn build_appearance(
    rect:        [f64; 4],
    image_path:  Option<&Path>,
    signer_name: &str,
    reason:      &str,
    date_str:    &str,
    ap_id:       ObjectId,
    n0_id:       ObjectId,
    n2_id:       ObjectId,
    img_id:      ObjectId,
    font_id:     ObjectId,
) -> Result<AppearanceObjects, PdfError> {

    let w = (rect[2] - rect[0]).abs();
    let h = (rect[3] - rect[1]).abs();

    // ── /n0  empty background Form ────────────────────────────────────────
    let n0_obj = make_form_xobj(w, h, b"% DSBlank\n".to_vec(), CosDictionary::new());

    // ── /n2  foreground Form ──────────────────────────────────────────────
    let (n2_bytes, n2_res, image_result) = match image_path {
        Some(p) => {
            let raw = load_image(p)?;
            let (bytes, res, img_cos) = build_image_layer(w, h, &raw, img_id);
            (bytes, res, Some(img_cos))
        }
        None => {
            let (bytes, res) = build_text_layer(w, h, signer_name, reason, date_str, font_id);
            (bytes, res, None)
        }
    };
    let n2_obj = make_form_xobj(w, h, n2_bytes, n2_res);

    // ── outer AP/N Form (delegates to /n0 and /n2) ───────────────────────
    let mut outer_xobj = CosDictionary::new();
    outer_xobj.set(CosName::new(b"n0"), CosObject::Reference(n0_id));
    outer_xobj.set(CosName::new(b"n2"), CosObject::Reference(n2_id));
    let mut outer_res = CosDictionary::new();
    outer_res.set(CosName::new(b"XObject"), CosObject::Dictionary(outer_xobj));

    let outer_bytes = b"q /n0 Do Q\nq /n2 Do Q\n".to_vec();
    let outer_len   = outer_bytes.len() as i64;

    let mut ap_dict = CosDictionary::new();
    ap_dict.set(CosName::type_name(),     CosObject::Name(CosName::new(b"XObject")));
    ap_dict.set(CosName::new(b"Subtype"),  CosObject::Name(CosName::new(b"Form")));
    ap_dict.set(CosName::new(b"FormType"), CosObject::Integer(1));
    ap_dict.set(CosName::new(b"Matrix"),   CosObject::Array(vec![
        CosObject::Integer(1), CosObject::Integer(0),
        CosObject::Integer(0), CosObject::Integer(1),
        CosObject::Integer(0), CosObject::Integer(0),
    ]));
    ap_dict.set(CosName::new(b"BBox"), CosObject::Array(vec![
        CosObject::Real(0.0), CosObject::Real(0.0),
        CosObject::Real(w),   CosObject::Real(h),
    ]));
    ap_dict.set(CosName::new(b"Resources"), CosObject::Dictionary(outer_res));
    ap_dict.set(CosName::new(b"Length"),    CosObject::Integer(outer_len));
    let ap_obj = CosObject::Stream(CosStream::new(ap_dict, outer_bytes));

    // ── Helvetica font resource (text-only mode) ──────────────────────────
    let mut font_dict = CosDictionary::new();
    font_dict.set(CosName::type_name(),      CosObject::Name(CosName::new(b"Font")));
    font_dict.set(CosName::new(b"Subtype"),  CosObject::Name(CosName::new(b"Type1")));
    font_dict.set(CosName::new(b"BaseFont"), CosObject::Name(CosName::new(b"Helvetica")));
    font_dict.set(CosName::new(b"Encoding"), CosObject::Name(CosName::new(b"WinAnsiEncoding")));
    let font_obj = CosObject::Dictionary(font_dict);

    Ok(AppearanceObjects {
        ap_id,  ap_obj,
        n0_id,  n0_obj,
        n2_id,  n2_obj,
        img_id:  image_result.as_ref().map(|_| img_id),
        img_obj: image_result,
        font_id, font_obj,
    })
}

// ─── Form XObject helper ──────────────────────────────────────────────────────

fn make_form_xobj(w: f64, h: f64, data: Vec<u8>, resources: CosDictionary) -> CosObject {
    let len = data.len() as i64;
    let mut d = CosDictionary::new();
    d.set(CosName::type_name(),     CosObject::Name(CosName::new(b"XObject")));
    d.set(CosName::new(b"Subtype"), CosObject::Name(CosName::new(b"Form")));
    d.set(CosName::new(b"FormType"),CosObject::Integer(1));
    d.set(CosName::new(b"BBox"),    CosObject::Array(vec![
        CosObject::Real(0.0), CosObject::Real(0.0),
        CosObject::Real(w),   CosObject::Real(h),
    ]));
    d.set(CosName::new(b"Resources"), CosObject::Dictionary(resources));
    d.set(CosName::new(b"Length"),    CosObject::Integer(len));
    CosObject::Stream(CosStream::new(d, data))
}

// ─── Image layer (/n2 content) ────────────────────────────────────────────────

struct RawImage {
    width:  u32,
    height: u32,
    rgb:    Vec<u8>,
}

fn load_image(path: &Path) -> Result<RawImage, PdfError> {
    let img = image::open(path).map_err(|e| PdfError::Parse {
        offset: None,
        context: format!("cannot open signature image {:?}: {e}", path),
    })?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    Ok(RawImage { width: w, height: h, rgb: rgb.into_raw() })
}

/// Returns `(n2_stream_bytes, n2_resources, image_xobject)`.
fn build_image_layer(
    bbox_w: f64,
    bbox_h: f64,
    img:    &RawImage,
    img_id: ObjectId,
) -> (Vec<u8>, CosDictionary, CosObject) {
    // Fit image inside bbox preserving aspect ratio, centre
    let aspect = img.width as f64 / img.height as f64;
    let (dw, dh) = if bbox_w / aspect <= bbox_h {
        (bbox_w, bbox_w / aspect)
    } else {
        (bbox_h * aspect, bbox_h)
    };
    let tx = (bbox_w - dw) / 2.0;
    let ty = (bbox_h - dh) / 2.0;

    // Image XObject
    let img_len = img.rgb.len() as i64;
    let mut id = CosDictionary::new();
    id.set(CosName::type_name(),             CosObject::Name(CosName::new(b"XObject")));
    id.set(CosName::new(b"Subtype"),          CosObject::Name(CosName::new(b"Image")));
    id.set(CosName::new(b"Width"),            CosObject::Integer(img.width  as i64));
    id.set(CosName::new(b"Height"),           CosObject::Integer(img.height as i64));
    id.set(CosName::new(b"ColorSpace"),       CosObject::Name(CosName::new(b"DeviceRGB")));
    id.set(CosName::new(b"BitsPerComponent"), CosObject::Integer(8));
    id.set(CosName::new(b"Length"),           CosObject::Integer(img_len));
    let img_obj = CosObject::Stream(CosStream::new(id, img.rgb.clone()));

    // /n2 resources: /XObject << /Img <img_id> >>
    let mut xs = CosDictionary::new();
    xs.set(CosName::new(b"Img"), CosObject::Reference(img_id));
    let mut res = CosDictionary::new();
    res.set(CosName::new(b"XObject"), CosObject::Dictionary(xs));

    // /n2 stream: place image
    let content = format!("q {dw:.4} 0 0 {dh:.4} {tx:.4} {ty:.4} cm /Img Do Q\n");
    (content.into_bytes(), res, img_obj)
}

// ─── Text layer (/n2 content) ─────────────────────────────────────────────────

fn build_text_layer(
    _w:          f64,
    h:           f64,
    signer_name: &str,
    reason:      &str,
    date_str:    &str,
    font_id:     ObjectId,
) -> (Vec<u8>, CosDictionary) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");

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
        lines.push(readable);
    }
    lines.push("Digitally Signed".into());

    let font_size = (h / (lines.len() as f64 + 1.0) * 0.85).max(5.0).min(12.0);
    let line_gap  = font_size * 1.35;
    let text_top  = h - font_size * 0.8;

    let mut content = String::new();
    content.push_str("BT\n");
    content.push_str(&format!("/F1 {font_size:.4} Tf\n"));
    content.push_str("0.1 0.1 0.1 rg\n");
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            content.push_str(&format!("2.0 {text_top:.4} Td ({line}) Tj\n"));
        } else {
            content.push_str(&format!("0 {:.4} Td ({line}) Tj\n", -line_gap));
        }
    }
    content.push_str("ET\n");

    // /n2 resources: /Font << /F1 <font_id> >>
    let mut fr = CosDictionary::new();
    fr.set(CosName::new(b"F1"), CosObject::Reference(font_id));
    let mut res = CosDictionary::new();
    res.set(CosName::new(b"Font"), CosObject::Dictionary(fr));

    (content.into_bytes(), res)
}

