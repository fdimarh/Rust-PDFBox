//! PDF Graphics State — tracks the current graphics state during content stream processing.
//!
//! Maps to Java PDFBox `PDGraphicsState` and `PDTextState`.
//!
//! # State managed here
//!
//! | PDF operator | Field updated |
//! |---|---|
//! | `cm a b c d e f` | `ctm` (current transformation matrix) |
//! | `q` | push copy of current state onto stack |
//! | `Q` | pop state from stack |
//! | `BT` | enter text mode; reset text matrix |
//! | `ET` | leave text mode |
//! | `Tm a b c d e f` | set text matrix + text line matrix |
//! | `Td tx ty` | move text position by (tx, ty) |
//! | `TD tx ty` | move + set leading = −ty |
//! | `T* ` | move to next line using current leading |
//! | `Tf name size` | set current font name + size |
//! | `TL leading` | set text leading |
//! | `Tc spacing` | set character spacing |
//! | `Tw spacing` | set word spacing |
//! | `Tz scale` | set horizontal scaling |
//! | `Ts rise` | set text rise |

/// A 2-D affine transformation matrix in column-major PDF form: [a b c d e f].
///
/// Corresponds to the matrix:
/// ```text
/// [ a  b  0 ]
/// [ c  d  0 ]
/// [ e  f  1 ]
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Matrix {
    /// The identity matrix.
    pub fn identity() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }

    /// Multiply `self × other` (right-multiply, PDF convention).
    pub fn multiply(&self, o: &Matrix) -> Matrix {
        Matrix {
            a: self.a * o.a + self.b * o.c,
            b: self.a * o.b + self.b * o.d,
            c: self.c * o.a + self.d * o.c,
            d: self.c * o.b + self.d * o.d,
            e: self.e * o.a + self.f * o.c + o.e,
            f: self.e * o.b + self.f * o.d + o.f,
        }
    }

    /// Translate matrix by (tx, ty).
    pub fn translate(tx: f64, ty: f64) -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: tx, f: ty }
    }

    /// Returns the X translation component.
    pub fn tx(&self) -> f64 { self.e }

    /// Returns the Y translation component.
    pub fn ty(&self) -> f64 { self.f }
}

impl Default for Matrix {
    fn default() -> Self { Self::identity() }
}

// ---------------------------------------------------------------------------
// Text state
// ---------------------------------------------------------------------------

/// PDF text state — the mutable text-related parameters within a BT/ET block.
///
/// Maps to Java PDFBox `PDTextState`.
#[derive(Debug, Clone)]
pub struct TextState {
    /// Current font resource name (e.g. `F1`).
    pub font_name: Option<String>,
    /// Current font size in unscaled text space units.
    pub font_size: f64,
    /// Character spacing (Tc).
    pub char_spacing: f64,
    /// Word spacing (Tw).
    pub word_spacing: f64,
    /// Horizontal scaling percentage (Tz), default 100.
    pub horizontal_scaling: f64,
    /// Text leading (TL).
    pub leading: f64,
    /// Text rise (Ts).
    pub rise: f64,
    /// Text matrix (Tm) — set by `Tm`, updated by `Td`, `TD`, `T*`, text operators.
    pub text_matrix: Matrix,
    /// Text line matrix — set by `Tm`, `Td`, `TD`, `T*`.
    pub line_matrix: Matrix,
    /// Whether we are currently inside a BT/ET block.
    pub in_text_object: bool,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font_name: None,
            font_size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            leading: 0.0,
            rise: 0.0,
            text_matrix: Matrix::identity(),
            line_matrix: Matrix::identity(),
            in_text_object: false,
        }
    }
}

impl TextState {
    /// Move text position by (tx, ty) — `Td` operator.
    pub fn move_text(&mut self, tx: f64, ty: f64) {
        let translation = Matrix::translate(tx, ty);
        self.line_matrix = translation.multiply(&self.line_matrix);
        self.text_matrix = self.line_matrix.clone();
    }

    /// Move text position and set leading — `TD tx ty` sets leading = −ty then moves.
    pub fn move_text_set_leading(&mut self, tx: f64, ty: f64) {
        self.leading = -ty;
        self.move_text(tx, ty);
    }

    /// Move to next line using current leading — `T*`.
    pub fn next_line(&mut self) {
        self.move_text(0.0, -self.leading);
    }

    /// Set text matrix from 6 values — `Tm a b c d e f`.
    pub fn set_text_matrix(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.text_matrix = Matrix { a, b, c, d, e, f };
        self.line_matrix = self.text_matrix.clone();
    }

