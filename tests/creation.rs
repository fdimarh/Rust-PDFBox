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
    writer.clip()?;
    writer.end_path()?;
    writer.add_rect(80.0, 680.0, 60.0, 20.0)?;
    writer.clip_even_odd()?;
    writer.end_path()?;
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

    assert!(
        instructions.iter().any(|ins| ins.operator.name.as_slice() == b"W"),
        "expected clipping operator W"
    );
    assert!(
        instructions.iter().any(|ins| ins.operator.name.as_slice() == b"W*"),
        "expected clipping operator W*"
    );

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

#[test]
fn test_content_stream_writer_register_and_draw_image_xobject() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;
    let rgb = vec![
        255, 0, 0, 0, 255, 0, // row 1: red, green
        0, 0, 255, 255, 255, 0, // row 2: blue, yellow
    ];
    let image_name = writer.register_image_xobject_rgb(Some("Im"), 2, 2, &rgb)?;
    writer.draw_registered_image(&image_name, 72.0, 500.0, 48.0, 48.0)?;
    writer.close()?;

    let page_tree = doc.pages()?;
    let page = page_tree.get(0).expect("expected page 0");

    let page_obj = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let resources = page_obj
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .expect("page should have local resources after image registration");
    let xobjects = resources
        .get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec()))
        .and_then(|obj| obj.as_dictionary())
        .expect("resources should contain XObject dictionary");
    let image_ref = xobjects
        .get(&rust_pdfbox::cos::CosName::new(image_name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .expect("registered image should be present in XObject dictionary");

    let image_stream = doc
        .get_object_ref(image_ref)
        .and_then(|obj| obj.as_stream())
        .expect("image xobject stream should exist");
    assert_eq!(
        image_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::subtype())
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("Image")
    );
    assert_eq!(
        image_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"Width".to_vec()))
            .and_then(|obj| obj.as_integer()),
        Some(2)
    );
    assert_eq!(
        image_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"Height".to_vec()))
            .and_then(|obj| obj.as_integer()),
        Some(2)
    );
    assert_eq!(image_stream.data.len(), 12);

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
    let do_op = instructions
        .iter()
        .find(|ins| ins.operator.name.as_slice() == b"Do")
        .expect("expected Do image operator");
    assert!(matches!(
        do_op.operands.first(),
        Some(CosObject::Name(name)) if name.as_str() == Some(image_name.as_str())
    ));

    Ok(())
}

#[test]
fn test_content_stream_writer_image_registration_validation() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let bad_rgb = vec![255, 0, 0];
    assert!(writer
        .register_image_xobject_rgb(Some("Bad"), 2, 2, &bad_rgb)
        .is_err());

    assert!(writer
        .draw_registered_image("Missing", 10.0, 10.0, 20.0, 20.0)
        .is_err());

    Ok(())
}

#[test]
fn test_content_stream_writer_register_encoded_images() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let dct_name = writer.register_image_xobject_dct_rgb8(Some("Jpeg"), 1, 1, &[0xFF, 0xD8, 0xFF])?;
    let flate_name = writer.register_image_xobject_flate_rgb8(Some("Fl"), 1, 1, &[0x78, 0x9C, 0x00])?;
    writer.draw_registered_image(&dct_name, 50.0, 520.0, 24.0, 24.0)?;
    writer.draw_registered_image(&flate_name, 80.0, 520.0, 24.0, 24.0)?;
    writer.close()?;

    let page_tree = doc.pages()?;
    let page = page_tree.get(0).expect("expected page 0");
    let page_dict = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let resources = page_dict
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .expect("page resources should exist");
    let xobjects = resources
        .get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec()))
        .and_then(|obj| obj.as_dictionary())
        .expect("xobject dictionary should exist");

    let dct_ref = xobjects
        .get(&rust_pdfbox::cos::CosName::new(dct_name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .expect("dct xobject should be present");
    let flate_ref = xobjects
        .get(&rust_pdfbox::cos::CosName::new(flate_name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .expect("flate xobject should be present");

    let dct_stream = doc
        .get_object_ref(dct_ref)
        .and_then(|obj| obj.as_stream())
        .expect("dct stream should exist");
    let flate_stream = doc
        .get_object_ref(flate_ref)
        .and_then(|obj| obj.as_stream())
        .expect("flate stream should exist");

    assert_eq!(
        dct_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::filter())
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DCTDecode")
    );
    assert_eq!(
        flate_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::filter())
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("FlateDecode")
    );

    Ok(())
}

#[test]
fn test_content_stream_writer_register_encoded_images_validation() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    assert!(writer
        .register_image_xobject_dct_rgb8(Some("J"), 0, 1, &[1])
        .is_err());
    assert!(writer
        .register_image_xobject_flate_rgb8(Some("F"), 1, 1, &[])
        .is_err());

    Ok(())
}

