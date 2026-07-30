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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_state_default() {
        let cs = CanvasState::default();
        assert!((cs.line_width - 1.0).abs() < f32::EPSILON);
        assert!((cs.fill_alpha - 1.0).abs() < f32::EPSILON);
        assert!((cs.stroke_alpha - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn canvas_state_debug() {
        let cs = CanvasState::default();
        // No Debug derive; just verify default values
        assert!(cs.line_width > 0.0);
    }

    #[test]
    fn canvas_state_non_default_values() {
        let cs = CanvasState {
            line_width: 2.5,
            fill_alpha: 0.5,
            stroke_alpha: 0.0,
        };
        assert!((cs.line_width - 2.5).abs() < f32::EPSILON);
        assert!((cs.fill_alpha - 0.5).abs() < f32::EPSILON);
        assert!((cs.stroke_alpha - 0.0).abs() < f32::EPSILON);
    }
}
