//! Integration tests for Page Manipulation (P15) features.
//!
//! Tests for PdfMerger, PdfSplitter, extract_pages, rotate_page,
//! PdfOverlay, and add_watermark.

use rust_pdfbox::content::parse_content_stream;
use rust_pdfbox::content::writer::ContentStreamWriter;
use rust_pdfbox::cos::{CosName, CosObject};
use rust_pdfbox::pageops::{
    add_watermark, extract_pages, rotate_page, PdfMerger, PdfOverlay, OverlayType,
    WatermarkConfig,
};
use rust_pdfbox::pdmodel::{DocumentBuilder, PageSize};
use rust_pdfbox::PdfResult;

// =========================================================================
// PdfMerger tests
// =========================================================================

#[test]
fn test_merge_two_docs() -> PdfResult<()> {
    let mut doc1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut doc2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    // Add distinct text to each
    {
        let mut cs = ContentStreamWriter::new(&mut doc1, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 720.0)?;
        cs.show_text("Document 1")?;
        cs.end_text()?;
        cs.close()?;
    }
    {
        let mut cs = ContentStreamWriter::new(&mut doc2, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 700.0)?;
        cs.show_text("Document 2")?;
        cs.end_text()?;
        cs.close()?;
    }

    let mut merger = PdfMerger::new();
    merger.append(&doc1)?;
    merger.append(&doc2)?;
    let merged = merger.finish();

    assert_eq!(merged.page_count(), 2);
    Ok(())
}

#[test]
fn test_merge_single_doc() -> PdfResult<()> {
    let doc = DocumentBuilder::new().page_size(PageSize::Letter).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&doc)?;
    let merged = merger.finish();

    assert_eq!(merged.page_count(), 1);
    Ok(())
}

#[test]
fn test_merge_three_docs() -> PdfResult<()> {
    let doc1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let doc2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let doc3 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&doc1)?;
    merger.append(&doc2)?;
    merger.append(&doc3)?;
    let merged = merger.finish();

    assert_eq!(merged.page_count(), 3);
    Ok(())
}

#[test]
fn test_merge_round_trip() -> PdfResult<()> {
    let mut doc1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let doc2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    {
        let mut cs = ContentStreamWriter::new(&mut doc1, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 720.0)?;
        cs.show_text("Hello from merged doc")?;
        cs.end_text()?;
        cs.close()?;
    }

    let mut merger = PdfMerger::new();
    merger.append(&doc1)?;
    merger.append(&doc2)?;
    let merged = merger.finish();

    // Save and reload
    let mut buf = std::io::Cursor::new(Vec::new());
    merged.save_to(&mut buf)?;
    let reloaded = rust_pdfbox::Document::load_from_bytes(buf.get_ref())?;
    assert_eq!(reloaded.page_count(), 2);
    Ok(())
}

// =========================================================================
// PdfSplitter tests
// =========================================================================

#[test]
fn test_split_two_pages() -> PdfResult<()> {
    // Build a 2-page doc
    let doc1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let doc2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&doc1)?;
    merger.append(&doc2)?;
    let mut merged = merger.finish();

    let mut splitter = rust_pdfbox::pageops::PdfSplitter::new(&mut merged);
    let parts = splitter.split(1)?;
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].page_count(), 1);
    assert_eq!(parts[1].page_count(), 1);
    Ok(())
}

#[test]
fn test_split_into_chunks() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    // Add a second page manually via merger
    let doc2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut merger = PdfMerger::new();
    merger.append(&doc)?;
    merger.append(&doc2)?;
    let mut merged = merger.finish();

    // Add third page
    let doc3 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut merger2 = PdfMerger::new();
    merger2.append(&merged)?;
    merger2.append(&doc3)?;
    let mut merged2 = merger2.finish();

    let mut splitter = rust_pdfbox::pageops::PdfSplitter::new(&mut merged2);
    let parts = splitter.split(2)?;
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].page_count(), 2);
    assert_eq!(parts[1].page_count(), 1);
    Ok(())
}

#[test]
fn test_split_single_page() -> PdfResult<()> {
    let doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut merger = PdfMerger::new();
    merger.append(&doc)?;
    let mut merged = merger.finish();

    let mut splitter = rust_pdfbox::pageops::PdfSplitter::new(&mut merged);
    let parts = splitter.split(5)?;
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].page_count(), 1);
    Ok(())
}

