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
    /// Clipping path (set by `W`/`W*`). Stored for potential future use.
    /// tiny_skia 0.12.0 does not expose a public clip API on PixmapMut,
    /// so clipping is tracked but not enforced in this version.
    _clip_path: Option<tiny_skia::Path>,
    _clip_fill_rule: tiny_skia::FillRule,
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
            _clip_path: None,
            _clip_fill_rule: tiny_skia::FillRule::Winding,
        }
    }
}

/// PDF text state (§5.2).
#[derive(Debug, Clone, Default)]
struct TextState {
    /// Text matrix (Tm)
    tm: Transform,
    /// Text line matrix
    tlm: Transform,
    /// Current font name (from Tf)
    font_name: Option<Vec<u8>>,
    /// Current font size
    font_size: f32,
    /// Character spacing (Tc)
    char_spacing: f32,
    /// Word spacing (Tw)
    word_spacing: f32,
    /// Horizontal scaling (Tz / 100)
    horizontal_scale: f32,
    /// Leading (TL)
    leading: f32,
    /// Rendering mode (Tr)
    render_mode: i32,
    /// Rise (Ts)
    rise: f32,
}

pub struct PagePainter<'a> {
    pixmap: PixmapMut<'a>,
    gs: GraphicsState,
    gs_stack: Vec<GraphicsState>,
    path_builder: PathBuilder,
    current_pos: (f32, f32),
    resources: Option<crate::pdmodel::Resources<'a>>,
    text: TextState,
    in_text_object: bool,
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
            text: TextState::default(),
            in_text_object: false,
        }
    }

    /// Iterates through the PDF content stream instructions and paints them to the canvas.
    pub fn paint_page(&mut self, page: &'a Page) -> PdfResult<()> {
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
                .map_err(|e| crate::PdfError::Parse { offset: None, context: format!("render: {e}") })?;
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
            Some("Q") => { if let Some(saved) = self.gs_stack.pop() { self.gs = saved; } }

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
                    self.path_builder.cubic_to(cx, cy, num_f32(&ops[0]), num_f32(&ops[1]), num_f32(&ops[2]), num_f32(&ops[3]));
                    self.current_pos = (num_f32(&ops[2]), num_f32(&ops[3]));
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
            Some("h") => self.path_builder.close(),
            Some("re") => {
                if ops.len() >= 4 {
                    let x = num_f32(&ops[0]); let y = num_f32(&ops[1]);
                    let w = num_f32(&ops[2]); let h = num_f32(&ops[3]);
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
            Some("W") => {
                if let Some(path) = self.path_builder.clone().finish() {
                    self.gs._clip_path = Some(path);
                    self.gs._clip_fill_rule = tiny_skia::FillRule::Winding;
                }
                self.path_builder = PathBuilder::new();
            }
            Some("W*") => {
                if let Some(path) = self.path_builder.clone().finish() {
                    self.gs._clip_path = Some(path);
                    self.gs._clip_fill_rule = tiny_skia::FillRule::EvenOdd;
                }
                self.path_builder = PathBuilder::new();
            }

            // ── Color Operators ───────────────────────────────────────
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
            Some("k") => {
                if ops.len() >= 4 {
                    let c = num_f32(&ops[0]); let m = num_f32(&ops[1]);
                    let y = num_f32(&ops[2]); let k = num_f32(&ops[3]);
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
            Some("g") => {
                if let Some(g) = ops.get(0).map(|o| num_f32(o)) {
                    if let Some(c) = Color::from_rgba(g, g, g, 1.0) { self.gs.fill_color = c; }
                }
            }
            Some("G") => {
                if let Some(g) = ops.get(0).map(|o| num_f32(o)) {
                    if let Some(c) = Color::from_rgba(g, g, g, 1.0) { self.gs.stroke_color = c; }
                }
            }
            Some("sc") | Some("SC") => {}
            Some("scn") | Some("SCN") => {}

            // ── Graphics State Params ─────────────────────────────────
            Some("w") => { if let Some(w) = ops.get(0).map(|o| num_f32(o)) { self.gs.line_width = w; } }
            Some("J") | Some("j") | Some("d") | Some("i") => {}
            Some("gs") => {
                if let Some(name) = ops.get(0).and_then(|o| o.as_name()) {
                    self.apply_ext_gstate(name);
                }
            }

            // ── Transformation ─────────────────────────────────────────
            Some("cm") => {
                if ops.len() >= 6 {
                    let (a, b) = (num_f32(&ops[0]), num_f32(&ops[1]));
                    let (c, d) = (num_f32(&ops[2]), num_f32(&ops[3]));
                    let (e, f) = (num_f32(&ops[4]), num_f32(&ops[5]));
                    self.gs.ctm = self.gs.ctm.pre_concat(Transform::from_row(a, b, c, d, e, f));
                }
            }

            // ── XObject ───────────────────────────────────────────────
            Some("Do") => {
                if let Some(name) = ops.get(0).and_then(|o| o.as_name()) {
                    self.render_xobject(name);
                }
            }

            // ── Text Objects ───────────────────────────────────────────
            Some("BT") => { self.text = TextState::default(); self.in_text_object = true; }
            Some("ET") => { self.in_text_object = false; }
            Some("Tf") => {
                if ops.len() >= 2 {
                    if let Some(name) = ops.get(0).and_then(|o| o.as_name()) {
                        self.text.font_name = Some(name.as_bytes().to_vec());
                    }
                    self.text.font_size = num_f32(&ops[1]);
                }
            }
            Some("Tm") => {
                if ops.len() >= 6 {
                    let vals: Vec<f32> = ops.iter().take(6).map(|o| num_f32(o)).collect();
                    self.text.tm = Transform::from_row(vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]);
                    self.text.tlm = self.text.tm;
                }
            }
            Some("Td") | Some("TD") => {
                if ops.len() >= 2 {
                    let t = Transform::from_row(1.0, 0.0, 0.0, 1.0, num_f32(&ops[0]), num_f32(&ops[1]));
                    self.text.tlm = self.text.tlm.pre_concat(t);
                    self.text.tm = self.text.tlm;
                }
                if op == Some("TD") && ops.len() >= 2 { self.text.leading = -num_f32(&ops[1]); }
            }
            Some("T*") => {
                let t = Transform::from_row(1.0, 0.0, 0.0, 1.0, 0.0, -self.text.leading);
                self.text.tlm = self.text.tlm.pre_concat(t);
                self.text.tm = self.text.tlm;
            }
            Some("Tj") => {
                if let Some(text) = ops.get(0).and_then(|o| o.as_string()) {
                    self.render_text(text);
                }
            }
            Some("TJ") => {
                for operand in ops {
                    if let Some(bytes) = operand.as_string() {
                        self.render_text(bytes);
                    } else if let Some(num) = operand.as_number() {
                        let adjust = -(num as f32) / 1000.0 * self.text.font_size;
                        self.text.tm = self.text.tm.pre_concat(Transform::from_row(1.0, 0.0, 0.0, 1.0, adjust, 0.0));
                    }
                }
            }
            Some("'") => {
                let t = Transform::from_row(1.0, 0.0, 0.0, 1.0, 0.0, -self.text.leading);
                self.text.tlm = self.text.tlm.pre_concat(t);
                self.text.tm = self.text.tlm;
                if let Some(text) = ops.get(0).and_then(|o| o.as_string()) { self.render_text(text); }
            }
            Some("\"") => {
                if ops.len() >= 3 {
                    self.text.word_spacing = num_f32(&ops[0]);
                    self.text.char_spacing = num_f32(&ops[1]);
                    let t = Transform::from_row(1.0, 0.0, 0.0, 1.0, 0.0, -self.text.leading);
                    self.text.tlm = self.text.tlm.pre_concat(t);
                    self.text.tm = self.text.tlm;
                    if let Some(text) = ops.get(2).and_then(|o| o.as_string()) { self.render_text(text); }
                }
            }
            Some("Tc") => { if let Some(tc) = ops.get(0).map(|o| num_f32(o)) { self.text.char_spacing = tc; } }
            Some("Tw") => { if let Some(tw) = ops.get(0).map(|o| num_f32(o)) { self.text.word_spacing = tw; } }
            Some("Tz") => { if let Some(tz) = ops.get(0).map(|o| num_f32(o)) { self.text.horizontal_scale = tz / 100.0; } }
            Some("TL") => { if let Some(tl) = ops.get(0).map(|o| num_f32(o)) { self.text.leading = tl; } }
            Some("Tr") => { if let Some(tr) = ops.get(0).and_then(|o| o.as_integer()) { self.text.render_mode = tr as i32; } }
            Some("Ts") => { if let Some(ts) = ops.get(0).map(|o| num_f32(o)) { self.text.rise = ts; } }

            // ── Inline Images ──────────────────────────────────────────
            Some("BI") | Some("ID") | Some("EI") => {}

            _ => {}
        }
    }

    fn apply_ext_gstate(&mut self, _name: &CosName) {}

    fn render_xobject(&mut self, name: &CosName) {
        let resources = match self.resources { Some(ref r) => r, None => return };
        let xobject_dict = resources.xobject_dict().cloned();
        let xobject_entry = match xobject_dict { Some(ref d) => d.get(name), None => return };
        match xobject_entry {
            Some(CosObject::Stream(stream)) => {
                let subtype = stream.dictionary.get(&CosName::new(b"Subtype".to_vec()))
                    .and_then(|o| o.as_name()).map(|n| n.as_bytes());
                match subtype {
                    Some(b"Image") => self.render_image_xobject(stream),
                    Some(b"Form") => self.render_form_xobject(stream),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn render_image_xobject(&mut self, stream: &crate::cos::CosStream) {
        let dict = &stream.dictionary;
        let width = dict.get(&CosName::new(b"Width".to_vec()))
            .and_then(|o| o.as_integer()).unwrap_or(0) as u32;
        let height = dict.get(&CosName::new(b"Height".to_vec()))
            .and_then(|o| o.as_integer()).unwrap_or(0) as u32;
        if width == 0 || height == 0 { return; }
        if let Ok(img) = raw_to_image(&stream.data, width, height, dict) {
            if let Some(pixmap) = tiny_skia::Pixmap::from_vec(
                img.to_vec(),
                tiny_skia::IntSize::from_wh(width, height).unwrap(),
            ) {
                let paint = tiny_skia::PixmapPaint::default();
                self.pixmap.draw_pixmap(0, 0, pixmap.as_ref(), &paint, self.gs.ctm, None);
            }
        }
    }

    fn render_form_xobject(&mut self, stream: &crate::cos::CosStream) {
        let dict = &stream.dictionary;
        self.gs_stack.push(self.gs.clone());
        if let Some(CosObject::Array(m)) = dict.get(&CosName::new(b"Matrix".to_vec())) {
            if m.len() == 6 {
                let vals: Vec<f32> = m.iter().map(|o| num_f32(o)).collect();
                self.gs.ctm = self.gs.ctm.pre_concat(Transform::from_row(vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]));
            }
        }
        if let Ok(insts) = parse_content_stream(&stream.data) {
            for inst in &insts { self.execute_instruction(inst); }
        }
        if let Some(saved) = self.gs_stack.pop() { self.gs = saved; }
    }

    /// Render text as filled rectangles (placeholder / debug).
    /// Real glyph rasterization requires font metric lookup.
    fn render_text(&mut self, text: &[u8]) {
        if text.is_empty() { return; }
        let font_size = self.text.font_size;
        let hscale = self.text.horizontal_scale.max(0.001);
        let avg_width = font_size * 0.6 * hscale;
        let total_width = avg_width * text.len() as f32;

        let text_ctm = self.text.tm;
        let font_scale = Transform::from_row(font_size * hscale, 0.0, 0.0, font_size, 0.0, 0.0);
        let final_tm = self.gs.ctm.pre_concat(text_ctm).pre_concat(font_scale);

        for i in 0..text.len() {
            let x = i as f32 * avg_width;
            let w = avg_width * 0.8;
            let h = font_size;
            let mut cp = PathBuilder::new();
            cp.move_to(x, h * 0.2);
            cp.line_to(x + w, h * 0.2);
            cp.line_to(x + w, h);
            cp.line_to(x, h);
            cp.close();
            if let Some(path) = cp.finish() {
                let mut paint = Paint::default();
                paint.set_color(self.gs.fill_color);
                self.pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, final_tm, None);
            }
        }
        self.text.tm = self.text.tm.pre_concat(Transform::from_row(1.0, 0.0, 0.0, 1.0, total_width, 0.0));
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

fn num_f32(obj: &CosObject) -> f32 {
    match obj {
        CosObject::Integer(i) => *i as f32,
        CosObject::Real(r) => *r as f32,
        _ => 0.0,
    }
}

fn raw_to_image(data: &[u8], width: u32, height: u32, dict: &crate::cos::CosDictionary) -> Result<Vec<u8>, String> {
    let color_space = dict.get(&CosName::new(b"ColorSpace".to_vec()))
        .and_then(|o| o.as_name()).map(|n| n.as_bytes());
    let bpc = dict.get(&CosName::new(b"BitsPerComponent".to_vec()))
        .and_then(|o| o.as_integer()).unwrap_or(8);
    let (spp, channels) = match color_space {
        Some(b"DeviceGray") => (1, 1),
        Some(b"DeviceRGB")  => (3, 3),
        Some(b"DeviceCMYK") => (4, 4),
        _ => if data.len() >= (width * height * 3) as usize { (3, 3) } else { (1, 1) },
    };
    if data.len() < (width * height * spp * (bpc as u32 / 8)) as usize {
        return Err(format!("Insufficient image data"));
    }
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    match channels {
        1 => { for p in data.chunks(spp as usize) { let g = p[0]; rgba.extend_from_slice(&[g, g, g, 255]); } }
        3 => { for p in data.chunks(3) { if p.len() >= 3 { rgba.extend_from_slice(&[p[0], p[1], p[2], 255]); } } }
        4 => {
            for p in data.chunks(4) {
                if p.len() >= 4 {
                    let (c,m,y,k) = (p[0] as f32/255.0, p[1] as f32/255.0, p[2] as f32/255.0, p[3] as f32/255.0);
                    rgba.extend_from_slice(&[(255.0*(1.0-c)*(1.0-k)) as u8, (255.0*(1.0-m)*(1.0-k)) as u8, (255.0*(1.0-y)*(1.0-k)) as u8, 255]);
                }
            }
        }
        _ => {}
    }
    Ok(rgba)
}