    /// Advance the text matrix horizontally by `width` in text space units.
    ///
    /// Called after each glyph is rendered to advance the current position.
    pub fn advance_text_position(&mut self, width: f64) {
        let dx = width * self.font_size * (self.horizontal_scaling / 100.0);
        self.text_matrix.e += dx * self.text_matrix.a;
        self.text_matrix.f += dx * self.text_matrix.b;
    }
}

// ---------------------------------------------------------------------------
// Graphics state snapshot
// ---------------------------------------------------------------------------

/// A snapshot of the complete graphics state, saved/restored by `q`/`Q`.
#[derive(Debug, Clone)]
pub struct GraphicsStateSnapshot {
    /// Current Transformation Matrix.
    pub ctm: Matrix,
    /// Text state snapshot.
    pub text: TextState,
}

impl Default for GraphicsStateSnapshot {
    fn default() -> Self {
        Self {
            ctm: Matrix::identity(),
            text: TextState::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// GraphicsState — the live state machine
// ---------------------------------------------------------------------------

/// The mutable PDF graphics state machine.
///
/// Processes graphics/text operators and maintains the current state.
/// Maps to Java PDFBox `PDGraphicsState` + `PDFStreamEngine` state loop.
#[derive(Debug)]
pub struct GraphicsState {
    /// Current Transformation Matrix.
    pub ctm: Matrix,
    /// Current text state.
    pub text: TextState,
    /// State stack for `q`/`Q` operators.
    stack: Vec<GraphicsStateSnapshot>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix::identity(),
            text: TextState::default(),
            stack: Vec::new(),
        }
    }
}

impl GraphicsState {
    pub fn new() -> Self { Self::default() }

    /// `q` — save current state.
    pub fn save(&mut self) {
        self.stack.push(GraphicsStateSnapshot {
            ctm: self.ctm.clone(),
            text: self.text.clone(),
        });
    }

    /// `Q` — restore most recently saved state. No-op if stack is empty.
    pub fn restore(&mut self) {
        if let Some(snap) = self.stack.pop() {
            self.ctm = snap.ctm;
            self.text = snap.text;
        }
    }

    /// `cm a b c d e f` — concatenate matrix onto CTM.
    pub fn concat_matrix(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        let m = Matrix { a, b, c, d, e, f };
        self.ctm = m.multiply(&self.ctm);
    }

    /// `BT` — begin text object.
    pub fn begin_text(&mut self) {
        self.text.text_matrix = Matrix::identity();
        self.text.line_matrix = Matrix::identity();
        self.text.in_text_object = true;
    }

    /// `ET` — end text object.
    pub fn end_text(&mut self) {
        self.text.in_text_object = false;
    }

    /// `Tf name size` — set font.
    pub fn set_font(&mut self, name: impl Into<String>, size: f64) {
        self.text.font_name = Some(name.into());
        self.text.font_size = size;
    }

    /// `TL leading` — set text leading.
    pub fn set_leading(&mut self, leading: f64) {
        self.text.leading = leading;
    }

    /// `Tc spacing` — set character spacing.
    pub fn set_char_spacing(&mut self, spacing: f64) {
        self.text.char_spacing = spacing;
    }

    /// `Tw spacing` — set word spacing.
    pub fn set_word_spacing(&mut self, spacing: f64) {
        self.text.word_spacing = spacing;
    }

    /// `Tz scale` — set horizontal scaling.
    pub fn set_horizontal_scaling(&mut self, scale: f64) {
        self.text.horizontal_scaling = scale;
    }

    /// `Ts rise` — set text rise.
    pub fn set_text_rise(&mut self, rise: f64) {
        self.text.rise = rise;
    }

    /// `Tm a b c d e f` — set text matrix.
    pub fn set_text_matrix(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.text.set_text_matrix(a, b, c, d, e, f);
    }

    /// `Td tx ty` — move text position.
    pub fn move_text(&mut self, tx: f64, ty: f64) {
        self.text.move_text(tx, ty);
    }

    /// `TD tx ty` — move text position and set leading.
    pub fn move_text_set_leading(&mut self, tx: f64, ty: f64) {
        self.text.move_text_set_leading(tx, ty);
    }

    /// `T*` — move to next line.
    pub fn next_line(&mut self) {
        self.text.next_line();
    }

    /// Returns the current text position in user space.
    pub fn text_position(&self) -> (f64, f64) {
        (self.text.text_matrix.tx(), self.text.text_matrix.ty())
    }

