//!
//! Appearance generation for interactive form fields.
//!
//! Generates appearance streams (`/AP` / `/N`) for widget annotations so that
//! filled form values render correctly in PDF viewers without relying on
//! `NeedAppearances`.
//!
//! Maps to Java PDFBox's `AppearanceGenerator`.

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, PdfResult};

/// Default font size used in appearance streams when not specified in `/DA`.
const DEFAULT_FONT_SIZE: f64 = 10.0;
/// Default font name used when not specified in `/DA`.
const DEFAULT_FONT: &str = "Helv";
/// Default padding inside widget annotations in points.
const PADDING: f64 = 2.0;

/// Generates appearance streams for all fields in a document's AcroForm.
///
/// This should be called after setting field values, before saving.
pub fn generate_all_appearances(doc: &mut Document, fields: &[ObjectId]) -> PdfResult<()> {
    for field_id in fields {
        generate_field_appearance(doc, *field_id)?;
    }
    Ok(())
}

/// Generates the appearance stream for a single field, looking up its dictionary
/// from the document's object store.
pub fn generate_field_appearance(doc: &mut Document, field_id: ObjectId) -> PdfResult<()> {
    // Clone field dict early to avoid holding a reference into doc.objects
    let field_dict = match doc.get_object_ref(field_id).and_then(|o| o.as_dictionary()) {
        Some(d) => d.clone(),
        None => return Ok(()),
    };

    let ft = field_dict
        .get(&CosName::new(b"FT".to_vec()))
        .and_then(|v| v.as_name())
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let flags = field_dict.get_int(&CosName::new(b"Ff".to_vec())).unwrap_or(0);

    match ft.as_str() {
        "Tx" => generate_text_field_appearance(doc, field_id, &field_dict),
        "Btn" => {
            if (flags & 0x10000) != 0 {
                generate_push_button_appearance(doc, field_id, &field_dict)
            } else if (flags & 0x8000) != 0 {
                generate_radio_button_appearance(doc, field_id, &field_dict)
            } else {
                generate_checkbox_appearance(doc, field_id, &field_dict)
            }
        }
        "Ch" => {
            if (flags & 0x20000) != 0 {
                generate_combo_box_appearance(doc, field_id, &field_dict)
            } else {
                generate_list_box_appearance(doc, field_id, &field_dict)
            }
        }
        "Sig" => generate_signature_field_appearance(doc, field_id, &field_dict),
        _ => Ok(()),
    }
}

/// Parses the Default Appearance string (`/DA`) to extract font name, size, and color.
fn parse_da(da: &str) -> (String, f64, f64, f64, f64) {
    let mut font_name = DEFAULT_FONT.to_string();
    let mut font_size = DEFAULT_FONT_SIZE;
    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;

    let tokens: Vec<&str> = da.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        if *token == "Tf" && i >= 2 {
            if let Some(name) = tokens.get(i.saturating_sub(2)) {
                font_name = name.trim_start_matches('/').to_string();
            }
            if let Some(size_str) = tokens.get(i.saturating_sub(1)) {
                if let Ok(size) = size_str.parse::<f64>() {
                    font_size = size;
                }
            }
        }
    }
    for (i, token) in tokens.iter().enumerate() {
        if *token == "g" && i >= 1 {
            if let Some(gray_str) = tokens.get(i.saturating_sub(1)) {
                if let Ok(gray) = gray_str.parse::<f64>() {
                    r = gray;
                    g = gray;
                    b = gray;
                }
            }
        }
        if *token == "rg" && i >= 3 {
            if let (Some(rc), Some(gc), Some(bc)) = (
                tokens.get(i.saturating_sub(3)).and_then(|s| s.parse::<f64>().ok()),
                tokens.get(i.saturating_sub(2)).and_then(|s| s.parse::<f64>().ok()),
                tokens.get(i.saturating_sub(1)).and_then(|s| s.parse::<f64>().ok()),
            ) {
                r = rc;
                g = gc;
                b = bc;
            }
        }
    }
    (font_name, font_size, r, g, b)
}