#[test]
fn test_content_stream_writer_register_gray_and_cmyk_images() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let gray_name = writer.register_image_xobject_gray8(Some("Gray"), 2, 1, &[0x00, 0xFF])?;
    let cmyk_name = writer.register_image_xobject_cmyk8(
        Some("Cmyk"),
        1,
        1,
        &[0x00, 0x00, 0x00, 0x00],
    )?;
    writer.draw_registered_image(&gray_name, 110.0, 520.0, 24.0, 12.0)?;
    writer.draw_registered_image(&cmyk_name, 140.0, 520.0, 24.0, 24.0)?;
    writer.close()?;

    let page_tree = doc.pages()?;
    let page = page_tree.get(0).expect("expected page 0");
    let page_dict = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let resources = page_dict
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .expect("page resources should exist");
    let xobjects = resources
        .get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec()))
        .and_then(|obj| obj.as_dictionary())
        .expect("xobject dictionary should exist");

    let gray_ref = xobjects
        .get(&rust_pdfbox::cos::CosName::new(gray_name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .expect("gray xobject should be present");
    let cmyk_ref = xobjects
        .get(&rust_pdfbox::cos::CosName::new(cmyk_name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .expect("cmyk xobject should be present");

    let gray_stream = doc
        .get_object_ref(gray_ref)
        .and_then(|obj| obj.as_stream())
        .expect("gray stream should exist");
    let cmyk_stream = doc
        .get_object_ref(cmyk_ref)
        .and_then(|obj| obj.as_stream())
        .expect("cmyk stream should exist");

    assert_eq!(
        gray_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DeviceGray")
    );
    assert_eq!(
        cmyk_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DeviceCMYK")
    );

    Ok(())
}

#[test]
fn test_content_stream_writer_register_gray_and_cmyk_validation() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    assert!(writer
        .register_image_xobject_gray8(Some("GrayBad"), 2, 2, &[0x00, 0xFF])
        .is_err());
    assert!(writer
        .register_image_xobject_cmyk8(Some("CmykBad"), 1, 1, &[0x00, 0x00, 0x00])
        .is_err());

    Ok(())
}

#[test]
fn test_content_stream_writer_register_encoded_gray_and_cmyk_images() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let dct_gray = writer.register_image_xobject_dct_gray8(Some("Dg"), 1, 1, &[0xFF, 0xD8, 0xFF])?;
    let flate_gray = writer.register_image_xobject_flate_gray8(Some("Fg"), 1, 1, &[0x78, 0x9C, 0x00])?;
    let dct_cmyk = writer.register_image_xobject_dct_cmyk8(Some("Dc"), 1, 1, &[0xFF, 0xD8, 0xFF])?;
    let flate_cmyk = writer.register_image_xobject_flate_cmyk8(Some("Fc"), 1, 1, &[0x78, 0x9C, 0x00])?;

    writer.draw_registered_image(&dct_gray, 170.0, 520.0, 20.0, 20.0)?;
    writer.draw_registered_image(&flate_gray, 195.0, 520.0, 20.0, 20.0)?;
    writer.draw_registered_image(&dct_cmyk, 220.0, 520.0, 20.0, 20.0)?;
    writer.draw_registered_image(&flate_cmyk, 245.0, 520.0, 20.0, 20.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).expect("expected page 0");
    let page_dict = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let xobjects = page_dict
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .and_then(|res| res.get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec())))
        .and_then(|obj| obj.as_dictionary())
        .expect("xobject dictionary should exist");

    let get_stream = |name: &str| {
        xobjects
            .get(&rust_pdfbox::cos::CosName::new(name.as_bytes().to_vec()))
            .and_then(|obj| obj.as_reference())
            .and_then(|id| doc.get_object_ref(id))
            .and_then(|obj| obj.as_stream())
            .expect("registered image stream should exist")
    };

    let dct_gray_stream = get_stream(&dct_gray);
    let flate_gray_stream = get_stream(&flate_gray);
    let dct_cmyk_stream = get_stream(&dct_cmyk);
    let flate_cmyk_stream = get_stream(&flate_cmyk);

    assert_eq!(
        dct_gray_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::filter())
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DCTDecode")
    );
    assert_eq!(
        flate_gray_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::filter())
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("FlateDecode")
    );
    assert_eq!(
        dct_cmyk_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DeviceCMYK")
    );
    assert_eq!(
        flate_cmyk_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::filter())
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("FlateDecode")
    );
    assert_eq!(
        flate_gray_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DeviceGray")
    );

    Ok(())
}

