//! Bridging PDF Content Stream operators to Tiny-Skia Canvas.

use tiny_skia::{PathBuilder, Transform, Paint, PixmapMut, Color, Stroke};
use crate::content::{parse_content_stream, Instruction};
use crate::cos::{CosName, CosObject};
use crate::pdmodel::page::Page;
use crate::PdfResult;

/// Represents the complete graphics state (PDF §8.4).
#[derive(Debug, Clone)]
struct GraphicsState {
    ctm: Transform,
    line_width: f32,
    fill_color: Color,
    stroke_color: Color,
    fill_alpha: f32,
    stroke_alpha: f32,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Transform::default(),
            line_width: 1.0,
            fill_color: Color::BLACK,
            stroke_color: Color::BLACK,
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
        }
    }
}

pub struct PagePainter<'a> {
    pixmap: PixmapMut<'a>,
    gs: GraphicsState,
    gs_stack: Vec<GraphicsState>,
    path_builder: PathBuilder,
    current_pos: (f32, f32),
    resources: Option<crate::pdmodel::Resources<'a>>, // page Resources dict reference
}

impl<'a> PagePainter<'a> {
    pub fn new(pixmap: PixmapMut<'a>, initial_transform: Transform) -> Self {
        Self {
            pixmap,
            gs: GraphicsState { ctm: initial_transform, ..Default::default() },
            gs_stack: Vec::new(),
            path_builder: PathBuilder::new(),
            current_pos: (0.0, 0.0),
            resources: None,
        }
    }

    /// Iterates through the PDF content stream instructions and paints them to the canvas.
    pub fn paint_page(&mut self, page: &'a Page) -> PdfResult<()> {
        // Cache page resources
        self.resources = page.resources();

        if let Some(contents) = page.contents_object() {
            let content_bytes = match contents {
                CosObject::Stream(s) => s.data.clone(),
                CosObject::Array(arr) => {
                    let mut b = Vec::new();
                    for item in arr {
                        if let CosObject::Stream(s) = item {
                            b.extend(&s.data);
                        }
                    }
                    b
                }
                _ => return Ok(()),
            };

            let instructions = parse_content_stream(&content_bytes)
                .map_err(|e| crate::PdfError::Parse {
                    offset: None,
                    context: format!("render: {e}"),
                })?;

            for instruction in &instructions {
                self.execute_instruction(instruction);
            }
        }
        Ok(())
    }

