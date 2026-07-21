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
    /// Dots per inch. Default is 72.0 (1 unit = 1 pixel).
    pub dpi: f32,
    /// Render annotations?
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
        
        // Allocate a pixel buffer
        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.fill(tiny_skia::Color::WHITE);
        
        // TODO: Bridge with `ContentStream` and `tiny_skia::PathBuilder`.
        // This will be expanded with `painter::PagePainter`.
        
        // Convert tiny-skia Pixmap to standard `image::DynamicImage`
        let rgba = RgbaImage::from_raw(width, height, pixmap.data().to_vec())
            .unwrap();
            
        Ok(DynamicImage::ImageRgba8(rgba))
    }
}