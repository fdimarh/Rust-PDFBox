//! Text extraction pipeline — Phase 3 MVP.
//!
//! Maps to Java PDFBox `PDFTextStripper`.
//!
//! # How it works
//!
//! 1. Walk the page's content stream instructions (from `parse_content_stream`).
//! 2. Feed each instruction through `GraphicsState` to track text position.
//! 3. Decode text operands via an optional `ToUnicodeCMap`; fall back to
//!    Latin-1 if no CMap is available.
//! 4. Accumulate `TextChunk` values (text + position).
//! 5. Sort by Y descending then X ascending (reading order heuristic).
//! 6. Insert line breaks when Y changes by more than half the font size.
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type |
//! |---|---|
//! | `PDFTextStripper` | [`extract_text`] |
//! | `TextPosition` | [`TextChunk`] |

use crate::content::graphics_state::GraphicsState;
use crate::content::{parse_content_stream, Instruction};
use crate::cos::CosObject;
use crate::font::ToUnicodeCMap;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single extracted text run with position metadata.
#[derive(Debug, Clone)]
pub struct TextChunk {
    /// The decoded Unicode text.
    pub text: String,
    /// X position in user space.
    pub x: f64,
    /// Y position in user space.
    pub y: f64,
    /// Effective font size (scaled by CTM).
    pub font_size: f64,
}

// ---------------------------------------------------------------------------
// extract_text
// ---------------------------------------------------------------------------

/// Extract text from a PDF content stream byte slice.
///
/// Returns a plain `String` with line breaks inserted at Y-axis breaks.
/// Pass `cmap` if the font's ToUnicode CMap is known; otherwise bytes are
/// decoded as Latin-1.
///
/// # Arguments
///
/// * `stream_data` — raw bytes of the content stream
/// * `cmap` — optional ToUnicode CMap for the active font
pub fn extract_text(stream_data: &[u8], cmap: Option<&ToUnicodeCMap>) -> String {
    let chunks = extract_chunks(stream_data, cmap);
    chunks_to_string(&chunks)
}

/// Extract text chunks (with position) from a content stream.
pub fn extract_chunks(stream_data: &[u8], cmap: Option<&ToUnicodeCMap>) -> Vec<TextChunk> {
    let instructions = match parse_content_stream(stream_data) {
        Ok(instrs) => instrs,
        Err(_) => return Vec::new(),
    };

    let mut gs = GraphicsState::new();
    let mut chunks: Vec<TextChunk> = Vec::new();

    for instr in &instructions {
        process_instruction(&mut gs, instr, cmap, &mut chunks);
    }

    chunks
}

// ---------------------------------------------------------------------------
// Instruction processor
// ---------------------------------------------------------------------------

