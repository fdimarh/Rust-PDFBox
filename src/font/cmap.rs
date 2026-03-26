//! ToUnicode CMap parser — decodes PDF ToUnicode CMaps into character maps.
//!
//! Maps to Java PDFBox `ToUnicodeWriter` / `CMapParser`.
//! Parses `bfchar` and `bfrange` sections to build a code→Unicode mapping.

use std::collections::HashMap;

/// A parsed ToUnicode CMap mapping encoded codes to Unicode strings.
#[derive(Debug, Default, Clone)]
pub struct ToUnicodeCMap {
    mappings: HashMap<u32, String>,
    ranges: Vec<BfRange>,
}

#[derive(Debug, Clone)]
struct BfRange { start: u32, end: u32, base: u32 }

impl ToUnicodeCMap {
    /// Look up a code using bfchar then bfrange mappings.
    pub fn to_unicode(&self, code: u32) -> Option<String> {
        if let Some(s) = self.mappings.get(&code) { return Some(s.clone()); }
        for r in &self.ranges {
            if code >= r.start && code <= r.end {
                let cp = r.base + (code - r.start);
                return char::from_u32(cp).map(|c| c.to_string());
            }
        }
        None
    }
    pub fn mapping_count(&self) -> usize { self.mappings.len() }
    pub fn range_count(&self) -> usize { self.ranges.len() }
}

/// Parse a ToUnicode CMap from raw bytes.
pub fn parse_to_unicode_cmap(data: &[u8]) -> ToUnicodeCMap {
    let mut cmap = ToUnicodeCMap::default();
    let text = String::from_utf8_lossy(data);
    let tokens = tokenize_cmap(&text);
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "beginbfchar" => {
                i += 1;
                while i < tokens.len() && tokens[i] != "endbfchar" {
                    if i + 1 < tokens.len() {
                        if let (Some(src), Some(dst)) = (parse_hex_token(&tokens[i]), parse_hex_to_unicode(&tokens[i+1])) {
                            cmap.mappings.insert(src, dst);
                        }
                        i += 2;
                    } else { break; }
                }
            }
            "beginbfrange" => {
                i += 1;
                while i < tokens.len() && tokens[i] != "endbfrange" {
                    if i + 2 < tokens.len() {
                        let start = parse_hex_token(&tokens[i]);
                        let end   = parse_hex_token(&tokens[i+1]);
                        let base_tok = tokens[i+2].clone();
                        if let (Some(s), Some(e)) = (start, end) {
                            if base_tok.starts_with('[') {
                                for (off, hex) in parse_array_token(&base_tok).iter().enumerate() {
                                    let code = s + off as u32;
                                    if code > e { break; }
                                    if let Some(u) = parse_hex_to_unicode(hex) { cmap.mappings.insert(code, u); }
                                }
                            } else if let Some(base) = parse_hex_token(&base_tok) {
                                cmap.ranges.push(BfRange { start: s, end: e, base });
                            }
                        }
                        i += 3;
                    } else { break; }
                }
            }
            _ => { i += 1; }
        }
    }
    cmap
}

fn tokenize_cmap(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => { chars.next(); }
            '%' => { chars.next(); while chars.peek().map(|&c| c != '\n').unwrap_or(false) { chars.next(); } }
            '<' => {
                let mut tok = String::from("<"); chars.next();
                while let Some(&c) = chars.peek() { chars.next(); if c == '>' { break; } tok.push(c); }
                tok.push('>'); tokens.push(tok);
            }
            '[' => {
                let mut tok = String::new(); let mut depth = 0i32;
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '[' { depth += 1; } if c == ']' { depth -= 1; }
                    tok.push(c); if depth == 0 { break; }
                }
                tokens.push(tok);
            }
            '(' => {
                let mut tok = String::from("("); chars.next(); let mut depth = 1i32;
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '(' { depth += 1; }
                    if c == ')' { depth -= 1; if depth == 0 { tok.push(')'); break; } }
                    tok.push(c);
                }
                tokens.push(tok);
            }
            _ => {
                let mut tok = String::new();
                while let Some(&c) = chars.peek() {
                    if " \t\n\r<>[]()'%".contains(c) { break; }
                    tok.push(c); chars.next();
                }
                if !tok.is_empty() { tokens.push(tok); }
            }
        }
    }
    tokens
}

fn parse_hex_token(tok: &str) -> Option<u32> {
    let inner = tok.strip_prefix('<')?.strip_suffix('>')?;
    if inner.is_empty() { return None; }
    u32::from_str_radix(inner, 16).ok()
}