/// Returns the default appearance string for a field.
fn get_da(field_dict: &CosDictionary) -> String {
    if let Some(da) = field_dict.get(&CosName::new(b"DA".to_vec())) {
        if let Some(s) = da.as_string() {
            if let Ok(s) = String::from_utf8(s.to_vec()) {
                return s;
            }
        }
    }
    format!("/{} {} Tf 0 g", DEFAULT_FONT, DEFAULT_FONT_SIZE)
}

/// Returns the widget rectangle (`/Rect`) for a field.
fn get_rect(field_dict: &CosDictionary) -> (f64, f64, f64, f64) {
    if let Some(rect_obj) = field_dict.get(&CosName::new(b"Rect".to_vec())) {
        if let Some(arr) = rect_obj.as_array() {
            if arr.len() >= 4 {
                let llx = arr[0].as_real().unwrap_or(0.0);
                let lly = arr[1].as_real().unwrap_or(0.0);
                let urx = arr[2].as_real().unwrap_or(0.0);
                let ury = arr[3].as_real().unwrap_or(0.0);
                return (llx, lly, urx, ury);
            }
        }
    }
    (0.0, 0.0, 100.0, 20.0)
}

fn get_effective_value(field_dict: &CosDictionary) -> Option<CosObject> {
    field_dict.get(&CosName::new(b"V".to_vec())).cloned()
}

fn get_field_value_as_string(field_dict: &CosDictionary) -> String {
    match get_effective_value(field_dict) {
        Some(CosObject::String(bytes)) => String::from_utf8_lossy(&bytes).to_string(),
        Some(CosObject::Name(name)) => name.to_string(),
        Some(other) => format!("{}", other),
        None => String::new(),
    }
}

fn get_field_name(field_dict: &CosDictionary) -> String {
    field_dict
        .get(&CosName::new(b"T".to_vec()))
        .and_then(|v| v.as_string())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_default()
}

/// Builds a Form XObject content stream for an appearance and registers it
/// on the widget annotation as `/AP` `/N`.
fn set_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    content_bytes: Vec<u8>,
    bbox: (f64, f64, f64, f64),
) -> PdfResult<()> {
    let (_llx, _lly, urx, ury) = bbox;
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let mut form_dict = CosDictionary::new();
    form_dict.insert(
        CosName::type_name(),
        CosObject::Name(CosName::new(b"XObject".to_vec())),
    );
    form_dict.insert(
        CosName::subtype(),
        CosObject::Name(CosName::new(b"Form".to_vec())),
    );
    form_dict.insert(
        CosName::new(b"BBox".to_vec()),
        CosObject::Array(vec![
            CosObject::Real(0.0),
            CosObject::Real(0.0),
            CosObject::Real(width),
            CosObject::Real(height),
        ]),
    );
    form_dict.insert(
        CosName::new(b"Length".to_vec()),
        CosObject::Integer(content_bytes.len() as i64),
    );

    let form_stream = crate::cos::CosStream::new(form_dict, content_bytes);
    let form_id = doc.allocate_object_id();
    doc.insert_object(form_id, CosObject::Stream(form_stream));
    doc.xref.insert_if_absent(
        form_id,
        crate::parser::xref::XRefEntry::InUse { offset: 0, generation: 0 },
    );

    let has_named_v = doc
        .get_object_ref(field_id)
        .and_then(|o| o.as_dictionary())
        .and_then(|d| d.get(&CosName::new(b"V".to_vec())))
        .and_then(|v| v.as_name())
        .is_some();

    // Pre-allocate Off appearance if needed (avoids borrowing doc inside the closure)
    let off_id = if has_named_v {
        let off_dict = CosDictionary::new();
        let oid = doc.allocate_object_id();
        doc.insert_object(oid, CosObject::Stream(crate::cos::CosStream::new(off_dict, vec![])));
        doc.xref.insert_if_absent(
            oid,
            crate::parser::xref::XRefEntry::InUse { offset: 0, generation: 0 },
        );
        Some(oid)
    } else {
        None
    };

    doc.mutate_object(field_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            let mut ap_dict = CosDictionary::new();

            if let Some(off) = off_id {
                let val_name = dict
                    .get(&CosName::new(b"V".to_vec()))
                    .and_then(|v| v.as_name())
                    .map(|n| n.as_bytes().to_vec())
                    .unwrap_or_else(|| b"Yes".to_vec());

                let mut n_sub_dict = CosDictionary::new();
                n_sub_dict.insert(CosName::new(val_name), CosObject::Reference(form_id));
                n_sub_dict.insert(CosName::new(b"Off".to_vec()), CosObject::Reference(off));
                ap_dict.insert(CosName::new(b"N".to_vec()), CosObject::Dictionary(n_sub_dict));
            } else {
                ap_dict.insert(CosName::new(b"N".to_vec()), CosObject::Reference(form_id));
            }

            dict.insert(CosName::new(b"AP".to_vec()), CosObject::Dictionary(ap_dict));
        }
    });

    Ok(())
}