    fn execute_instruction(&mut self, instruction: &Instruction) {
        let op = instruction.operator.as_str();
        let ops = &instruction.operands;

        match op {
            // ── Graphics State Stack ──────────────────────────────────
            Some("q") => self.gs_stack.push(self.gs.clone()),
            Some("Q") => {
                if let Some(saved) = self.gs_stack.pop() {
                    self.gs = saved;
                }
            }

            // ── Path Construction ─────────────────────────────────────
            Some("m") => {
                if let (Some(x), Some(y)) = (ops.get(0), ops.get(1)) {
                    let (x, y) = (num_f32(x), num_f32(y));
                    self.path_builder.move_to(x, y);
                    self.current_pos = (x, y);
                }
            }
            Some("l") => {
                if let (Some(x), Some(y)) = (ops.get(0), ops.get(1)) {
                    let (x, y) = (num_f32(x), num_f32(y));
                    self.path_builder.line_to(x, y);
                    self.current_pos = (x, y);
                }
            }
            Some("c") => {
                if ops.len() >= 6 {
                    let points: Vec<f32> = ops.iter().take(6).map(|o| num_f32(o)).collect();
                    self.path_builder.cubic_to(points[0], points[1], points[2], points[3], points[4], points[5]);
                    self.current_pos = (points[4], points[5]);
                }
            }
            Some("v") => {
                if ops.len() >= 4 {
                    let (cx, cy) = self.current_pos;
                    let (x2, y2) = (num_f32(&ops[0]), num_f32(&ops[1]));
                    let (x3, y3) = (num_f32(&ops[2]), num_f32(&ops[3]));
                    self.path_builder.cubic_to(cx, cy, x2, y2, x3, y3);
                    self.current_pos = (x3, y3);
                }
            }
            Some("y") => {
                if ops.len() >= 4 {
                    let (x1, y1) = (num_f32(&ops[0]), num_f32(&ops[1]));
                    let (x3, y3) = (num_f32(&ops[2]), num_f32(&ops[3]));
                    self.path_builder.cubic_to(x1, y1, x3, y3, x3, y3);
                    self.current_pos = (x3, y3);
                }
            }
            Some("h") => {
                self.path_builder.close();
            }
            Some("re") => {
                if ops.len() >= 4 {
                    let x = num_f32(&ops[0]);
                    let y = num_f32(&ops[1]);
                    let w = num_f32(&ops[2]);
                    let h = num_f32(&ops[3]);
                    self.path_builder.move_to(x, y);
                    self.path_builder.line_to(x + w, y);
                    self.path_builder.line_to(x + w, y + h);
                    self.path_builder.line_to(x, y + h);
                    self.path_builder.close();
                }
            }

            // ── Path Painting ─────────────────────────────────────────
            Some("S") => self.stroke(),
            Some("s") => { self.path_builder.close(); self.stroke(); }
            Some("f") | Some("F") => self.fill(tiny_skia::FillRule::Winding),
            Some("f*") => self.fill(tiny_skia::FillRule::EvenOdd),
            Some("B") => { self.fill(tiny_skia::FillRule::Winding); self.stroke(); }
            Some("B*") => { self.fill(tiny_skia::FillRule::EvenOdd); self.stroke(); }
            Some("b") => { self.path_builder.close(); self.fill(tiny_skia::FillRule::Winding); self.stroke(); }
            Some("b*") => { self.path_builder.close(); self.fill(tiny_skia::FillRule::EvenOdd); self.stroke(); }
            Some("n") => { self.path_builder = PathBuilder::new(); }

            // ── Clipping ──────────────────────────────────────────────
            Some("W") => { /* clipping not yet implemented */ }
            Some("W*") => { /* clipping not yet implemented */ }

            // ── Color Operators ───────────────────────────────────────
            // DeviceRGB (non-stroking / stroking)
            Some("rg") => {
                if ops.len() >= 3 {
                    if let Some(c) = Color::from_rgba(num_f32(&ops[0]), num_f32(&ops[1]), num_f32(&ops[2]), 1.0) {
                        self.gs.fill_color = c;
                    }
                }
            }
            Some("RG") => {
                if ops.len() >= 3 {
                    if let Some(c) = Color::from_rgba(num_f32(&ops[0]), num_f32(&ops[1]), num_f32(&ops[2]), 1.0) {
                        self.gs.stroke_color = c;
                    }
                }
            }
            // DeviceCMYK (non-stroking / stroking) — approximate to RGB
            Some("k") => {
                if ops.len() >= 4 {
                    let c = num_f32(&ops[0]);
                    let m = num_f32(&ops[1]);
                    let y = num_f32(&ops[2]);
                    let k = num_f32(&ops[3]);
                    let r = 1.0 - (c + k).min(1.0);
                    let g = 1.0 - (m + k).min(1.0);
                    let b = 1.0 - (y + k).min(1.0);
                    if let Some(col) = Color::from_rgba(r.max(0.0), g.max(0.0), b.max(0.0), 1.0) {
                        self.gs.fill_color = col;
                    }
                }
            }
            Some("K") => {
                if ops.len() >= 4 {
                    let c = num_f32(&ops[0]); let m = num_f32(&ops[1]);
                    let y = num_f32(&ops[2]); let k = num_f32(&ops[3]);
                    let r = 1.0 - (c + k).min(1.0);
                    let g = 1.0 - (m + k).min(1.0);
                    let b = 1.0 - (y + k).min(1.0);
                    if let Some(col) = Color::from_rgba(r.max(0.0), g.max(0.0), b.max(0.0), 1.0) {
                        self.gs.stroke_color = col;
                    }
                }
            }
            // DeviceGray (non-stroking / stroking)
            Some("g") => {
                if let Some(g) = ops.get(0).map(|o| num_f32(o)) {
                    if let Some(c) = Color::from_rgba(g, g, g, 1.0) {
                        self.gs.fill_color = c;
                    }
                }
            }
            Some("G") => {
                if let Some(g) = ops.get(0).map(|o| num_f32(o)) {
                    if let Some(c) = Color::from_rgba(g, g, g, 1.0) {
                        self.gs.stroke_color = c;
                    }
                }
            }
            // Uncolored (uncalibrated) with /CS — convert via current color space
            Some("sc") | Some("SC") => { /* color space specific — stub */ }
            Some("scn") | Some("SCN") => { /* extended color names — stub */ }

            // ── Graphics State Params ─────────────────────────────────
            Some("w") => {
                if let Some(w) = ops.get(0).map(|o| num_f32(o)) {
                    self.gs.line_width = w;
                }
            }
            Some("J") => { /* line cap — stub */ }
            Some("j") => { /* line join — stub */ }
            Some("d") => { /* dash pattern — stub */ }
            Some("gs") => {
                // Extended graphics state — read from the Resources/ExtGState dict
                if let Some(name) = ops.get(0).and_then(|o| o.as_name()) {
                    self.apply_ext_gstate(name);
                }
            }
            Some("i") => { /* flatness — stub */ }

            // ── Transformation ─────────────────────────────────────────
            Some("cm") => {
                if ops.len() >= 6 {
                    let (a, b) = (num_f32(&ops[0]), num_f32(&ops[1]));
                    let (c, d) = (num_f32(&ops[2]), num_f32(&ops[3]));
                    let (e, f) = (num_f32(&ops[4]), num_f32(&ops[5]));
                    let matrix = Transform::from_row(a, b, c, d, e, f);
                    self.gs.ctm = self.gs.ctm.pre_concat(matrix);
                }
            }

            // ── XObject ───────────────────────────────────────────────
            Some("Do") => {
                if let Some(name) = ops.get(0).and_then(|o| o.as_name()) {
                    self.render_xobject(name);
                }
            }

            // ── Text Objects (stub) ────────────────────────────────────
            Some("BT") => { /* begin text */ }
            Some("ET") => { /* end text */ }
            Some("Tf") => { /* set font + size */ }
            Some("Tm") => { /* set text matrix */ }
            Some("Td") | Some("TD") => { /* move text position */ }
            Some("Tj") => { /* show text — not yet rendered */ }
            Some("TJ") => { /* show text with positioning — not yet rendered */ }
            Some("'") => { /* show text with spacing — not yet rendered */ }
            Some("\"") => { /* show text with spacing — not yet rendered */ }
            Some("T*)") => { /* move to next line */ }

            // ── Inline Images ──────────────────────────────────────────
            Some("BI") => { /* begin inline image — stub */ }
            Some("ID") => { /* inline image data — stub */ }
            Some("EI") => { /* end inline image — stub */ }

            _ => {}
        }
    }

