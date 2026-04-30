//! Comprehensive integration tests for ContentStreamWriter (P16).
//!
//! Covers all operator categories added in P16: path (v, y), paint (b, b*),
//! color (cs, CS, sc, SC), state (d, ri, i, gs), Form XObject.

use rust_pdfbox::content::writer::ContentStreamWriter;
use rust_pdfbox::cos::{CosName, CosObject};
use rust_pdfbox::pdmodel::{DocumentBuilder, PageSize};
use rust_pdfbox::PdfResult;

// =========================================================================
// Path operator tests (P16 additions)
// =========================================================================

#[test]
fn test_path_v_operator() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.move_to_point(0.0, 0.0)?;
    writer.curve_to_final(100.0, 0.0, 100.0, 100.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    assert!(stream_data.windows(2).any(|w| w == b" v"), "expected v operator");
    Ok(())
}

#[test]
fn test_path_y_operator() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.move_to_point(0.0, 0.0)?;
    writer.curve_to_initial(50.0, 0.0, 100.0, 100.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    assert!(stream_data.windows(2).any(|w| w == b" y"), "expected y operator");
    Ok(())
}

#[test]
fn test_all_path_operators_present() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.move_to_point(50.0, 50.0)?;
    writer.line_to(100.0, 50.0)?;
    writer.line_to(100.0, 100.0)?;
    writer.close_path()?;
    writer.move_to_point(200.0, 200.0)?;
    writer.curve_to(210.0, 210.0, 220.0, 220.0, 230.0, 230.0)?;
    writer.close_path()?;
    writer.curve_to_final(250.0, 250.0, 260.0, 260.0)?;
    writer.close_path()?;
    writer.curve_to_initial(300.0, 300.0, 310.0, 310.0)?;
    writer.close_path()?;
    writer.add_rect(72.0, 600.0, 100.0, 50.0)?;
    writer.close_path()?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains(" m\n") || s.contains("\nm\n"), "expected m");
    assert!(s.contains(" l\n") || s.contains("\nl\n"), "expected l");
    assert!(s.contains(" c\n") || s.contains("\nc\n"), "expected c");
    assert!(s.contains(" v\n") || s.contains("\nv\n"), "expected v");
    assert!(s.contains(" y\n") || s.contains("\ny\n"), "expected y");
    assert!(s.contains(" h\n") || s.contains("\nh\n"), "expected h");
    assert!(s.contains(" re\n") || s.contains("\nre\n"), "expected re");
    Ok(())
}

// =========================================================================
// Paint operator tests (P16 additions: b, b*)
// =========================================================================

#[test]
fn test_close_fill_and_stroke_operators() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.add_rect(10.0, 10.0, 50.0, 50.0)?;
    writer.close_fill_and_stroke()?;
    writer.add_rect(70.0, 10.0, 50.0, 50.0)?;
    writer.close_fill_and_stroke_even_odd()?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("\nb\n"), "expected b operator");
    assert!(s.contains("\nb*\n"), "expected b* operator");
    Ok(())
}

#[test]
fn test_all_paint_operators() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.add_rect(10.0, 10.0, 50.0, 50.0)?;
    writer.stroke()?;
    writer.add_rect(70.0, 10.0, 50.0, 50.0)?;
    writer.fill()?;
    writer.add_rect(130.0, 10.0, 50.0, 50.0)?;
    writer.fill_even_odd()?;
    writer.add_rect(190.0, 10.0, 50.0, 50.0)?;
    writer.close_fill_and_stroke()?;
    writer.add_rect(250.0, 10.0, 50.0, 50.0)?;
    writer.close_fill_and_stroke_even_odd()?;
    writer.add_rect(310.0, 10.0, 50.0, 50.0)?;
    writer.fill_and_stroke()?;
    writer.add_rect(370.0, 10.0, 50.0, 50.0)?;
    writer.fill_and_stroke_even_odd()?;
    writer.add_rect(430.0, 10.0, 50.0, 50.0)?;
    writer.end_path()?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("\nS\n"), "expected S");
    assert!(s.contains("\nf\n"), "expected f");
    assert!(s.contains("\nf*\n"), "expected f*");
    assert!(s.contains("\nb\n"), "expected b");
    assert!(s.contains("\nb*\n"), "expected b*");
    assert!(s.contains("\nB\n"), "expected B");
    assert!(s.contains("\nB*\n"), "expected B*");
    assert!(s.contains("\nn\n"), "expected n");
    Ok(())
}