// ── Text Field ──────────────────────────────────────────────────────────────

fn generate_text_field_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    field_dict: &CosDictionary,
) -> PdfResult<()> {
    let da_str = get_da(field_dict);
    let (font_name, font_size, r, g, b) = parse_da(&da_str);
    let (_llx, _lly, urx, ury) = get_rect(field_dict);
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let text_value = get_field_value_as_string(field_dict);

    let mut content = Vec::new();
    content.extend_from_slice(format!("{} {} {} rg\n", r, g, b).as_bytes());
    content.extend_from_slice(format!("/{} {} Tf\n", font_name, font_size).as_bytes());
    content.extend_from_slice(b"BT\n");

    if !text_value.is_empty() {
        let text_x = PADDING;
        let text_y = height - PADDING - font_size;
        content.extend_from_slice(format!("{} {} Td\n", text_x, text_y).as_bytes());
        content.push(b'(');
        for byte in text_value.as_bytes() {
            match byte {
                b'(' => content.extend_from_slice(b"\\("),
                b')' => content.extend_from_slice(b"\\)"),
                b'\\' => content.extend_from_slice(b"\\\\"),
                _ => content.push(*byte),
            }
        }
        content.extend_from_slice(b") Tj\n");
    }
    content.extend_from_slice(b"ET\n");

    set_appearance(doc, field_id, content, (_llx, _lly, urx, ury))
}

// ── Check Box ───────────────────────────────────────────────────────────────

fn generate_checkbox_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    field_dict: &CosDictionary,
) -> PdfResult<()> {
    let (_llx, _lly, urx, ury) = get_rect(field_dict);
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let value_name = get_effective_value(field_dict)
        .and_then(|v| v.as_name().cloned())
        .map(|n| n.as_bytes().to_vec());
    let is_checked = value_name.as_ref().map(|v| v != b"Off").unwrap_or(false);

    let mut content = Vec::new();
    content.extend_from_slice(format!("0 0 {} {} re\n", width, height).as_bytes());
    content.extend_from_slice(b"0 0 0 RG\n");
    content.extend_from_slice(b"S\n");

    if is_checked {
        let margin = width * 0.2;
        let x1 = margin;
        let y1 = height * 0.3;
        let x2 = width * 0.45;
        let y2 = height * 0.7;
        let x3 = width - margin;
        let y3 = height * 0.3;
        content.extend_from_slice(format!("{} {} m\n", x1, y1).as_bytes());
        content.extend_from_slice(format!("{} {} l\n", x2, y2).as_bytes());
        content.extend_from_slice(format!("{} {} l\n", x3, y3).as_bytes());
        content.extend_from_slice(b"0 0 0 RG\n");
        content.extend_from_slice(format!("{} w\n", (width * 0.08).max(1.5)).as_bytes());
        content.extend_from_slice(b"S\n");
    }

    set_appearance(doc, field_id, content, (_llx, _lly, urx, ury))
}

