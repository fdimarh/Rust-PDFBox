use rust_pdfbox::pdmodel::{DocumentBuilder, PageSize};
use rust_pdfbox::content::writer::ContentStreamWriter;
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
    let reloaded_doc = rust_pdfbox::Document::load_from_bytes(buf.get_ref())?;

    // Persist generated PDF under tests/
    std::fs::create_dir_all("tests")?;
    std::fs::write("tests/generated.pdf", buf.get_ref())?;

    Ok(())
}