// =========================================================================
// State operator tests (P16 additions: d, ri, i, gs)
// =========================================================================

#[test]
fn test_line_dash_operator() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.add_rect(10.0, 10.0, 50.0, 50.0)?;
    writer.set_line_dash(&[2, 3, 4], 1)?;
    writer.stroke()?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("[2 3 4] 1 d"), "expected dash pattern");
    Ok(())
}

#[test]
fn test_rendering_intent_operator() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.set_rendering_intent("Perceptual")?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("/Perceptual ri"), "expected ri operator");
    Ok(())
}

#[test]
fn test_flatness_operator() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.set_flatness(1.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("1 i"), "expected i operator");
    Ok(())
}

#[test]
fn test_graphics_state_operator() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.set_graphics_state("MyState")?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("/MyState gs"), "expected gs operator");
    Ok(())
}

#[test]
fn test_all_state_operators() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.save_state()?;
    writer.set_line_width(2.5)?;
    writer.set_line_cap(1)?;
    writer.set_line_join(2)?;
    writer.set_miter_limit(5.0)?;
    writer.set_line_dash(&[3, 5], 0)?;
    writer.set_rendering_intent("RelativeColorimetric")?;
    writer.set_flatness(1.0)?;
    writer.transform(1.0, 0.0, 0.0, 1.0, 10.0, 20.0)?;
    writer.restore_state()?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("q\n"), "expected q");
    assert!(s.contains("Q\n"), "expected Q");
    assert!(s.contains(" w\n") || s.contains("\nw\n"), "expected w");
    assert!(s.contains(" J\n") || s.contains("\nJ\n"), "expected J");
    assert!(s.contains(" j\n") || s.contains("\nj\n"), "expected j");
    assert!(s.contains(" M\n") || s.contains("\nM\n"), "expected M");
    assert!(s.contains(" d\n") || s.contains("\nd\n"), "expected d");
    assert!(s.contains(" ri\n") || s.contains("\nri\n"), "expected ri");
    assert!(s.contains(" i\n") || s.contains("\ni\n"), "expected i");
    assert!(s.contains(" cm\n") || s.contains("\ncm\n"), "expected cm");
    Ok(())
}

// =========================================================================
// Color operator tests (P16 additions: cs, CS, sc, SC)
// =========================================================================

#[test]
fn test_color_space_operators() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.set_fill_color_space("DeviceRGB")?;
    writer.set_stroke_color_space("DeviceCMYK")?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("/DeviceRGB cs"), "expected cs");
    assert!(s.contains("/DeviceCMYK CS"), "expected CS");
    Ok(())
}

#[test]
fn test_custom_color_operators() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.set_fill_color_custom(&[0.5, 0.5, 0.5])?;
    writer.set_stroke_color_custom(&[0.2, 0.3])?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains(" sc\n"), "expected sc");
    assert!(s.contains(" SC\n"), "expected SC");
    Ok(())
}

#[test]
fn test_all_color_operators() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.set_stroke_color(1.0, 0.0, 0.0)?;
    writer.set_fill_color(0.0, 1.0, 0.0)?;
    writer.set_stroke_gray(0.3)?;
    writer.set_fill_gray(0.7)?;
    writer.set_stroke_color_cmyk(0.1, 0.2, 0.3, 0.4)?;
    writer.set_fill_color_cmyk(0.4, 0.3, 0.2, 0.1)?;
    writer.set_fill_color_space("DeviceRGB")?;
    writer.set_stroke_color_space("DeviceCMYK")?;
    writer.set_fill_color_custom(&[0.5, 0.5, 0.5])?;
    writer.set_stroke_color_custom(&[0.2, 0.3])?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("RG\n") || s.contains("\nRG\n"), "expected RG");
    assert!(s.contains("rg\n") || s.contains("\nrg\n"), "expected rg");
    assert!(s.contains(" G\n") || s.contains("\nG\n"), "expected G");
    assert!(s.contains(" g\n") || s.contains("\ng\n"), "expected g");
    assert!(s.contains("K\n") || s.contains("\nK\n"), "expected K");
    assert!(s.contains("k\n") || s.contains("\nk\n"), "expected k");
    assert!(s.contains("cs\n") || s.contains("\ncs\n"), "expected cs");
    assert!(s.contains("CS\n") || s.contains("\nCS\n"), "expected CS");
    assert!(s.contains("sc\n") || s.contains("\nsc\n"), "expected sc");
    assert!(s.contains("SC\n") || s.contains("\nSC\n"), "expected SC");
    Ok(())
}