// ── Radio Button ────────────────────────────────────────────────────────────

fn generate_radio_button_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    field_dict: &CosDictionary,
) -> PdfResult<()> {
    let (_llx, _lly, urx, ury) = get_rect(field_dict);
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let value_name = get_effective_value(field_dict)
        .and_then(|v| v.as_name().cloned())
        .map(|n| n.as_bytes().to_vec());
    let is_selected = value_name.as_ref().map(|v| v != b"Off").unwrap_or(false);

    let mut content = Vec::new();
    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = (width.min(height)) / 2.0 - 1.0;
    let k: f64 = 0.5522847498;

    content.extend_from_slice(format!("{} {} m\n", cx + radius, cy).as_bytes());
    content.extend_from_slice(
        format!("{} {} {} {} {} {} c\n", cx + radius, cy + radius * k, cx + radius * k, cy + radius, cx, cy + radius).as_bytes(),
    );
    content.extend_from_slice(
        format!("{} {} {} {} {} {} c\n", cx - radius * k, cy + radius, cx - radius, cy + radius * k, cx - radius, cy).as_bytes(),
    );
    content.extend_from_slice(
        format!("{} {} {} {} {} {} c\n", cx - radius, cy - radius * k, cx - radius * k, cy - radius, cx, cy - radius).as_bytes(),
    );
    content.extend_from_slice(
        format!("{} {} {} {} {} {} c\n", cx + radius * k, cy - radius, cx + radius, cy - radius * k, cx + radius, cy).as_bytes(),
    );
    content.extend_from_slice(b"0 0 0 RG\n");
    content.extend_from_slice(b"S\n");

    if is_selected {
        let inner_r = radius * 0.4;
        content.extend_from_slice(format!("{} {} m\n", cx + inner_r, cy).as_bytes());
        content.extend_from_slice(
            format!("{} {} {} {} {} {} c\n", cx + inner_r, cy + inner_r * k, cx + inner_r * k, cy + inner_r, cx, cy + inner_r).as_bytes(),
        );
        content.extend_from_slice(
            format!("{} {} {} {} {} {} c\n", cx - inner_r * k, cy + inner_r, cx - inner_r, cy + inner_r * k, cx - inner_r, cy).as_bytes(),
        );
        content.extend_from_slice(
            format!("{} {} {} {} {} {} c\n", cx - inner_r, cy - inner_r * k, cx - inner_r * k, cy - inner_r, cx, cy - inner_r).as_bytes(),
        );
        content.extend_from_slice(
            format!("{} {} {} {} {} {} c\n", cx + inner_r * k, cy - inner_r, cx + inner_r, cy - inner_r * k, cx + inner_r, cy).as_bytes(),
        );
        content.extend_from_slice(b"0 0 0 rg\n");
        content.extend_from_slice(b"f\n");
    }

    set_appearance(doc, field_id, content, (_llx, _lly, urx, ury))
}

// ── Combo Box ───────────────────────────────────────────────────────────────