#[test]
fn test_content_stream_writer_register_encoded_gray_and_cmyk_validation() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    assert!(writer
        .register_image_xobject_dct_gray8(Some("bad"), 0, 1, &[1])
        .is_err());
    assert!(writer
        .register_image_xobject_flate_cmyk8(Some("bad"), 1, 1, &[])
        .is_err());

    Ok(())
}

#[test]
fn test_content_stream_writer_flate_decode_parms_persisted() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let mut decode_parms = rust_pdfbox::cos::CosDictionary::new();
    decode_parms.insert(
        rust_pdfbox::cos::CosName::new(b"Predictor".to_vec()),
        rust_pdfbox::cos::CosObject::Integer(15),
    );
    decode_parms.insert(
        rust_pdfbox::cos::CosName::new(b"Colors".to_vec()),
        rust_pdfbox::cos::CosObject::Integer(3),
    );
    decode_parms.insert(
        rust_pdfbox::cos::CosName::new(b"BitsPerComponent".to_vec()),
        rust_pdfbox::cos::CosObject::Integer(8),
    );
    decode_parms.insert(
        rust_pdfbox::cos::CosName::new(b"Columns".to_vec()),
        rust_pdfbox::cos::CosObject::Integer(1),
    );

    let image_name = writer.register_image_xobject_flate_rgb8_with_decode_parms(
        Some("Fp"),
        1,
        1,
        &[0x78, 0x9C, 0x00],
        Some(decode_parms),
    )?;
    writer.draw_registered_image(&image_name, 270.0, 520.0, 20.0, 20.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).expect("expected page 0");
    let page_dict = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let xobjects = page_dict
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .and_then(|res| res.get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec())))
        .and_then(|obj| obj.as_dictionary())
        .expect("xobject dictionary should exist");
    let image_stream = xobjects
        .get(&rust_pdfbox::cos::CosName::new(image_name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .and_then(|id| doc.get_object_ref(id))
        .and_then(|obj| obj.as_stream())
        .expect("image stream should exist");

    let decode_parms_dict = image_stream
        .dictionary
        .get(&rust_pdfbox::cos::CosName::new(b"DecodeParms".to_vec()))
        .and_then(|obj| obj.as_dictionary())
        .expect("DecodeParms dictionary should be present");
    assert_eq!(
        decode_parms_dict
            .get(&rust_pdfbox::cos::CosName::new(b"Predictor".to_vec()))
            .and_then(|obj| obj.as_integer()),
        Some(15)
    );

    Ok(())
}

#[test]
fn test_content_stream_writer_flate_decode_parms_validation() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let mut invalid = rust_pdfbox::cos::CosDictionary::new();
    invalid.insert(
        rust_pdfbox::cos::CosName::new(b"Predictor".to_vec()),
        rust_pdfbox::cos::CosObject::Integer(99),
    );

    assert!(writer
        .register_image_xobject_flate_rgb8_with_decode_parms(
            Some("BadParms"),
            1,
            1,
            &[0x78, 0x9C, 0x00],
            Some(invalid),
        )
        .is_err());

    Ok(())
}

#[test]
fn test_content_stream_writer_register_png_convenience() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let rgba = image::RgbaImage::from_raw(1, 1, vec![10, 20, 30, 128]).expect("rgba image");
    let mut rgba_cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba).write_to(&mut rgba_cursor, image::ImageFormat::Png).unwrap();
    let rgba_png = rgba_cursor.into_inner();

    let gray = image::GrayImage::from_raw(1, 1, vec![150]).expect("gray image");
    let mut gray_cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(gray).write_to(&mut gray_cursor, image::ImageFormat::Png).unwrap();
    let gray_png = gray_cursor.into_inner();

    let rgb_name = writer.register_image_xobject_png(Some("PngRgb"), &rgba_png)?;
    let gray_name = writer.register_image_xobject_png(Some("PngGray"), &gray_png)?;
    writer.draw_registered_image(&rgb_name, 300.0, 520.0, 20.0, 20.0)?;
    writer.draw_registered_image(&gray_name, 325.0, 520.0, 20.0, 20.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).expect("expected page 0");
    let page_dict = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let xobjects = page_dict
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .and_then(|res| res.get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec())))
        .and_then(|obj| obj.as_dictionary())
        .expect("xobject dictionary should exist");

    let resolve_stream = |name: &str| {
        xobjects
            .get(&rust_pdfbox::cos::CosName::new(name.as_bytes().to_vec()))
            .and_then(|obj| obj.as_reference())
            .and_then(|id| doc.get_object_ref(id))
            .and_then(|obj| obj.as_stream())
            .expect("image stream should exist")
    };

    let rgb_stream = resolve_stream(&rgb_name);
    let gray_stream = resolve_stream(&gray_name);

    assert_eq!(
        rgb_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DeviceRGB")
    );
    assert_eq!(
        gray_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DeviceGray")
    );
    let rgb_smask_ref = rgb_stream
        .dictionary
        .get(&rust_pdfbox::cos::CosName::new(b"SMask".to_vec()))
        .and_then(|obj| obj.as_reference())
        .expect("rgba png should produce SMask reference");
    let rgb_smask_stream = doc
        .get_object_ref(rgb_smask_ref)
        .and_then(|obj| obj.as_stream())
        .expect("smask stream should exist");
    assert_eq!(
        rgb_smask_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
            .and_then(|obj| obj.as_name())
            .and_then(|n| n.as_str()),
        Some("DeviceGray")
    );
    assert_eq!(rgb_smask_stream.data.len(), 1);

    assert!(
        gray_stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"SMask".to_vec()))
            .is_none(),
        "gray png without alpha should not include SMask"
    );

    Ok(())
}

