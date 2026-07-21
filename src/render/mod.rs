//! PDF Rendering Engine.
//!
//! Converts PDF pages (`Page`) into raster images (e.g. PNG).
//! This uses `tiny-skia` as a 2D rendering backend.

pub mod painter;
pub mod canvas;

use crate::pdmodel::page::Page;
use crate::PdfResult;
use image::{RgbaImage, DynamicImage};
use tiny_skia::Pixmap;

/// Configuration for PDF to Image rendering.
pub struct RenderOptions {
    pub dpi: f32,
    pub render_annotations: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            dpi: 72.0,
            render_annotations: true,
        }
    }
}

/// Renderer responsible for converting a PDF page into a raster image.
pub struct PdfRenderer;

impl PdfRenderer {
    /// Render a specific page at the given DPI.
    pub fn render_page(page: &Page, options: &RenderOptions) -> PdfResult<DynamicImage> {
        let bbox = page.crop_box().or_else(|| page.media_box()).unwrap_or(crate::pdmodel::page::Rectangle::new(0.0, 0.0, 612.0, 792.0));
        
        let scale = options.dpi as f64 / 72.0;
        let width = (bbox.width() as f64 * scale).round() as u32;
        let height = (bbox.height() as f64 * scale).round() as u32;
        
        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.fill(tiny_skia::Color::WHITE);
        
        let mut painter = painter::PagePainter::new(pixmap.as_mut(), tiny_skia::Transform::default());
        let _ = painter.paint_page(page);
        
        let rgba = RgbaImage::from_raw(width, height, pixmap.data().to_vec()).unwrap();
        Ok(DynamicImage::ImageRgba8(rgba))
    }
}