fn generate_combo_box_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    field_dict: &CosDictionary,
) -> PdfResult<()> {
    let da_str = get_da(field_dict);
    let (font_name, font_size, r, g, b) = parse_da(&da_str);
    let (_llx, _lly, urx, ury) = get_rect(field_dict);
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let text_value = get_field_value_as_string(field_dict);

    let mut content = Vec::new();
    content.extend_from_slice(format!("0 0 {} {} re\n", width, height).as_bytes());
    content.extend_from_slice(b"0.6 0.6 0.6 RG\n");
    content.extend_from_slice(b"S\n");

    let arrow_size = height * 0.6;
    let arrow_x = width - height * 0.8;
    content.extend_from_slice(format!("{} {} m\n", arrow_x, height * 0.3).as_bytes());
    content.extend_from_slice(format!("{} {} l\n", arrow_x + arrow_size * 0.7, height * 0.3).as_bytes());
    content.extend_from_slice(format!("{} {} l\n", arrow_x + arrow_size * 0.35, height * 0.7).as_bytes());
    content.extend_from_slice(b"h 0.4 0.4 0.4 rg f\n");

    if !text_value.is_empty() {
        content.extend_from_slice(format!("{} {} {} rg\n", r, g, b).as_bytes());
        content.extend_from_slice(format!("/{} {} Tf\n", font_name, font_size).as_bytes());
        content.extend_from_slice(b"BT\n");
        let text_x = PADDING;
        let text_y = height - PADDING - font_size;
        content.extend_from_slice(format!("{} {} Td\n", text_x, text_y).as_bytes());
        content.push(b'(');
        for byte in text_value.as_bytes() {
            match byte {
                b'(' => content.extend_from_slice(b"\\("),
                b')' => content.extend_from_slice(b"\\)"),
                b'\\' => content.extend_from_slice(b"\\\\"),
                _ => content.push(*byte),
            }
        }
        content.extend_from_slice(b") Tj\n");
        content.extend_from_slice(b"ET\n");
    }

    set_appearance(doc, field_id, content, (_llx, _lly, urx, ury))
}

// ── List Box ────────────────────────────────────────────────────────────────

fn generate_list_box_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    field_dict: &CosDictionary,
) -> PdfResult<()> {
    let da_str = get_da(field_dict);
    let (font_name, font_size, r, g, b) = parse_da(&da_str);
    let (_llx, _lly, urx, ury) = get_rect(field_dict);
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let text_value = get_field_value_as_string(field_dict);

    let mut content = Vec::new();
    content.extend_from_slice(format!("0 0 {} {} re\n", width, height).as_bytes());
    content.extend_from_slice(b"0.6 0.6 0.6 RG\n");
    content.extend_from_slice(b"S\n");

    if !text_value.is_empty() {
        content.extend_from_slice(format!("{} {} {} rg\n", r, g, b).as_bytes());
        content.extend_from_slice(format!("/{} {} Tf\n", font_name, font_size).as_bytes());
        content.extend_from_slice(b"BT\n");
        let text_x = PADDING;
        let text_y = height - PADDING - font_size;
        content.extend_from_slice(format!("{} {} Td\n", text_x, text_y).as_bytes());
        content.push(b'(');
        for byte in text_value.as_bytes() {
            match byte {
                b'(' => content.extend_from_slice(b"\\("),
                b')' => content.extend_from_slice(b"\\)"),
                b'\\' => content.extend_from_slice(b"\\\\"),
                _ => content.push(*byte),
            }
        }
        content.extend_from_slice(b") Tj\n");
        content.extend_from_slice(b"ET\n");
    }

    set_appearance(doc, field_id, content, (_llx, _lly, urx, ury))
}

// ── Push Button ─────────────────────────────────────────────────────────────

fn generate_push_button_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    field_dict: &CosDictionary,
) -> PdfResult<()> {
    let (_llx, _lly, urx, ury) = get_rect(field_dict);
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let label = get_field_name(field_dict);

    let mut content = Vec::new();
    content.extend_from_slice(format!("0 0 {} {} re\n", width, height).as_bytes());
    content.extend_from_slice(b"0.9 0.9 0.9 rg\n");
    content.extend_from_slice(b"f\n");
    content.extend_from_slice(b"0 0 0 RG\n");
    content.extend_from_slice(format!("{} w\n", 0.5).as_bytes());
    content.extend_from_slice(b"S\n");

    if !label.is_empty() {
        content.extend_from_slice(b"0 0 0 rg\n");
        let font_size = 10.0;
        content.extend_from_slice(format!("/Helv {} Tf\n", font_size).as_bytes());
        content.extend_from_slice(b"BT\n");
        let text_x = width / 2.0;
        let text_y = (height - font_size) / 2.0;
        content.extend_from_slice(format!("{} {} Td\n", text_x, text_y).as_bytes());
        content.push(b'(');
        for byte in label.as_bytes() {
            match byte {
                b'(' => content.extend_from_slice(b"\\("),
                b')' => content.extend_from_slice(b"\\)"),
                b'\\' => content.extend_from_slice(b"\\\\"),
                _ => content.push(*byte),
            }
        }
        content.extend_from_slice(b") Tj\n");
        content.extend_from_slice(b"ET\n");
    }

    set_appearance(doc, field_id, content, (_llx, _lly, urx, ury))
}