#[test]
fn test_split_can_be_reloaded() -> PdfResult<()> {
    let doc1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let doc2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let mut merger = PdfMerger::new();
    merger.append(&doc1)?;
    merger.append(&doc2)?;
    let mut merged = merger.finish();

    let mut splitter = rust_pdfbox::pageops::PdfSplitter::new(&mut merged);
    let parts = splitter.split(1)?;

    for part in &parts {
        let mut buf = std::io::Cursor::new(Vec::new());
        part.save_to(&mut buf)?;
        let reloaded = rust_pdfbox::Document::load_from_bytes(buf.get_ref())?;
        assert_eq!(reloaded.page_count(), 1);
    }
    Ok(())
}

// =========================================================================
// extract_pages tests
// =========================================================================

#[test]
fn test_extract_middle_pages() -> PdfResult<()> {
    let d1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d3 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&d1)?;
    merger.append(&d2)?;
    merger.append(&d3)?;
    let mut merged = merger.finish();

    let extracted = extract_pages(&mut merged, &[1])?;
    assert_eq!(extracted.page_count(), 1);
    Ok(())
}

#[test]
fn test_extract_first_page() -> PdfResult<()> {
    let d1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&d1)?;
    merger.append(&d2)?;
    let mut merged = merger.finish();

    let extracted = extract_pages(&mut merged, &[0])?;
    assert_eq!(extracted.page_count(), 1);
    Ok(())
}

#[test]
fn test_extract_last_page() -> PdfResult<()> {
    let d1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d3 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&d1)?;
    merger.append(&d2)?;
    merger.append(&d3)?;
    let mut merged = merger.finish();

    let extracted = extract_pages(&mut merged, &[2])?;
    assert_eq!(extracted.page_count(), 1);
    Ok(())
}

#[test]
fn test_extract_multiple_pages() -> PdfResult<()> {
    let d1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d3 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d4 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&d1)?;
    merger.append(&d2)?;
    merger.append(&d3)?;
    merger.append(&d4)?;
    let mut merged = merger.finish();

    let extracted = extract_pages(&mut merged, &[0, 2])?;
    assert_eq!(extracted.page_count(), 2);
    Ok(())
}

#[test]
fn test_extract_all_pages() -> PdfResult<()> {
    let d1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&d1)?;
    merger.append(&d2)?;
    let mut merged = merger.finish();

    let extracted = extract_pages(&mut merged, &[0, 1])?;
    assert_eq!(extracted.page_count(), 2);
    Ok(())
}

// =========================================================================
// rotate_page tests
// =========================================================================

#[test]
fn test_rotate_page_90() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    rotate_page(&mut doc, 0, 90)?;

    let tree = doc.pages()?;
    let page = tree.get(0).unwrap();
    assert_eq!(page.rotation(), 90);
    Ok(())
}

#[test]
fn test_rotate_page_180() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    rotate_page(&mut doc, 0, 180)?;

    let tree = doc.pages()?;
    let page = tree.get(0).unwrap();
    assert_eq!(page.rotation(), 180);
    Ok(())
}

#[test]
fn test_rotate_page_270() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    rotate_page(&mut doc, 0, 270)?;

    let tree = doc.pages()?;
    let page = tree.get(0).unwrap();
    assert_eq!(page.rotation(), 270);
    Ok(())
}

#[test]
fn test_rotate_page_360_is_zero() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    rotate_page(&mut doc, 0, 360)?;

    let tree = doc.pages()?;
    let page = tree.get(0).unwrap();
    assert_eq!(page.rotation(), 0);
    Ok(())
}

#[test]
fn test_rotate_page_negative() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    rotate_page(&mut doc, 0, -90)?;

    let tree = doc.pages()?;
    let page = tree.get(0).unwrap();
    assert_eq!(page.rotation(), 270);
    Ok(())
}

#[test]
fn test_rotate_page_accumulates() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    rotate_page(&mut doc, 0, 45)?;
    rotate_page(&mut doc, 0, 45)?;

    let tree = doc.pages()?;
    let page = tree.get(0).unwrap();
    assert_eq!(page.rotation(), 90);
    Ok(())
}