// =========================================================================
// Clipping operator tests
// =========================================================================

#[test]
fn test_clipping_operators() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.add_rect(72.0, 700.0, 100.0, 50.0)?;
    writer.clip()?;
    writer.end_path()?;
    writer.add_rect(72.0, 600.0, 100.0, 50.0)?;
    writer.clip_even_odd()?;
    writer.end_path()?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let stream_data = resolve_content_stream(&doc, &page)?;
    let s = String::from_utf8_lossy(&stream_data);
    assert!(s.contains("W\n") || s.contains("\nW\n"), "expected W");
    assert!(s.contains("W*\n") || s.contains("\nW*\n"), "expected W*");
    Ok(())
}

// =========================================================================
// Form XObject tests
// =========================================================================

#[test]
fn test_register_form_xobject() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let form_content = b"BT /Helvetica 12 Tf 72 720 Td (Form) Tj ET".to_vec();
    let name = writer.register_form_xobject(Some("Fm"), form_content, (0.0, 0.0, 612.0, 792.0))?;
    assert!(name.starts_with("Fm") || name.starts_with("Im"));

    writer.draw_xobject(&name, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let page_dict = doc.get_object_ref(page.id).and_then(|o| o.as_dictionary()).unwrap();
    let resources = page_dict.get(&CosName::resources()).and_then(|r| r.as_dictionary()).unwrap();
    let xobjects = resources.get(&CosName::new(b"XObject".to_vec())).and_then(|x| x.as_dictionary()).unwrap();
    let _form_ref = xobjects.get(&CosName::new(name.as_bytes().to_vec())).and_then(|o| o.as_reference()).expect("form");

    Ok(())
}

#[test]
fn test_form_xobject_bbox() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let name = writer.register_form_xobject(Some("Stamp"), b"q Q".to_vec(), (10.0, 20.0, 100.0, 200.0))?;
    writer.close()?;

    let page = doc.pages()?.get(0).unwrap();
    let page_dict = doc.get_object_ref(page.id).and_then(|o| o.as_dictionary()).unwrap();
    let xobjects = page_dict
        .get(&CosName::resources())
        .and_then(|r| r.as_dictionary())
        .and_then(|res| res.get(&CosName::new(b"XObject".to_vec())))
        .and_then(|x| x.as_dictionary())
        .unwrap();
    let stream = xobjects
        .get(&CosName::new(name.as_bytes().to_vec()))
        .and_then(|o| o.as_reference())
        .and_then(|id| doc.get_object_ref(id))
        .and_then(|o| o.as_stream())
        .unwrap();

    let bbox = stream.dictionary.get(&CosName::new(b"BBox".to_vec())).and_then(|o| o.as_array()).unwrap();
    assert_eq!(bbox.len(), 4);
    assert!((bbox[0].as_number().unwrap() - 10.0).abs() < 1e-9);
    assert!((bbox[3].as_number().unwrap() - 200.0).abs() < 1e-9);
    Ok(())
}

// =========================================================================
// Helper: resolve content stream bytes from a page
// =========================================================================

fn resolve_content_stream(doc: &rust_pdfbox::Document, page: &rust_pdfbox::pdmodel::page::Page<'_>) -> PdfResult<Vec<u8>> {
    let contents_obj = page.contents_object().ok_or_else(|| rust_pdfbox::PdfError::Parse {
        offset: None,
        context: "no contents".to_string(),
    })?;

    match contents_obj {
        CosObject::Reference(id) => {
            let stream = doc.get_object_ref(*id).and_then(|obj| obj.as_stream()).ok_or_else(|| {
                rust_pdfbox::PdfError::Parse {
                    offset: None,
                    context: "not a stream".to_string(),
                }
            })?;
            Ok(stream.data.clone())
        }
        CosObject::Array(arr) => {
            let mut combined = Vec::new();
            for item in arr {
                if let Some(ref_id) = item.as_reference() {
                    if let Some(stream) = doc.get_object_ref(ref_id).and_then(|o| o.as_stream()) {
                        combined.extend_from_slice(&stream.data);
                    }
                }
            }
            Ok(combined)
        }
        CosObject::Stream(stream) => Ok(stream.data.clone()),
        _ => Err(rust_pdfbox::PdfError::Parse {
            offset: None,
            context: "unexpected contents type".to_string(),
        }),
    }
}
