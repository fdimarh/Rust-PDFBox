//! Bridging PDF Content Stream operators to Tiny-Skia Canvas.

use crate::content::{parse_content_stream, Instruction};
use crate::cos::{CosDictionary, CosName, CosObject};
use crate::pdmodel::page::Page;
use crate::PdfResult;
use std::collections::HashMap;
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};
use ab_glyph::Font as AbFont;

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

/// Per-font glyph metrics parsed from the PDF font dict.
#[derive(Debug, Clone)]
struct FontMetrics {
    /// Base font name (PostScript)
    base_font: String,
    /// First character code with a width
    first_char: u8,
    /// Last character code with a width
    last_char: u8,
    /// Per-code widths in 1/1000 text space (PDF glyph units)
    widths: Vec<f64>,
    /// Default width for codes outside the range
    missing_width: f64,
    /// Ascent from font descriptor (in glyph units)
    ascent: f64,
    /// Descent from font descriptor (in glyph units)
    descent: f64,
    /// Cap height from font descriptor
    cap_height: f64,
    /// Font bounding box width
    _bbox_width: f64,
    /// Embedded font program bytes (TrueType / OpenType) for ab_glyph rendering
    font_data: Option<Vec<u8>>,
    /// Scaled ab_glyph font for glyph outline rendering
    ab_font: Option<ab_glyph::FontArc>,
}