    /// Returns the effective font size (font_size scaled by CTM y-scale).
    pub fn effective_font_size(&self) -> f64 {
        let ctm_scale = (self.ctm.b * self.ctm.b + self.ctm.d * self.ctm.d).sqrt();
        if ctm_scale > 0.0 {
            self.text.font_size * ctm_scale
        } else {
            self.text.font_size
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matrix_multiply() {
        let id = Matrix::identity();
        let m = Matrix { a: 2.0, b: 0.0, c: 0.0, d: 3.0, e: 10.0, f: 20.0 };
        let r = id.multiply(&m);
        assert_eq!(r, m);
    }

    #[test]
    fn matrix_multiply_translation() {
        let t1 = Matrix::translate(10.0, 20.0);
        let t2 = Matrix::translate(5.0, 3.0);
        let r = t1.multiply(&t2);
        assert!((r.e - 15.0).abs() < 1e-10);
        assert!((r.f - 23.0).abs() < 1e-10);
    }

    #[test]
    fn save_restore_state() {
        let mut gs = GraphicsState::new();
        gs.set_font("F1", 12.0);
        gs.save();
        gs.set_font("F2", 24.0);
        assert_eq!(gs.text.font_name.as_deref(), Some("F2"));
        gs.restore();
        assert_eq!(gs.text.font_name.as_deref(), Some("F1"));
        assert!((gs.text.font_size - 12.0).abs() < 1e-10);
    }

    #[test]
    fn restore_empty_stack_is_noop() {
        let mut gs = GraphicsState::new();
        gs.restore(); // must not panic
    }

    #[test]
    fn begin_end_text() {
        let mut gs = GraphicsState::new();
        gs.begin_text();
        assert!(gs.text.in_text_object);
        gs.end_text();
        assert!(!gs.text.in_text_object);
    }

    #[test]
    fn move_text_updates_position() {
        let mut gs = GraphicsState::new();
        gs.begin_text();
        gs.move_text(100.0, 200.0);
        let (x, y) = gs.text_position();
        assert!((x - 100.0).abs() < 1e-10);
        assert!((y - 200.0).abs() < 1e-10);
    }

    #[test]
    fn move_text_accumulates() {
        let mut gs = GraphicsState::new();
        gs.begin_text();
        gs.move_text(50.0, 100.0);
        gs.move_text(20.0, 5.0);
        let (x, y) = gs.text_position();
        assert!((x - 70.0).abs() < 1e-10);
        assert!((y - 105.0).abs() < 1e-10);
    }

    #[test]
    fn move_text_set_leading_sets_leading() {
        let mut gs = GraphicsState::new();
        gs.begin_text();
        gs.move_text_set_leading(0.0, -14.0);
        assert!((gs.text.leading - 14.0).abs() < 1e-10);
    }

    #[test]
    fn next_line_uses_leading() {
        let mut gs = GraphicsState::new();
        gs.begin_text();
        gs.set_leading(12.0);
        gs.move_text(0.0, 100.0);
        gs.next_line();
        let (_, y) = gs.text_position();
        assert!((y - 88.0).abs() < 1e-10);
    }

    #[test]
    fn set_text_matrix_resets_position() {
        let mut gs = GraphicsState::new();
        gs.begin_text();
        gs.move_text(999.0, 999.0);
        gs.set_text_matrix(1.0, 0.0, 0.0, 1.0, 50.0, 75.0);
        let (x, y) = gs.text_position();
        assert!((x - 50.0).abs() < 1e-10);
        assert!((y - 75.0).abs() < 1e-10);
    }

    #[test]
    fn concat_matrix_updates_ctm() {
        let mut gs = GraphicsState::new();
        gs.concat_matrix(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        assert!((gs.ctm.a - 2.0).abs() < 1e-10);
    }

    #[test]
    fn set_font_updates_state() {
        let mut gs = GraphicsState::new();
        gs.set_font("Helvetica", 10.0);
        assert_eq!(gs.text.font_name.as_deref(), Some("Helvetica"));
        assert!((gs.text.font_size - 10.0).abs() < 1e-10);
    }

    #[test]
    fn text_state_defaults() {
        let ts = TextState::default();
        assert!((ts.horizontal_scaling - 100.0).abs() < 1e-10);
        assert!((ts.char_spacing).abs() < 1e-10);
        assert!(!ts.in_text_object);
    }

    #[test]
    fn advance_text_position() {
        let mut ts = TextState::default();
        ts.font_size = 12.0;
        ts.horizontal_scaling = 100.0;
        ts.text_matrix = Matrix::identity();
        // Advance by glyph width 0.5 (in units where 1.0 = 1 text unit)
        ts.advance_text_position(0.5);
        assert!((ts.text_matrix.e - 6.0).abs() < 1e-10);
    }
}