    /// Apply a named extended graphics state dictionary (gs operator).
    fn apply_ext_gstate(&mut self, _name: &CosName) {
        // TODO: read from self.resources -> ExtGState -> _name
    }

    /// Render a named XObject (Do operator — image or form).
    fn render_xobject(&mut self, name: &CosName) {
        let resources = match self.resources {
            Some(ref r) => r,
            None => return,
        };

        // Get the XObject dict via Resources API
        let xobject_dict = resources.xobject_dict().cloned();

        let xobject_entry = match xobject_dict {
            Some(ref d) => d.get(name),
            None => return,
        };

        match xobject_entry {
            Some(CosObject::Stream(stream)) => {
                let subtype = stream
                    .dictionary
                    .get(&CosName::new(b"Subtype".to_vec()))
                    .and_then(|o| o.as_name())
                    .map(|n| n.as_bytes());

                match subtype {
                    Some(b"Image") => self.render_image_xobject(stream),
                    Some(b"Form") => self.render_form_xobject(stream),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Render an Image XObject to the canvas.
    fn render_image_xobject(&mut self, stream: &crate::cos::CosStream) {
        let dict = &stream.dictionary;
        let width = dict.get(&CosName::new(b"Width".to_vec()))
            .and_then(|o| o.as_integer()).unwrap_or(0) as u32;
        let height = dict.get(&CosName::new(b"Height".to_vec()))
            .and_then(|o| o.as_integer()).unwrap_or(0) as u32;
        if width == 0 || height == 0 { return; }

        // We decode to RGBA via image crate for display.
        // Simple case: raw RGB / grayscale data (no complex filters).
        // For production PDFs, use io::decode_stream instead.
        if let Ok(img) = raw_to_image(&stream.data, width, height, dict) {
            // Convert the image into a tiny-skia Pixmap and draw at (0,0)
            // under current transformation
            if let Some(pixmap) = tiny_skia::Pixmap::from_vec(
                img.to_vec(),
                tiny_skia::IntSize::from_wh(width, height).unwrap(),
            ) {
                let mut paint = tiny_skia::PixmapPaint::default();
                self.pixmap.draw_pixmap(
                    0, 0,
                    pixmap.as_ref(),
                    &paint,
                    self.gs.ctm,
                    None,
                );
            }
        }
    }

    /// Render a Form XObject — execute its content stream in a sub-context.
    fn render_form_xobject(&mut self, stream: &crate::cos::CosStream) {
        let dict = &stream.dictionary;

        // Save current graphics state (push)
        self.gs_stack.push(self.gs.clone());

        // Apply Form's /Matrix if present
        if let Some(CosObject::Array(m)) = dict.get(&CosName::new(b"Matrix".to_vec())) {
            if m.len() == 6 {
                let vals: Vec<f32> = m.iter().map(|o| num_f32(o)).collect();
                let form_matrix = Transform::from_row(vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]);
                self.gs.ctm = self.gs.ctm.pre_concat(form_matrix);
            }
        }

        // Parse and execute the form's own content stream
        let instructions = parse_content_stream(&stream.data);
        if let Ok(insts) = instructions {
            for inst in &insts {
                self.execute_instruction(inst);
            }
        }

        // Restore graphics state (pop)
        if let Some(saved) = self.gs_stack.pop() {
            self.gs = saved;
        }
    }

    pub fn fill(&mut self, rule: tiny_skia::FillRule) {
        let path = self.path_builder.clone().finish();
        if let Some(p) = path {
            let mut paint = Paint::default();
            paint.set_color(self.gs.fill_color);
            self.pixmap.fill_path(&p, &paint, rule, self.gs.ctm, None);
        }
        self.path_builder = PathBuilder::new();
    }

    pub fn stroke(&mut self) {
        let path = self.path_builder.clone().finish();
        if let Some(p) = path {
            let mut paint = Paint::default();
            paint.set_color(self.gs.stroke_color);
            let mut stroke = Stroke::default();
            stroke.width = self.gs.line_width;
            self.pixmap.stroke_path(&p, &paint, &stroke, self.gs.ctm, None);
        }
        self.path_builder = PathBuilder::new();
    }
}

/// Convert a CosObject operand to f32 regardless of whether it's Integer or Real.
fn num_f32(obj: &CosObject) -> f32 {
    match obj {
        CosObject::Integer(i) => *i as f32,
        CosObject::Real(r) => *r as f32,
        _ => 0.0,
    }
}

/// Convert raw pixel data to RGBA bytes based on the stream's color space.
fn raw_to_image(data: &[u8], width: u32, height: u32, dict: &crate::cos::CosDictionary) -> Result<Vec<u8>, String> {
    let color_space = dict.get(&CosName::new(b"ColorSpace".to_vec()))
        .and_then(|o| o.as_name())
        .map(|n| n.as_bytes());

    let bpc = dict.get(&CosName::new(b"BitsPerComponent".to_vec()))
        .and_then(|o| o.as_integer()).unwrap_or(8);

    let (samples_per_pixel, channels) = match color_space {
        Some(b"DeviceGray") => (1, 1),
        Some(b"DeviceRGB")  => (3, 3),
        Some(b"DeviceCMYK") => (4, 4),
        _ => {
            // Default to grayscale or RGB based on data length
            if data.len() >= (width * height * 3) as usize { (3, 3) } else { (1, 1) }
        }
    };

    let expected = (width * height * samples_per_pixel * (bpc as u32 / 8)) as usize;
    if data.len() < expected {
        return Err(format!("Insufficient image data: {} < {}", data.len(), expected));
    }

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    match channels {
        1 => {
            // Grayscale -> RGB
            for pixel in data.chunks(samples_per_pixel as usize) {
                let gray = pixel[0];
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
        }
        3 => {
            // RGB
            for pixel in data.chunks(3) {
                if pixel.len() >= 3 {
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
            }
        }
        4 => {
            // CMYK -> RGB (naive)
            for pixel in data.chunks(4) {
                if pixel.len() >= 4 {
                    let c = pixel[0] as f32 / 255.0;
                    let m = pixel[1] as f32 / 255.0;
                    let y = pixel[2] as f32 / 255.0;
                    let k = pixel[3] as f32 / 255.0;
                    let r = (255.0 * (1.0 - c) * (1.0 - k)) as u8;
                    let g = (255.0 * (1.0 - m) * (1.0 - k)) as u8;
                    let b = (255.0 * (1.0 - y) * (1.0 - k)) as u8;
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
            }
        }
        _ => {}
    }

    Ok(rgba)
}