// ── Signature Field ─────────────────────────────────────────────────────────

fn generate_signature_field_appearance(
    doc: &mut Document,
    field_id: ObjectId,
    field_dict: &CosDictionary,
) -> PdfResult<()> {
    let (_llx, _lly, urx, ury) = get_rect(field_dict);
    let width = urx - _llx;
    let height = ury - _lly;

    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }

    let mut content = Vec::new();
    content.extend_from_slice(format!("0 0 {} {} re\n", width, height).as_bytes());
    content.extend_from_slice(b"0.95 0.95 0.95 rg\n");
    content.extend_from_slice(b"f\n");
    content.extend_from_slice(b"0.5 0.5 0.5 RG\n");
    content.extend_from_slice(b"S\n");

    let line_y = height * 0.4;
    content.extend_from_slice(format!("{} {} m\n", PADDING, line_y).as_bytes());
    content.extend_from_slice(format!("{} {} l\n", width - PADDING, line_y).as_bytes());
    content.extend_from_slice(b"0.5 0.5 0.5 RG\n");
    content.extend_from_slice(b"S\n");

    content.extend_from_slice(b"0.5 0.5 0.5 rg\n");
    content.extend_from_slice(b"/Helv 8 Tf\n");
    content.extend_from_slice(b"BT\n");
    content.extend_from_slice(format!("{} {} Td\n", PADDING, line_y + 2.0).as_bytes());
    content.extend_from_slice(b"(Signature) Tj\n");
    content.extend_from_slice(b"ET\n");

    set_appearance(doc, field_id, content, (_llx, _lly, urx, ury))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_da_simple() {
        let (font, size, r, ..) = parse_da("/Helv 10 Tf 0 g");
        assert_eq!(font, "Helv");
        assert!((size - 10.0).abs() < f64::EPSILON);
        assert!((r - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_da_rgb() {
        let (font, size, r, g, b) = parse_da("/F1 12 Tf 1 0 0 rg");
        assert_eq!(font, "F1");
        assert!((size - 12.0).abs() < f64::EPSILON);
        assert!((r - 1.0).abs() < f64::EPSILON);
        assert!((g - 0.0).abs() < f64::EPSILON);
        assert!((b - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_da_no_color_defaults_black() {
        let (font, size, ..) = parse_da("/ZapfDingbats 14 Tf");
        assert_eq!(font, "ZapfDingbats");
        assert!((size - 14.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_rect_default() {
        let dict = CosDictionary::new();
        let (_llx, _lly, _urx, _ury) = get_rect(&dict);
    }

    #[test]
    fn test_get_da_fallback() {
        let dict = CosDictionary::new();
        let da = get_da(&dict);
        assert!(da.contains("/Helv"));
        assert!(da.contains("10 Tf"));
    }

    #[test]
    fn test_get_field_name_empty() {
        let dict = CosDictionary::new();
        let name = get_field_name(&dict);
        assert!(name.is_empty());
    }

    #[test]
    fn test_get_field_value_as_string_empty() {
        let dict = CosDictionary::new();
        let val = get_field_value_as_string(&dict);
        assert!(val.is_empty());
    }
}