fn parse_hex_to_unicode(tok: &str) -> Option<String> {
    let inner = tok.strip_prefix('<')?.strip_suffix('>')?;
    if inner.is_empty() { return None; }
    let bytes = hex_str_to_bytes(inner)?;
    match bytes.len() {
        1 => char::from_u32(bytes[0] as u32).map(|c| c.to_string()),
        2 => char::from_u32(u16::from_be_bytes([bytes[0], bytes[1]]) as u32).map(|c| c.to_string()),
        4 => {
            let u0 = u16::from_be_bytes([bytes[0], bytes[1]]);
            let u1 = u16::from_be_bytes([bytes[2], bytes[3]]);
            if (0xD800..=0xDBFF).contains(&u0) && (0xDC00..=0xDFFF).contains(&u1) {
                let cp = 0x10000u32 + ((u0 as u32 - 0xD800) << 10) + (u1 as u32 - 0xDC00);
                char::from_u32(cp).map(|c| c.to_string())
            } else {
                Some(format!("{}{}", char::from_u32(u0 as u32)?, char::from_u32(u1 as u32)?))
            }
        }
        _ => {
            let mut s = String::new(); let mut i = 0;
            while i + 1 < bytes.len() {
                if let Some(c) = char::from_u32(u16::from_be_bytes([bytes[i], bytes[i+1]]) as u32) { s.push(c); }
                i += 2;
            }
            if s.is_empty() { None } else { Some(s) }
        }
    }
}

fn parse_array_token(tok: &str) -> Vec<String> {
    let inner = tok.strip_prefix('[').unwrap_or(tok).strip_suffix(']').unwrap_or(tok);
    tokenize_cmap(inner).into_iter().filter(|t| t.starts_with('<') && t.ends_with('>')).collect()
}

fn hex_str_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let padded = if hex.len() % 2 != 0 { format!("0{hex}") } else { hex.to_string() };
    (0..padded.len()).step_by(2).map(|i| u8::from_str_radix(&padded[i..i+2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(body: &str) -> String {
        format!("begincmap\n1 begincodespacerange\n<00><FF>\nendcodespacerange\n{body}endcmap\n")
    }

    #[test]
    fn parse_empty_cmap() {
        let c = parse_to_unicode_cmap(b"begincmap\nendcmap\n");
        assert_eq!(c.mapping_count(), 0);
        assert_eq!(c.range_count(), 0);
    }
    #[test]
    fn parse_bfchar_single() {
        let c = parse_to_unicode_cmap(wrap("1 beginbfchar\n<41> <0041>\nendbfchar\n").as_bytes());
        assert_eq!(c.to_unicode(0x41), Some("A".to_string()));
    }
    #[test]
    fn parse_bfchar_multiple() {
        let c = parse_to_unicode_cmap(wrap("3 beginbfchar\n<41><0041>\n<42><0042>\n<43><0043>\nendbfchar\n").as_bytes());
        assert_eq!(c.to_unicode(0x41), Some("A".to_string()));
        assert_eq!(c.to_unicode(0x43), Some("C".to_string()));
        assert_eq!(c.mapping_count(), 3);
    }
    #[test]
    fn parse_bfrange_sequential() {
        let c = parse_to_unicode_cmap(wrap("1 beginbfrange\n<30><39><0030>\nendbfrange\n").as_bytes());
        assert_eq!(c.to_unicode(0x30), Some("0".to_string()));
        assert_eq!(c.to_unicode(0x39), Some("9".to_string()));
        assert_eq!(c.range_count(), 1);
    }
    #[test]
    fn parse_bfrange_array_form() {
        let c = parse_to_unicode_cmap(wrap("1 beginbfrange\n<01><03>[<0041><0042><0043>]\nendbfrange\n").as_bytes());
        assert_eq!(c.to_unicode(0x01), Some("A".to_string()));
        assert_eq!(c.to_unicode(0x02), Some("B".to_string()));
        assert_eq!(c.to_unicode(0x03), Some("C".to_string()));
    }
    #[test]
    fn unknown_code_returns_none() {
        assert_eq!(parse_to_unicode_cmap(b"begincmap\nendcmap\n").to_unicode(0xFF), None);
    }
    #[test]
    fn two_byte_source_code() {
        let c = parse_to_unicode_cmap(wrap("1 beginbfchar\n<0041><0041>\nendbfchar\n").as_bytes());
        assert_eq!(c.to_unicode(0x0041), Some("A".to_string()));
    }
    #[test]
    fn comments_are_skipped() {
        let c = parse_to_unicode_cmap(wrap("% comment\n1 beginbfchar\n<41><0041> % A\nendbfchar\n").as_bytes());
        assert_eq!(c.to_unicode(0x41), Some("A".to_string()));
    }
    #[test]
    fn bfrange_out_of_range_returns_none() {
        let c = parse_to_unicode_cmap(wrap("1 beginbfrange\n<30><39><0030>\nendbfrange\n").as_bytes());
        assert_eq!(c.to_unicode(0x40), None);
    }
    #[test]
    fn hex_bytes_odd_padded() {
        assert_eq!(hex_str_to_bytes("F"), Some(vec![0x0F]));
    }
}

