use rust_pdfbox::pdmodel::{DocumentBuilder, PageSize};
use rust_pdfbox::content::parse_content_stream;
use rust_pdfbox::content::writer::{ContentStreamWriter, TextShowElement};
use rust_pdfbox::cos::CosObject;
use rust_pdfbox::PdfResult;

#[test]
fn test_create_and_read_pdf() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new()
        .page_size(PageSize::A4)
        .build()?;

    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.begin_text()?;
    writer.set_font("Helvetica", 12.0)?; // standard 14 font
    writer.move_to(72.0, 720.0)?;
    writer.show_text("Hello PDF World!")?;
    writer.end_text()?;
    writer.close()?;

    // Just save to a byte buffer
    let mut buf = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut buf).unwrap();

    // Reload the document from bytes
    let _reloaded_doc = rust_pdfbox::Document::load_from_bytes(buf.get_ref())?;

    // Persist generated PDF under tests/
    std::fs::create_dir_all("tests")?;
    std::fs::write("tests/generated.pdf", buf.get_ref())?;

    Ok(())
}

#[test]
fn test_content_stream_writer_extended_operators_roundtrip() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new()
        .page_size(PageSize::A4)
        .build()?;

    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    writer.save_state()?;
    writer.transform(1.0, 0.0, 0.0, 1.0, 5.0, 10.0)?;
    writer.set_line_width(1.5)?;
    writer.set_line_cap(1)?;
    writer.set_line_join(2)?;
    writer.set_miter_limit(3.0)?;
    writer.set_stroke_gray(0.2)?;
    writer.set_fill_gray(0.8)?;
    writer.add_rect(72.0, 700.0, 120.0, 30.0)?;
    writer.fill_and_stroke()?;
    writer.restore_state()?;

    writer.begin_text()?;
    writer.set_font("Helvetica", 11.0)?;
    writer.set_text_matrix(1.0, 0.0, 0.0, 1.0, 72.0, 660.0)?;
    writer.set_char_spacing(0.5)?;
    writer.set_word_spacing(1.0)?;
    writer.set_horizontal_scaling(100.0)?;
    writer.set_text_leading(14.0)?;
    writer.show_text("Line 1")?;
    writer.move_to_next_line()?;
    writer.set_text_rise(2.0)?;
    writer.show_text("Line 2")?;
    writer.show_text_positioned(&[
        TextShowElement::Text("A"),
        TextShowElement::Adjust(-120.0),
        TextShowElement::Text("B"),
    ])?;
    writer.show_text_next_line("Line 3")?;
    writer.show_text_next_line_with_spacing(2.0, 0.25, "Line 4")?;
    writer.end_text()?;
    writer.close()?;

    let page_tree = doc.pages()?;
    let page = page_tree.get(0).expect("expected page 0");
    let contents_obj = page.contents_object().expect("expected page contents");
    let stream_data = match contents_obj {
        CosObject::Reference(id) => doc
            .get_object_ref(*id)
            .and_then(|obj| obj.as_stream())
            .map(|s| s.data.clone())
            .expect("expected contents stream"),
        CosObject::Array(arr) => {
            let first_ref = arr
                .iter()
                .find_map(|obj| obj.as_reference())
                .expect("expected at least one contents reference");
            doc.get_object_ref(first_ref)
                .and_then(|obj| obj.as_stream())
                .map(|s| s.data.clone())
                .expect("expected referenced contents stream")
        }
        _ => panic!("unexpected contents object type"),
    };

    let instructions = parse_content_stream(&stream_data).expect("content stream should parse");
    let positioned_text = instructions
        .iter()
        .find(|ins| ins.operator.is_show_text_positioned())
        .expect("expected TJ instruction");
    assert!(matches!(
        positioned_text.operands.first(),
        Some(CosObject::Array(arr)) if arr.len() == 3
    ));

    let quote_single = instructions
        .iter()
        .find(|ins| ins.operator.name.as_slice() == b"'")
        .expect("expected single-quote text operator");
    assert!(matches!(
        quote_single.operands.first(),
        Some(CosObject::String(s)) if s == b"Line 3"
    ));

    let quote_double = instructions
        .iter()
        .find(|ins| ins.operator.name.as_slice() == b"\"")
        .expect("expected double-quote text operator");
    assert_eq!(quote_double.operands.len(), 3);
    let word_spacing = quote_double.operands[0].as_number().expect("word spacing");
    let char_spacing = quote_double.operands[1].as_number().expect("char spacing");
    assert!((word_spacing - 2.0).abs() < 1e-9);
    assert!((char_spacing - 0.25).abs() < 1e-9);
    assert!(matches!(
        quote_double.operands.get(2),
        Some(CosObject::String(s)) if s == b"Line 4"
    ));

    let mut buf = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut buf)?;

    let reloaded_doc = rust_pdfbox::Document::load_from_bytes(buf.get_ref())?;
    assert_eq!(reloaded_doc.page_count(), 1);

    Ok(())
}