fn process_instruction(
    gs: &mut GraphicsState,
    instr: &Instruction,
    cmap: Option<&ToUnicodeCMap>,
    chunks: &mut Vec<TextChunk>,
) {
    let op = instr.operator.name.as_slice();

    match op {
        // Graphics state
        b"q"  => gs.save(),
        b"Q"  => gs.restore(),
        b"cm" => {
            if let Some([a, b, c, d, e, f]) = six_reals(&instr.operands) {
                gs.concat_matrix(a, b, c, d, e, f);
            }
        }

        // Text object
        b"BT" => gs.begin_text(),
        b"ET" => gs.end_text(),

        // Text state operators
        b"Tf" => {
            if instr.operands.len() >= 2 {
                let name = instr.operands[0].as_name().map(|n| {
                    String::from_utf8_lossy(n.as_bytes()).to_string()
                }).unwrap_or_default();
                let size = real_at(&instr.operands, 1).unwrap_or(0.0);
                gs.set_font(name, size);
            }
        }
        b"TL" => { if let Some(v) = real_at(&instr.operands, 0) { gs.set_leading(v); } }
        b"Tc" => { if let Some(v) = real_at(&instr.operands, 0) { gs.set_char_spacing(v); } }
        b"Tw" => { if let Some(v) = real_at(&instr.operands, 0) { gs.set_word_spacing(v); } }
        b"Tz" => { if let Some(v) = real_at(&instr.operands, 0) { gs.set_horizontal_scaling(v); } }
        b"Ts" => { if let Some(v) = real_at(&instr.operands, 0) { gs.set_text_rise(v); } }

        // Text position
        b"Tm" => {
            if let Some([a, b, c, d, e, f]) = six_reals(&instr.operands) {
                gs.set_text_matrix(a, b, c, d, e, f);
            }
        }
        b"Td" => {
            if let Some([tx, ty]) = two_reals(&instr.operands) {
                gs.move_text(tx, ty);
            }
        }
        b"TD" => {
            if let Some([tx, ty]) = two_reals(&instr.operands) {
                gs.move_text_set_leading(tx, ty);
            }
        }
        b"T*" => gs.next_line(),

        // Text showing operators
        b"Tj" => {
            if let Some(bytes) = string_bytes_at(&instr.operands, 0) {
                let text = decode_bytes(&bytes, cmap, &gs);
                if !text.is_empty() {
                    let (x, y) = gs.text_position();
                    let fs = gs.effective_font_size();
                    chunks.push(TextChunk { text, x, y, font_size: fs });
                }
            }
        }
        b"TJ" => {
            if let Some(arr) = array_at(&instr.operands, 0) {
                let mut text = String::new();
                for elem in arr {
                    match elem {
                        CosObject::String(b) => {
                            text.push_str(&decode_bytes(b, cmap, gs));
                        }
                        CosObject::Integer(n) => {
                            // Negative kern > threshold → insert space
                            if *n < -250 { text.push(' '); }
                        }
                        CosObject::Real(r) => {
                            if *r < -250.0 { text.push(' '); }
                        }
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    let (x, y) = gs.text_position();
                    let fs = gs.effective_font_size();
                    chunks.push(TextChunk { text, x, y, font_size: fs });
                }
            }
        }
        // Move-to-next-line-and-show-text
        b"'" => {
            gs.next_line();
            if let Some(bytes) = string_bytes_at(&instr.operands, 0) {
                let text = decode_bytes(&bytes, cmap, &gs);
                if !text.is_empty() {
                    let (x, y) = gs.text_position();
                    let fs = gs.effective_font_size();
                    chunks.push(TextChunk { text, x, y, font_size: fs });
                }
            }
        }
        // Set-spacing, move, show
        b"\"" => {
            if instr.operands.len() >= 3 {
                if let Some(aw) = real_at(&instr.operands, 0) { gs.set_word_spacing(aw); }
                if let Some(ac) = real_at(&instr.operands, 1) { gs.set_char_spacing(ac); }
                gs.next_line();
                if let Some(bytes) = string_bytes_at(&instr.operands, 2) {
                    let text = decode_bytes(&bytes, cmap, &gs);
                    if !text.is_empty() {
                        let (x, y) = gs.text_position();
                        let fs = gs.effective_font_size();
                        chunks.push(TextChunk { text, x, y, font_size: fs });
                    }
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Chunk → String
// ---------------------------------------------------------------------------

/// Convert sorted TextChunks to a plain string with line breaks.
pub fn chunks_to_string(chunks: &[TextChunk]) -> String {
    if chunks.is_empty() { return String::new(); }

    // Sort: Y descending (top of page first), then X ascending
    let mut sorted = chunks.to_vec();
    sorted.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut result = String::new();
    let mut prev_y = f64::MAX;
    let mut prev_x = 0.0_f64;

    for chunk in &sorted {
        let line_height = if chunk.font_size > 0.0 { chunk.font_size } else { 12.0 };
        let y_diff = (prev_y - chunk.y).abs();

        if prev_y == f64::MAX {
            // First chunk — no prefix
        } else if y_diff > line_height * 0.5 {
            // Different line
            result.push('\n');
        } else if chunk.x > prev_x + line_height * 0.3 {
            // Same line but gap large enough for a space
            result.push(' ');
        }

        result.push_str(&chunk.text);
        prev_y = chunk.y;
        prev_x = chunk.x + chunk.text.chars().count() as f64 * chunk.font_size * 0.6;
    }

    result
}

// ---------------------------------------------------------------------------
// Decoding helpers
// ---------------------------------------------------------------------------

/// Decode a byte string using the CMap if available, otherwise Latin-1.
fn decode_bytes(bytes: &[u8], cmap: Option<&ToUnicodeCMap>, _gs: &GraphicsState) -> String {
    if let Some(cmap) = cmap {
        decode_with_cmap(bytes, cmap)
    } else {
        // Heuristic: if font size is set and bytes look like single-byte codes, use Latin-1
        decode_latin1(bytes)
    }
}

fn decode_with_cmap(bytes: &[u8], cmap: &ToUnicodeCMap) -> String {
    let mut s = String::new();
    // Try 2-byte codes first (composite fonts), then 1-byte
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() {
            let code2 = u32::from_be_bytes([0, 0, bytes[i], bytes[i + 1]]);
            if let Some(u) = cmap.to_unicode(code2) {
                s.push_str(&u);
                i += 2;
                continue;
            }
        }
        let code1 = bytes[i] as u32;
        if let Some(u) = cmap.to_unicode(code1) {
            s.push_str(&u);
        } else {
            // Fallback: treat as Latin-1
            if let Some(c) = char::from_u32(code1) { s.push(c); }
        }
        i += 1;
    }
    s
}

fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter()
        .filter_map(|&b| char::from_u32(b as u32))
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\r')
        .collect()
}

// ---------------------------------------------------------------------------
// Operand extraction helpers
// ---------------------------------------------------------------------------

fn real_at(ops: &[CosObject], idx: usize) -> Option<f64> {
    ops.get(idx)?.as_number()
}

fn two_reals(ops: &[CosObject]) -> Option<[f64; 2]> {
    Some([real_at(ops, 0)?, real_at(ops, 1)?])
}

fn six_reals(ops: &[CosObject]) -> Option<[f64; 6]> {
    Some([
        real_at(ops, 0)?, real_at(ops, 1)?,
        real_at(ops, 2)?, real_at(ops, 3)?,
        real_at(ops, 4)?, real_at(ops, 5)?,
    ])
}

fn string_bytes_at(ops: &[CosObject], idx: usize) -> Option<Vec<u8>> {
    ops.get(idx)?.as_string().map(|b| b.to_vec())
}

fn array_at(ops: &[CosObject], idx: usize) -> Option<&[CosObject]> {
    ops.get(idx)?.as_array()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stream(ops: &str) -> Vec<u8> { ops.as_bytes().to_vec() }

    #[test]
    fn empty_stream_returns_empty_string() {
        assert_eq!(extract_text(b"", None), "");
    }

    #[test]
    fn bt_et_no_text_returns_empty() {
        assert_eq!(extract_text(b"BT ET", None), "");
    }

    #[test]
    fn tj_single_word() {
        let stream = make_stream("BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let text = extract_text(&stream, None);
        assert!(text.contains("Hello"), "got: {text:?}");
    }

    #[test]
    fn tj_multiple_words_same_line() {
        let stream = make_stream("BT /F1 12 Tf 0 700 Td (Hello) Tj 50 0 Td ( World) Tj ET");
        let text = extract_text(&stream, None);
        assert!(text.contains("Hello"), "got: {text:?}");
        assert!(text.contains("World"), "got: {text:?}");
    }

    #[test]
    fn td_moves_to_next_line() {
        let stream = make_stream("BT /F1 12 Tf 0 700 Td (Line1) Tj 0 -14 Td (Line2) Tj ET");
        let text = extract_text(&stream, None);
        assert!(text.contains("Line1"), "got: {text:?}");
        assert!(text.contains("Line2"), "got: {text:?}");
    }

    #[test]
    fn tstar_moves_to_next_line() {
        let stream = make_stream("BT /F1 12 Tf 14 TL 0 700 Td (First) Tj T* (Second) Tj ET");
        let text = extract_text(&stream, None);
        assert!(text.contains("First"), "got: {text:?}");
        assert!(text.contains("Second"), "got: {text:?}");
    }

    #[test]
    fn tj_with_cmap() {
        use crate::font::cmap::{parse_to_unicode_cmap};
        let cmap_data = b"begincmap\n1 beginbfchar\n<48><0048>\n<69><0069>\nendbfchar\nendcmap\n";
        let cmap = parse_to_unicode_cmap(cmap_data);
        let stream = make_stream("BT /F1 12 Tf 0 700 Td (Hi) Tj ET");
        let text = extract_text(&stream, Some(&cmap));
        assert!(text.contains("Hi"), "got: {text:?}");
    }

    #[test]
    fn tj_array_with_kerning() {
        // TJ array: negative kern between glyphs
        let stream = make_stream("BT /F1 12 Tf 0 700 Td [(He) -300 (llo)] TJ ET");
        let text = extract_text(&stream, None);
        // Large kern (-300 < -250) inserts a space
        assert!(text.contains("He"), "got: {text:?}");
        assert!(text.contains("llo"), "got: {text:?}");
    }

    #[test]
    fn save_restore_graphics_state() {
        // q/Q must not corrupt text extraction
        let stream = make_stream("q BT /F1 12 Tf 0 700 Td (Test) Tj ET Q");
        let text = extract_text(&stream, None);
        assert!(text.contains("Test"), "got: {text:?}");
    }

    #[test]
    fn chunks_to_string_single_chunk() {
        let chunks = vec![TextChunk { text: "Hello".to_string(), x: 0.0, y: 700.0, font_size: 12.0 }];
        assert_eq!(chunks_to_string(&chunks), "Hello");
    }

    #[test]
    fn chunks_to_string_two_lines() {
        let chunks = vec![
            TextChunk { text: "Line1".to_string(), x: 0.0, y: 700.0, font_size: 12.0 },
            TextChunk { text: "Line2".to_string(), x: 0.0, y: 680.0, font_size: 12.0 },
        ];
        let s = chunks_to_string(&chunks);
        assert!(s.contains('\n'), "expected newline, got: {s:?}");
        assert!(s.contains("Line1"));
        assert!(s.contains("Line2"));
    }

    #[test]
    fn decode_latin1_printable() {
        assert_eq!(decode_latin1(b"Hello"), "Hello");
    }

    #[test]
    fn decode_latin1_filters_control_chars() {
        // Control chars other than \n and \r are filtered
        let bytes = &[b'A', 0x01, b'B'];
        let s = decode_latin1(bytes);
        assert_eq!(s, "AB");
    }
}
