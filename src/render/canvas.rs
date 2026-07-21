//! Holds the 2D canvas state.

pub struct CanvasState {
    pub line_width: f32,
    pub fill_alpha: f32,
    pub stroke_alpha: f32,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            line_width: 1.0,
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
        }
    }
}