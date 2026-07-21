//! Bridging PDF Content Stream operators to Tiny-Skia Canvas.

use tiny_skia::{PathBuilder, Transform, Paint, PixmapMut, Color};
use crate::content::{parse_content_stream, Instruction};
use crate::pdmodel::page::Page;
use crate::PdfResult;

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
                _ => return Ok(()), // no valid contents
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
            Some("m") => {
                if instruction.operands.len() == 2 {
                    let x = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    self.path_builder.move_to(x, y);
                }
            }
            Some("l") => {
                if instruction.operands.len() == 2 {
                    let x = instruction.operands[0].as_real().unwrap_or(0.0) as f32;
                    let y = instruction.operands[1].as_real().unwrap_or(0.0) as f32;
                    self.path_builder.line_to(x, y);
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
                }
            }
            Some("h") => {
                self.path_builder.close();
            }
            Some("f") | Some("F") | Some("f*") | Some("S") | Some("s") => {
                self.fill();
            }
            _ => {}
        }
    }

    pub fn fill(&mut self) {
        let path = self.path_builder.clone().finish();
        if let Some(p) = path {
            let mut paint = Paint::default();
            paint.set_color(Color::BLACK); // Placeholder
            self.pixmap.fill_path(&p, &paint, tiny_skia::FillRule::Winding, self.current_transform, None);
        }
        self.path_builder = PathBuilder::new();
    }
}