#[test]
fn test_content_stream_writer_register_png_convenience_validation() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    assert!(writer.register_image_xobject_png(Some("BadPng"), &[]).is_err());
    assert!(writer
        .register_image_xobject_png(Some("BadPng"), b"not-a-png")
        .is_err());

    Ok(())
}

#[test]
fn test_content_stream_writer_register_png_opaque_alpha_omits_smask() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let rgba_opaque = image::RgbaImage::from_raw(1, 1, vec![44, 55, 66, 255]).expect("rgba image");
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba_opaque)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    let png = cursor.into_inner();

    let name = writer.register_image_xobject_png(Some("PngOpaque"), &png)?;
    writer.draw_registered_image(&name, 350.0, 520.0, 20.0, 20.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).expect("expected page 0");
    let page_dict = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let xobjects = page_dict
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .and_then(|res| res.get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec())))
        .and_then(|obj| obj.as_dictionary())
        .expect("xobject dictionary should exist");

    let stream = xobjects
        .get(&rust_pdfbox::cos::CosName::new(name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .and_then(|id| doc.get_object_ref(id))
        .and_then(|obj| obj.as_stream())
        .expect("image stream should exist");

    assert!(
        stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"SMask".to_vec()))
            .is_none(),
        "fully opaque alpha PNG should not emit SMask"
    );

    Ok(())
}

#[test]
fn test_content_stream_writer_register_indexed_png_preserves_palette() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut writer = ContentStreamWriter::new(&mut doc, 0)?;

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 2, 1);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![255, 0, 0, 0, 255, 0]);
        let mut png_writer = encoder.write_header().expect("indexed png header");
        png_writer
            .write_image_data(&[0, 1])
            .expect("indexed png data");
    }

    let image_name = writer.register_image_xobject_png(Some("PngIdx"), &png_bytes)?;
    writer.draw_registered_image(&image_name, 380.0, 520.0, 30.0, 15.0)?;
    writer.close()?;

    let page = doc.pages()?.get(0).expect("expected page 0");
    let page_dict = doc
        .get_object_ref(page.id)
        .and_then(|obj| obj.as_dictionary())
        .expect("page dictionary should exist");
    let xobjects = page_dict
        .get(&rust_pdfbox::cos::CosName::resources())
        .and_then(|obj| obj.as_dictionary())
        .and_then(|res| res.get(&rust_pdfbox::cos::CosName::new(b"XObject".to_vec())))
        .and_then(|obj| obj.as_dictionary())
        .expect("xobject dictionary should exist");
    let stream = xobjects
        .get(&rust_pdfbox::cos::CosName::new(image_name.as_bytes().to_vec()))
        .and_then(|obj| obj.as_reference())
        .and_then(|id| doc.get_object_ref(id))
        .and_then(|obj| obj.as_stream())
        .expect("indexed image stream should exist");

    let cs = stream
        .dictionary
        .get(&rust_pdfbox::cos::CosName::new(b"ColorSpace".to_vec()))
        .and_then(|obj| obj.as_array())
        .expect("indexed colorspace array should exist");
    assert!(matches!(cs.first(), Some(CosObject::Name(name)) if name.as_str() == Some("Indexed")));
    assert!(matches!(cs.get(1), Some(CosObject::Name(name)) if name.as_str() == Some("DeviceRGB")));
    assert_eq!(cs.get(2).and_then(|o| o.as_integer()), Some(1));
    assert!(matches!(cs.get(3), Some(CosObject::String(p)) if p == &vec![255, 0, 0, 0, 255, 0]));
    assert_eq!(stream.data, vec![0, 1]);
    assert!(
        stream
            .dictionary
            .get(&rust_pdfbox::cos::CosName::new(b"SMask".to_vec()))
            .is_none(),
        "opaque indexed png should not include SMask"
    );

    Ok(())
}

