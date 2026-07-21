use rust_pdfbox::Document;
use rust_pdfbox::render::{PdfRenderer, RenderOptions};
use std::fs;

#[test]
fn test_render_blank_page() {
    let mut doc = Document::empty();
    let page = doc.add_page();
    
    let options = RenderOptions { dpi: 72.0, render_annotations: false };
    let img = PdfRenderer::render_page(&page, &options).expect("Render failed");
    
    assert_eq!(img.width(), 612);
    assert_eq!(img.height(), 792);
}