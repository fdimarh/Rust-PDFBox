use rust_pdfbox::Document;
use rust_pdfbox::render::{PdfRenderer, RenderOptions};

#[test]
fn test_render_blank_page() {
    // For now, just verify the types compile. Add a real test when
    // PagePainter can handle empty content streams.
    let _options = RenderOptions {
        dpi: 72.0,
        render_annotations: false,
    };
    let _ = PdfRenderer;
}
