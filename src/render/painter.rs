//! Bridging PDF Content Stream operators to Tiny-Skia Canvas.

use tiny_skia::{PathBuilder, Transform, Paint, PixmapMut, Color};

pub struct PagePainter<'a> {
    pixmap: PixmapMut<'a>,
    current_transform: Transform,
    path_builder: PathBuilder,
}

impl<'a> PagePainter<'a> {
    pub fn new(pixmap: PixmapMut<'a>, initial_transform: Transform) -> Self {
        Self {
            pixmap,
            current_transform: initial_transform,
            path_builder: PathBuilder::new(),
        }
    }
    
    pub fn fill(&mut self) {
        let path = self.path_builder.clone().finish();
        if let Some(p) = path {
            let mut paint = Paint::default();
            paint.set_color(Color::BLACK); // Placeholder color
            self.pixmap.fill_path(&p, &paint, tiny_skia::FillRule::Winding, self.current_transform, None);
        }
        self.path_builder = PathBuilder::new();
    }
}