impl FontMetrics {
    fn from_dict(name: &[u8], dict: &CosDictionary) -> Option<Self> {
        let base_font = dict
            .get_name(&CosName::new(b"BaseFont".to_vec()))
            .map(|n| String::from_utf8_lossy(n.as_bytes()).to_string())
            .unwrap_or_else(|| String::from_utf8_lossy(name).to_string());

        let first_char = dict.get_int(&CosName::new(b"FirstChar".to_vec())).unwrap_or(0) as u8;
        let last_char = dict.get_int(&CosName::new(b"LastChar".to_vec())).unwrap_or(0) as u8;

        let widths: Vec<f64> = dict
            .get_array(&CosName::new(b"Widths".to_vec()))
            .map(|arr| arr.iter().filter_map(|v| v.as_number()).collect())
            .unwrap_or_default();

        let mut missing_width = 0.0;
        let mut ascent = 800.0;
        let mut descent = -200.0;
        let mut cap_height = 700.0;
        let mut bbox_width = 1000.0;
        let mut font_data: Option<Vec<u8>> = None;
        let mut ab_font: Option<ab_glyph::FontArc> = None;

        // Parse font descriptor for metrics and embedded font program
        if let Some(desc_dict) = dict
            .get(&CosName::new(b"FontDescriptor".to_vec()))
            .and_then(|v| match v {
                CosObject::Dictionary(d) => Some(d),
                CosObject::Reference(_) => None, // Can't resolve without store
                _ => None,
            })
        {
            ascent = desc_dict.get_number(&CosName::new(b"Ascent".to_vec())).unwrap_or(ascent);
            descent = desc_dict.get_number(&CosName::new(b"Descent".to_vec())).unwrap_or(descent);
            cap_height = desc_dict.get_number(&CosName::new(b"CapHeight".to_vec())).unwrap_or(cap_height);
            missing_width = desc_dict.get_number(&CosName::new(b"MissingWidth".to_vec())).unwrap_or(0.0);

            if let Some(arr) = desc_dict.get_array(&CosName::new(b"FontBBox".to_vec())) {
                let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_number()).collect();
                if nums.len() >= 4 {
                    bbox_width = (nums[2] - nums[0]).abs();
                }
            }

            // Extract embedded font program from descriptor
            let ff2_name = CosName::new(b"FontFile2".to_vec());
            let ff3_name = CosName::new(b"FontFile3".to_vec());
            let ff_name = CosName::new(b"FontFile".to_vec());

            let font_program = desc_dict
                .get(&ff2_name)
                .and_then(|v| match v {
                    CosObject::Stream(s) => Some(s.data.clone()),
                    _ => None,
                })
                .or_else(|| {
                    desc_dict
                        .get(&ff3_name)
                        .and_then(|v| match v {
                            CosObject::Stream(s) => Some(s.data.clone()),
                            _ => None,
                        })
                })
                .or_else(|| {
                    desc_dict
                        .get(&ff_name)
                        .and_then(|v| match v {
                            CosObject::Stream(s) => Some(s.data.clone()),
                            _ => None,
                        })
                });

            if let Some(data) = font_program {
                font_data = Some(data.clone());
                ab_font = ab_glyph::FontArc::try_from_vec(data).ok();
            }
        } else {
            // Standard font defaults
            match base_font.as_str() {
                "Helvetica" | "Helvetica-Bold" | "Helvetica-Oblique" | "Helvetica-BoldOblique" => {
                    ascent = 718.0; descent = -207.0; cap_height = 718.0;
                    bbox_width = if base_font.contains("Bold") { 1000.0 } else { 1000.0 };
                }
                "Times-Roman" | "Times-Bold" | "Times-Italic" | "Times-BoldItalic" => {
                    ascent = 683.0; descent = -217.0; cap_height = 662.0; bbox_width = 1000.0;
                }
                "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => {
                    ascent = 629.0; descent = -157.0; cap_height = 562.0; bbox_width = 1000.0;
                }
                _ => {}
            }
        }

        Some(Self {
            base_font,
            first_char,
            last_char,
            widths,
            missing_width,
            ascent,
            descent,
            cap_height,
            _bbox_width: bbox_width,
            font_data,
            ab_font,
        })
    }

    /// Width for a character code, in 1/1000 text space units
    fn width_for_code(&self, code: u8) -> f64 {
        if code < self.first_char || code > self.last_char {
            return self.missing_width;
        }
        let idx = (code - self.first_char) as usize;
        self.widths.get(idx).copied().unwrap_or(self.missing_width)
    }
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
    /// Per-font metrics indexed by font resource name (e.g. "F1", "F2")
    font_metrics: HashMap<Vec<u8>, FontMetrics>,
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
            font_metrics: HashMap::new(),
        }
    }

    /// Iterates through the PDF content stream instructions and paints them to the canvas.
    pub fn paint_page(&mut self, page: &'a Page) -> PdfResult<()> {
        self.resources = page.resources();

        // Parse font metrics from the page's Font resources
        if let Some(ref resources) = self.resources {
            if let Some(font_dict) = resources.font_dict() {
                for (name, val) in font_dict.iter() {
                    let dict = match val {
                        CosObject::Dictionary(d) => Some(d.clone()),
                        _ => None,
                    };
                    if let Some(d) = dict {
                        if let Some(metrics) = FontMetrics::from_dict(name.as_bytes(), &d) {
                            self.font_metrics.insert(name.as_bytes().to_vec(), metrics);
                        }
                    }
                }
            }
        }

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

    /// Render text with glyph outline rendering via ab_glyph (when available).
    /// Falls back to proportional rectangles when no font program is embedded.
    fn render_text(&mut self, text: &[u8]) {
        if text.is_empty() { return; }

        let font_size = self.text.font_size;
        let hscale = self.text.horizontal_scale.max(0.001);
        let char_spacing = self.text.char_spacing;
        let word_spacing = self.text.word_spacing;

        // Look up font metrics from parsed resources
        let metrics = self.text.font_name.as_ref()
            .and_then(|name| self.font_metrics.get(name));

        // ── ab_glyph outline rendering path ──
        if let Some(m) = metrics {
            if let Some(ref ab_font) = m.ab_font {
                let units_per_em = ab_font.units_per_em().unwrap_or(1000.0);
                let scale = font_size / units_per_em;

                let mut total_advance = 0.0f32;

                for (i, &code) in text.iter().enumerate() {
                    let glyph_id = ab_glyph::GlyphId(code as u16);

                    if let Some(outline) = ab_font.outline(glyph_id) {
                        // Convert outline curves to tiny_skia path
                        let mut cp = PathBuilder::new();
                        for curve in &outline.curves {
                            match curve {
                                ab_glyph::OutlineCurve::Line(_from, to) => {
                                    cp.line_to(to.x, -to.y);
                                }
                                ab_glyph::OutlineCurve::Quad(fr, ctrl, to) => {
                                    let c0_x = fr.x + (2.0/3.0) * (ctrl.x - fr.x);
                                    let c0_y = fr.y + (2.0/3.0) * (ctrl.y - fr.y);
                                    let c1_x = to.x + (2.0/3.0) * (ctrl.x - to.x);
                                    let c1_y = to.y + (2.0/3.0) * (ctrl.y - to.y);
                                    cp.cubic_to(c0_x, -c0_y, c1_x, -c1_y, to.x, -to.y);
                                }
                                ab_glyph::OutlineCurve::Cubic(_from, c1, c2, to) => {
                                    cp.cubic_to(c1.x, -c1.y, c2.x, -c2.y, to.x, -to.y);
                                }
                            }
                        }

                        if let Some(path) = cp.finish() {
                            let mut paint = Paint::default();
                            paint.set_color(self.gs.fill_color);

                            let glyph_tm = self.gs.ctm.pre_concat(self.text.tm)
                                .pre_concat(Transform::from_row(
                                    scale * hscale, 0.0, 0.0, scale,
                                    total_advance, 0.0,
                                ));

                            self.pixmap.fill_path(
                                &path, &paint,
                                tiny_skia::FillRule::Winding,
                                glyph_tm, None,
                            );
                        }

                        total_advance += ab_font.h_advance_unscaled(glyph_id) * scale * hscale;
                    } else {
                        // Fallback to PDF metric
                        let gw = m.width_for_code(code) as f32;
                        total_advance += gw * font_size / 1000.0 * hscale;
                    }

                    // Spacing
                    if code == b' ' {
                        total_advance += word_spacing;
                    }
                    total_advance += char_spacing;
                }

                self.text.tm = self.text.tm.pre_concat(
                    Transform::from_row(1.0, 0.0, 0.0, 1.0, total_advance, 0.0)
                );
                return;
            }
        }

        // ── Fallback: rectangle-based rendering ──
        let (ascent, descent, _cap_height) = if let Some(m) = metrics {
            (m.ascent as f32, m.descent as f32, m.cap_height as f32)
        } else {
            (650.0, -200.0, 650.0)
        };

        let font_scale = font_size / 1000.0;
        let glyph_height = (ascent - descent) * font_scale;

        let final_tm = self.gs.ctm.pre_concat(self.text.tm)
            .pre_concat(Transform::from_row(font_size * hscale, 0.0, 0.0, font_size, 0.0, 0.0));

        let mut total_advance = 0.0f32;

        for (i, &code) in text.iter().enumerate() {
            let glyph_width = metrics
                .map(|m| m.width_for_code(code) as f32)
                .unwrap_or(font_size * 0.6 * hscale);

            let w = glyph_width * font_scale * hscale;
            let h = glyph_height;
            let y_offset = -descent * font_scale;

            let mut cp = PathBuilder::new();
            cp.move_to(total_advance, y_offset);
            cp.line_to(total_advance + w, y_offset);
            cp.line_to(total_advance + w, y_offset + h * 0.8);
            cp.line_to(total_advance, y_offset + h * 0.8);
            cp.close();
            if let Some(path) = cp.finish() {
                let mut paint = Paint::default();
                paint.set_color(self.gs.fill_color);
                self.pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, final_tm, None);
            }

            let advance = glyph_width * font_scale * hscale;
            total_advance += advance;

            if code == b' ' {
                total_advance += word_spacing * font_scale;
            }
            if i + 1 < text.len() {
                total_advance += char_spacing * font_scale;
            }
        }

        self.text.tm = self.text.tm.pre_concat(
            Transform::from_row(1.0, 0.0, 0.0, 1.0, total_advance, 0.0)
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject};

    #[test]
    fn test_font_metrics_from_dict() {
        let mut d = CosDictionary::new();
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(b"Helvetica".to_vec())));
        d.set(CosName::new(b"FirstChar".to_vec()), CosObject::Integer(32));
        d.set(CosName::new(b"LastChar".to_vec()), CosObject::Integer(122));
        let widths: Vec<CosObject> = (32u8..=122u8).map(|_| CosObject::Integer(600)).collect();
        d.set(CosName::new(b"Widths".to_vec()), CosObject::Array(widths));

        let metrics = FontMetrics::from_dict(b"F1", &d).unwrap();
        assert_eq!(metrics.base_font, "Helvetica");
        assert_eq!(metrics.first_char, 32);
        assert_eq!(metrics.last_char, 122);
        assert_eq!(metrics.width_for_code(32), 600.0);
        assert_eq!(metrics.width_for_code(0), 0.0); // below FirstChar
    }

    #[test]
    fn test_font_metrics_standard_font_defaults() {
        let mut d = CosDictionary::new();
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(b"Times-Roman".to_vec())));

        let metrics = FontMetrics::from_dict(b"F1", &d).unwrap();
        assert_eq!(metrics.base_font, "Times-Roman");
        assert_eq!(metrics.ascent, 683.0);
        assert_eq!(metrics.descent, -217.0);
    }

    #[test]
    fn test_font_metrics_with_descriptor() {
        let mut desc = CosDictionary::new();
        desc.set(CosName::new(b"Ascent".to_vec()), CosObject::Integer(900));
        desc.set(CosName::new(b"Descent".to_vec()), CosObject::Integer(-300));
        desc.set(CosName::new(b"MissingWidth".to_vec()), CosObject::Integer(500));

        let mut d = CosDictionary::new();
        d.set(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(b"CustomFont".to_vec())));
        d.set(CosName::new(b"FontDescriptor".to_vec()), CosObject::Dictionary(desc));

        let metrics = FontMetrics::from_dict(b"F1", &d).unwrap();
        assert_eq!(metrics.ascent, 900.0);
        assert_eq!(metrics.descent, -300.0);
        assert_eq!(metrics.missing_width, 500.0);
    }

    #[test]
    fn test_font_metrics_missing_dict() {
        let d = CosDictionary::new();
        assert!(FontMetrics::from_dict(b"F1", &d).is_some()); // still succeeds with defaults
    }

    #[test]
    fn test_num_f32_integer() {
        let obj = CosObject::Integer(42);
        assert_eq!(num_f32(&obj), 42.0);
    }

    #[test]
    fn test_num_f32_real() {
        let obj = CosObject::Real(3.14);
        assert_eq!(num_f32(&obj), 3.14);
    }

    #[test]
    fn test_num_f32_null_defaults_zero() {
        let obj = CosObject::Null;
        assert_eq!(num_f32(&obj), 0.0);
    }
}