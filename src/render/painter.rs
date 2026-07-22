//! Bridging PDF Content Stream operators to Tiny-Skia Canvas.

use tiny_skia::{PathBuilder, Transform, Paint, PixmapMut, Color, Stroke};
use crate::content::{parse_content_stream, Instruction};
use crate::pdmodel::page::Page;
use crate::PdfResult;

pub struct PagePainter<'a> {
    pixmap: PixmapMut<'a>,
    current_transform: Transform,
    path_builder: PathBuilder,
    current_pos: (f32, f32),
    fill_color: Color,
    stroke_color: Color,
    line_width: f32,
}

impl<'a> PagePainter<'a> {
    pub fn new(pixmap: PixmapMut<'a>, initial_transform: Transform) -> Self {
        Self {
            pixmap,
            current_transform: initial_transform,
            path_builder: PathBuilder::new(),
            current_pos: (0.0, 0.0),
            fill_color: Color::BLACK,
            stroke_color: Color::BLACK,
            line_width: 1.0,
        }
    }

    /// Iterates through the PDF content stream instructions and paints them to the canvas.
    pub fn paint_page(&mut self, page: &Page) -> PdfResult<()> {
        if let Some(contents) = page.contents_object() {
            // Read content bytes
            let content_bytes = match contents {
                crate::cos::CosObject::Stream(s) => s.data.clone(),
                crate::cos::CosObject::Array(arr) => {
                    let mut b = Vec::new();
                    for item in arr {
                        if let crate::cos::CosObject::Stream(s) = item {
                            b.extend(&s.data);
                        }
                    }
                    b
                }
                _ => return Ok(()),
            };

            let instructions = parse_content_stream(&content_bytes)
                .map_err(|e| crate::PdfError::Parse { offset: None, context: format!("render: {}", e) })?;

            for instruction in instructions {
                self.execute_instruction(&instruction);
            }
        }
        Ok(())
    }