#[test]
fn test_rotate_page_invalid_index() {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build().unwrap();
    let result = rotate_page(&mut doc, 99, 90);
    assert!(result.is_err());
}

// =========================================================================
// PdfOverlay tests
// =========================================================================

#[test]
fn test_overlay_full_page() -> PdfResult<()> {
    let mut base = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    {
        let mut cs = ContentStreamWriter::new(&mut base, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 720.0)?;
        cs.show_text("Base content")?;
        cs.end_text()?;
        cs.close()?;
    }

    let overlay = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let op = PdfOverlay::new().overlay_type(OverlayType::FullPage);
    assert!(op.apply(&mut base, &overlay).is_ok());
    Ok(())
}

#[test]
fn test_overlay_header() -> PdfResult<()> {
    let mut base = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    {
        let mut cs = ContentStreamWriter::new(&mut base, 0)?;
        cs.close()?;
    }

    let overlay = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let op = PdfOverlay::new().overlay_type(OverlayType::Header);
    assert!(op.apply(&mut base, &overlay).is_ok());
    Ok(())
}

#[test]
fn test_overlay_footer() -> PdfResult<()> {
    let mut base = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let overlay = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let op = PdfOverlay::new().overlay_type(OverlayType::Footer);
    assert!(op.apply(&mut base, &overlay).is_ok());
    Ok(())
}

// =========================================================================
// add_watermark tests
// =========================================================================

#[test]
fn test_watermark_default_config() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    {
        let mut cs = ContentStreamWriter::new(&mut doc, 0)?;
        cs.close()?;
    }

    let result = add_watermark(&mut doc, "DRAFT", WatermarkConfig::default());
    assert!(result.is_ok());

    let tree = doc.pages()?;
    let page = tree.get(0).unwrap();
    let contents = page.contents_object();
    assert!(contents.is_some());
    Ok(())
}

#[test]
fn test_watermark_underlay() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    {
        let mut cs = ContentStreamWriter::new(&mut doc, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 720.0)?;
        cs.show_text("Visible")?;
        cs.end_text()?;
        cs.close()?;
    }

    let mut cfg = WatermarkConfig::default();
    cfg.underlay = true;
    let result = add_watermark(&mut doc, "CONFIDENTIAL", cfg);
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn test_watermark_custom_position() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let cfg = WatermarkConfig {
        vertical_position: 0.1,
        font_size: 36.0,
        rotation: 0.0,
        ..Default::default()
    };

    let result = add_watermark(&mut doc, "BOTTOM", cfg);
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn test_watermark_round_trip() -> PdfResult<()> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let result = add_watermark(&mut doc, "SAMPLE", WatermarkConfig::default());
    assert!(result.is_ok());

    let mut buf = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut buf)?;
    let reloaded = rust_pdfbox::Document::load_from_bytes(buf.get_ref())?;
    assert_eq!(reloaded.page_count(), 1);
    Ok(())
}

// =========================================================================
// Combined / Integration tests
// =========================================================================

#[test]
fn test_merge_then_split() -> PdfResult<()> {
    let d1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d3 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&d1)?;
    merger.append(&d2)?;
    merger.append(&d3)?;
    let mut merged = merger.finish();

    let mut splitter = rust_pdfbox::pageops::PdfSplitter::new(&mut merged);
    let parts = splitter.split(2)?;
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].page_count(), 2);
    assert_eq!(parts[1].page_count(), 1);

    // Re-merge split parts
    let mut re_merger = PdfMerger::new();
    re_merger.append(&parts[0])?;
    re_merger.append(&parts[1])?;
    let re_merged = re_merger.finish();
    assert_eq!(re_merged.page_count(), 3);
    Ok(())
}

#[test]
fn test_extract_then_watermark() -> PdfResult<()> {
    let d1 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d2 = DocumentBuilder::new().page_size(PageSize::A4).build()?;
    let d3 = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let mut merger = PdfMerger::new();
    merger.append(&d1)?;
    merger.append(&d2)?;
    merger.append(&d3)?;
    let mut merged = merger.finish();

    let mut extracted = extract_pages(&mut merged, &[0, 2])?;
    assert_eq!(extracted.page_count(), 2);

    let result = add_watermark(&mut extracted, "EXTRACT", WatermarkConfig::default());
    assert!(result.is_ok());
    Ok(())
}