    fn execute_instruction(&mut self, instruction: &Instruction) {
        match instruction.operator.as_str() {
            // Path Construction
            Some("m") => {
                if instruction.operands.len() == 2 {
                    let x = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    self.path_builder.move_to(x, y);
                    self.current_pos = (x, y);
                }
            }
            Some("l") => {
                if instruction.operands.len() == 2 {
                    let x = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    self.path_builder.line_to(x, y);
                    self.current_pos = (x, y);
                }
            }
            Some("c") => {
                if instruction.operands.len() == 6 {
                    let x1 = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y1 = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    let x2 = instruction.operands[2].as_real().unwrap_or(0.0) as f32;
                    let y2 = instruction.operands[3].as_real().unwrap_or(0.0) as f32;
                    let x3 = instruction.operands[4].as_real().unwrap_or(0.0) as f32;
                    let y3 = instruction.operands[5].as_real().unwrap_or(0.0) as f32;
                    self.path_builder.cubic_to(x1, y1, x2, y2, x3, y3);
                    self.current_pos = (x3, y3);
                }
            }
            Some("v") => {
                if instruction.operands.len() == 4 {
                    let x2 = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y2 = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    let x3 = instruction.operands[2].as_real().unwrap_or(0.0) as f32;
                    let y3 = instruction.operands[3].as_real().unwrap_or(0.0) as f32;
                    let (cx, cy) = self.current_pos;
                    self.path_builder.cubic_to(cx, cy, x2, y2, x3, y3);
                    self.current_pos = (x3, y3);
                }
            }
            Some("y") => {
                if instruction.operands.len() == 4 {
                    let x1 = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y1 = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    let x3 = instruction.operands[2].as_real().unwrap_or(0.0) as f32;
                    let y3 = instruction.operands[3].as_real().unwrap_or(0.0) as f32;
                    self.path_builder.cubic_to(x1, y1, x3, y3, x3, y3);
                    self.current_pos = (x3, y3);
                }
            }
            Some("h") => {
                self.path_builder.close();
            }
            Some("re") => {
                if instruction.operands.len() == 4 {
                    let x = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    let w = instruction.operands[2].as_real().unwrap_or(0.0) as f32;
                    let h = instruction.operands[3].as_real().unwrap_or(0.0) as f32;
                    self.path_builder.move_to(x, y);
                    self.path_builder.line_to(x + w, y);
                    self.path_builder.line_to(x + w, y + h);
                    self.path_builder.line_to(x, y + h);
                    self.path_builder.close();
                }
            }
            
            // Path Painting
            Some("S") => self.stroke(),
            Some("s") => { self.path_builder.close(); self.stroke(); }
            Some("f") | Some("F") => self.fill(tiny_skia::FillRule::Winding),
            Some("f*") => self.fill(tiny_skia::FillRule::EvenOdd),
            Some("B") => { self.fill(tiny_skia::FillRule::Winding); self.stroke(); }
            Some("B*") => { self.fill(tiny_skia::FillRule::EvenOdd); self.stroke(); }
            Some("b") => { self.path_builder.close(); self.fill(tiny_skia::FillRule::Winding); self.stroke(); }
            Some("b*") => { self.path_builder.close(); self.fill(tiny_skia::FillRule::EvenOdd); self.stroke(); }
            Some("n") => { self.path_builder = PathBuilder::new(); } // End path no-op (often for clipping)

            // Graphics State (Colors)
            Some("rg") => {
                if instruction.operands.len() == 3 {
                    let r = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let g = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    let b = instruction.operands[2].as_real().unwrap_or(0.0) as f32;
                    if let Some(c) = Color::from_rgba(r, g, b, 1.0) {
                        self.fill_color = c;
                    }
                }
            }
            Some("RG") => {
                if instruction.operands.len() == 3 {
                    let r = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let g = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    let b = instruction.operands[2].as_real().unwrap_or(0.0) as f32;
                    if let Some(c) = Color::from_rgba(r, g, b, 1.0) {
                        self.stroke_color = c;
                    }
                }
            }
            Some("w") => {
                if instruction.operands.len() == 1 {
                    self.line_width = instruction.operands[0].as_real().unwrap_or(1.0) as f32;
                }
            }
            Some("cm") => {
                if instruction.operands.len() == 6 {
                    let a = instruction.operands[0].as_real().unwrap_or(1.0) as f32;
                    let b = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    let c = instruction.operands[2].as_real().unwrap_or(0.0) as f32;
                    let d = instruction.operands[3].as_real().unwrap_or(1.0) as f32;
                    let e = instruction.operands[4].as_real().unwrap_or(0.0) as f32;
                    let f_val = instruction.operands[5].as_real().unwrap_or(0.0) as f32;
                    let matrix = Transform::from_row(a, b, c, d, e, f_val);
                    self.current_transform = self.current_transform.pre_concat(matrix);
                }
            }
            
            // Text Objects (Stub implementation for rendering)
            Some("BT") => {
                // Begin text object. Reset text line matrix and text matrix.
                // Normally handled via a full GraphicsState stack.
            }
            Some("ET") => {
                // End text object
            }
            Some("Tf") => {
                // Set text font and size
                // Example: /F1 12 Tf
            }
            Some("Tm") => {
                // Set text matrix
            }
            Some("Td") | Some("TD") => {
                // Move text position
            }
            Some("Tj") => {
                // Show text
                // Uses `ab_glyph` in full implementation to rasterize glyphs at current pos.
            }
            Some("TJ") => {
                // Show text with individual glyph positioning
            }

            _ => {}
        }
    }

    pub fn fill(&mut self, rule: tiny_skia::FillRule) {
        let path = self.path_builder.clone().finish();
        if let Some(p) = path {
            let mut paint = Paint::default();
            paint.set_color(self.fill_color);
            self.pixmap.fill_path(&p, &paint, rule, self.current_transform, None);
        }
        self.path_builder = PathBuilder::new();
    }

    pub fn stroke(&mut self) {
        let path = self.path_builder.clone().finish();
        if let Some(p) = path {
            let mut paint = Paint::default();
            paint.set_color(self.stroke_color);
            let mut stroke = Stroke::default();
            stroke.width = self.line_width;
            self.pixmap.stroke_path(&p, &paint, &stroke, self.current_transform, None);
        }
        self.path_builder = PathBuilder::new();
    }
}