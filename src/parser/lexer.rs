//! PDF lexer — tokenizes raw PDF bytes into syntactic tokens.
//!
//! Maps to Java PDFBox `BaseParser` low-level token reading. The lexer
//! operates on a byte slice with an internal cursor and produces [`Token`]
//! values consumed by the parser.

use crate::cos::CosName;

/// A single lexical token from the PDF byte stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A boolean keyword: `true` or `false`.
    Bool(bool),
    /// An integer literal.
    Integer(i64),
    /// A floating-point literal.
    Real(f64),
    /// A literal string `(...)` — decoded bytes (escape sequences resolved).
    LiteralString(Vec<u8>),
    /// A hex string `<...>` — decoded bytes.
    HexString(Vec<u8>),
    /// A name object `/Something`.
    Name(CosName),
    /// A PDF keyword (`null`, `obj`, `endobj`, `stream`, `endstream`,
    /// `xref`, `trailer`, `startxref`, `R`, `f`, `n`).
    Keyword(Vec<u8>),
    /// `[`
    ArrayStart,
    /// `]`
    ArrayEnd,
    /// `<<`
    DictStart,
    /// `>>`
    DictEnd,
}

impl Token {
    /// Returns `true` if this token is the keyword matching `kw`.
    pub fn is_keyword(&self, kw: &[u8]) -> bool {
        matches!(self, Token::Keyword(k) if k == kw)
    }
}

/// Lexer state: wraps a byte slice with a position cursor.
pub struct Lexer<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer over the given byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Returns the current byte offset.
    #[inline]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Sets the position (used by the parser for seeking).
    #[inline]
    pub fn set_position(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Returns `true` when all input has been consumed.
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Peeks at the current byte without advancing.
    #[inline]
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// Reads and advances one byte.
    #[inline]
    fn advance(&mut self) -> Option<u8> {
        let b = self.data.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    /// Skips whitespace and comments.
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(b) if is_whitespace(b) => {
                    self.pos += 1;
                }
                Some(b'%') => {
                    // Skip comment until end-of-line.
                    self.pos += 1;
                    while let Some(b) = self.peek() {
                        self.pos += 1;
                        if b == b'\n' || b == b'\r' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    /// Reads the next token, or `None` at EOF.
    pub fn next_token(&mut self) -> Result<Option<Token>, LexError> {
        self.skip_whitespace_and_comments();

        let Some(b) = self.peek() else {
            return Ok(None);
        };

        match b {
            b'[' => {
                self.advance();
                Ok(Some(Token::ArrayStart))
            }
            b']' => {
                self.advance();
                Ok(Some(Token::ArrayEnd))
            }
            b'<' => {
                self.advance();
                if self.peek() == Some(b'<') {
                    self.advance();
                    Ok(Some(Token::DictStart))
                } else {
                    self.read_hex_string().map(|s| Some(Token::HexString(s)))
                }
            }
            b'>' => {
                self.advance();
                if self.peek() == Some(b'>') {
                    self.advance();
                    Ok(Some(Token::DictEnd))
                } else {
                    Err(LexError::unexpected(b'>', self.pos - 1))
                }
            }
            b'(' => {
                self.advance();
                self.read_literal_string()
                    .map(|s| Some(Token::LiteralString(s)))
            }
            b'/' => {
                self.advance();
                self.read_name().map(|n| Some(Token::Name(n)))
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.read_number().map(Some),
            _ if b.is_ascii_alphabetic() => self.read_keyword_or_bool().map(Some),
            other => Err(LexError::unexpected(other, self.pos)),
        }
    }

    // ---- Private readers ----

    fn read_hex_string(&mut self) -> Result<Vec<u8>, LexError> {
        let mut hex = Vec::new();
        loop {
            match self.advance() {
                Some(b'>') => break,
                Some(b) if is_whitespace(b) => continue,
                Some(b) if b.is_ascii_hexdigit() => hex.push(b),
                Some(b) => return Err(LexError::invalid_hex(b, self.pos - 1)),
                None => return Err(LexError::unterminated("hex string", self.pos)),
            }
        }
        // Pad odd-length hex strings with trailing 0 per spec.
        if hex.len() % 2 != 0 {
            hex.push(b'0');
        }
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for pair in hex.chunks(2) {
            let hi = hex_digit(pair[0]);
            let lo = hex_digit(pair[1]);
            bytes.push((hi << 4) | lo);
        }
        Ok(bytes)
    }

    fn read_literal_string(&mut self) -> Result<Vec<u8>, LexError> {
        let mut result = Vec::new();
        let mut depth: u32 = 1;
        loop {
            match self.advance() {
                None => return Err(LexError::unterminated("literal string", self.pos)),
                Some(b'(') => {
                    depth += 1;
                    result.push(b'(');
                }
                Some(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    result.push(b')');
                }
                Some(b'\\') => {
                    match self.advance() {
                        Some(b'n') => result.push(b'\n'),
                        Some(b'r') => result.push(b'\r'),
                        Some(b't') => result.push(b'\t'),
                        Some(b'b') => result.push(0x08),
                        Some(b'f') => result.push(0x0C),
                        Some(b'(') => result.push(b'('),
                        Some(b')') => result.push(b')'),
                        Some(b'\\') => result.push(b'\\'),
                        Some(b'\r') => {
                            // Line continuation: \<CR> or \<CR><LF>
                            if self.peek() == Some(b'\n') {
                                self.advance();
                            }
                        }
                        Some(b'\n') => {
                            // Line continuation: \<LF>
                        }
                        Some(d) if d.is_ascii_digit() => {
                            // Octal escape: up to 3 digits.
                            let mut octal: u16 = (d - b'0') as u16;
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(o) if o.is_ascii_digit() && o <= b'7' => {
                                        self.advance();
                                        octal = octal * 8 + (o - b'0') as u16;
                                    }
                                    _ => break,
                                }
                            }
                            result.push((octal & 0xFF) as u8);
                        }
                        Some(other) => {
                            // Unknown escape — ignore backslash per spec.
                            result.push(other);
                        }
                        None => {
                            return Err(LexError::unterminated("literal string escape", self.pos))
                        }
                    }
                }
                Some(b) => result.push(b),
            }
        }
        Ok(result)
    }

    fn read_name(&mut self) -> Result<CosName, LexError> {
        let mut name_bytes = Vec::new();
        while let Some(b) = self.peek() {
            if is_whitespace(b) || is_delimiter(b) {
                break;
            }
            self.advance();
            if b == b'#' {
                // Hex escape in name: #XX
                let hi = self
                    .advance()
                    .ok_or_else(|| LexError::unterminated("name hex escape", self.pos))?;
                let lo = self
                    .advance()
                    .ok_or_else(|| LexError::unterminated("name hex escape", self.pos))?;
                if !hi.is_ascii_hexdigit() || !lo.is_ascii_hexdigit() {
                    return Err(LexError::invalid_hex(hi, self.pos - 2));
                }
                name_bytes.push((hex_digit(hi) << 4) | hex_digit(lo));
            } else {
                name_bytes.push(b);
            }
        }
        Ok(CosName::new(name_bytes))
    }

    fn read_number(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        let mut has_dot = false;

        // Consume sign.
        if matches!(self.peek(), Some(b'+') | Some(b'-')) {
            self.advance();
        }
        // Consume digits and optional dot.
        while let Some(b) = self.peek() {
            if b == b'.' {
                if has_dot {
                    break;
                }
                has_dot = true;
                self.advance();
            } else if b.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        let slice = &self.data[start..self.pos];
        let s = std::str::from_utf8(slice).map_err(|_| LexError::invalid_number(start))?;

        if has_dot {
            let val: f64 = s.parse().map_err(|_| LexError::invalid_number(start))?;
            Ok(Token::Real(val))
        } else {
            let val: i64 = s.parse().map_err(|_| LexError::invalid_number(start))?;
            Ok(Token::Integer(val))
        }
    }

    fn read_keyword_or_bool(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        // Content-stream operators can contain non-alpha chars as suffix:
        //   T*   '   "   (PDF spec Table 107)
        // We allow: ascii alphabetic, *, ', " as keyword characters.
        while let Some(b) = self.peek() {
            if b.is_ascii_alphabetic() || matches!(b, b'*' | b'\'' | b'"') {
                self.advance();
            } else {
                break;
            }
        }
        let word = &self.data[start..self.pos];
        match word {
            b"true" => Ok(Token::Bool(true)),
            b"false" => Ok(Token::Bool(false)),
            _ => Ok(Token::Keyword(word.to_vec())),
        }
    }
}

// ---- Helpers ----

#[inline]
fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x00)
}

#[inline]
fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

#[inline]
fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

/// Lexer error with byte-offset context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub offset: usize,
}

impl LexError {
    fn unexpected(byte: u8, offset: usize) -> Self {
        Self {
            message: format!("unexpected byte 0x{byte:02X} ('{}')", byte as char),
            offset,
        }
    }

    fn unterminated(what: &str, offset: usize) -> Self {
        Self {
            message: format!("unterminated {what}"),
            offset,
        }
    }

    fn invalid_hex(byte: u8, offset: usize) -> Self {
        Self {
            message: format!("invalid hex digit 0x{byte:02X}"),
            offset,
        }
    }

    fn invalid_number(offset: usize) -> Self {
        Self {
            message: "invalid number literal".to_string(),
            offset,
        }
    }
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lex error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for LexError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_all(input: &[u8]) -> Vec<Token> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Ok(Some(tok)) = lexer.next_token() {
            tokens.push(tok);
        }
        tokens
    }

    #[test]
    fn lex_integer() {
        assert_eq!(lex_all(b"42"), vec![Token::Integer(42)]);
        assert_eq!(lex_all(b"-7"), vec![Token::Integer(-7)]);
        assert_eq!(lex_all(b"+3"), vec![Token::Integer(3)]);
        assert_eq!(lex_all(b"0"), vec![Token::Integer(0)]);
    }

    #[test]
    fn lex_real() {
        assert_eq!(lex_all(b"3.14"), vec![Token::Real(3.14)]);
        assert_eq!(lex_all(b"-0.5"), vec![Token::Real(-0.5)]);
        assert_eq!(lex_all(b".25"), vec![Token::Real(0.25)]);
    }

    #[test]
    fn lex_bool() {
        assert_eq!(lex_all(b"true"), vec![Token::Bool(true)]);
        assert_eq!(lex_all(b"false"), vec![Token::Bool(false)]);
    }

    #[test]
    fn lex_null_keyword() {
        assert_eq!(lex_all(b"null"), vec![Token::Keyword(b"null".to_vec())]);
    }

    #[test]
    fn lex_name() {
        let tokens = lex_all(b"/Type");
        assert_eq!(tokens, vec![Token::Name(CosName::new(b"Type".to_vec()))]);
    }

    #[test]
    fn lex_name_with_hex_escape() {
        // /A#20B -> name bytes "A B"
        let tokens = lex_all(b"/A#20B");
        assert_eq!(tokens, vec![Token::Name(CosName::new(b"A B".to_vec()))]);
    }

    #[test]
    fn lex_literal_string() {
        let tokens = lex_all(b"(hello world)");
        assert_eq!(tokens, vec![Token::LiteralString(b"hello world".to_vec())]);
    }

    #[test]
    fn lex_literal_string_nested_parens() {
        let tokens = lex_all(b"(a(b)c)");
        assert_eq!(tokens, vec![Token::LiteralString(b"a(b)c".to_vec())]);
    }

    #[test]
    fn lex_literal_string_escapes() {
        let tokens = lex_all(b"(\\n\\r\\t\\\\)");
        assert_eq!(
            tokens,
            vec![Token::LiteralString(b"\n\r\t\\".to_vec())]
        );
    }

    #[test]
    fn lex_literal_string_octal() {
        // \101 = 'A' (65)
        let tokens = lex_all(b"(\\101)");
        assert_eq!(tokens, vec![Token::LiteralString(b"A".to_vec())]);
    }

    #[test]
    fn lex_hex_string() {
        let tokens = lex_all(b"<48656C6C6F>");
        assert_eq!(tokens, vec![Token::HexString(b"Hello".to_vec())]);
    }

    #[test]
    fn lex_hex_string_odd_length() {
        // <ABC> pads to <ABC0> -> bytes 0xAB 0xC0
        let tokens = lex_all(b"<ABC>");
        assert_eq!(tokens, vec![Token::HexString(vec![0xAB, 0xC0])]);
    }

    #[test]
    fn lex_hex_string_with_whitespace() {
        let tokens = lex_all(b"<48 65 6C 6C 6F>");
        assert_eq!(tokens, vec![Token::HexString(b"Hello".to_vec())]);
    }

    #[test]
    fn lex_array_delimiters() {
        let tokens = lex_all(b"[1 2]");
        assert_eq!(
            tokens,
            vec![Token::ArrayStart, Token::Integer(1), Token::Integer(2), Token::ArrayEnd]
        );
    }

    #[test]
    fn lex_dict_delimiters() {
        let tokens = lex_all(b"<< /Type /Page >>");
        assert_eq!(
            tokens,
            vec![
                Token::DictStart,
                Token::Name(CosName::new(b"Type".to_vec())),
                Token::Name(CosName::new(b"Page".to_vec())),
                Token::DictEnd,
            ]
        );
    }

    #[test]
    fn lex_keywords() {
        let tokens = lex_all(b"obj endobj stream endstream");
        assert_eq!(
            tokens,
            vec![
                Token::Keyword(b"obj".to_vec()),
                Token::Keyword(b"endobj".to_vec()),
                Token::Keyword(b"stream".to_vec()),
                Token::Keyword(b"endstream".to_vec()),
            ]
        );
    }

    #[test]
    fn lex_skip_comments() {
        let tokens = lex_all(b"% this is a comment\n42");
        assert_eq!(tokens, vec![Token::Integer(42)]);
    }

    #[test]
    fn lex_mixed_sequence() {
        let input = b"<< /Length 5 /Filter /FlateDecode >>";
        let tokens = lex_all(input);
        assert_eq!(
            tokens,
            vec![
                Token::DictStart,
                Token::Name(CosName::new(b"Length".to_vec())),
                Token::Integer(5),
                Token::Name(CosName::new(b"Filter".to_vec())),
                Token::Name(CosName::new(b"FlateDecode".to_vec())),
                Token::DictEnd,
            ]
        );
    }

    #[test]
    fn lex_indirect_reference_tokens() {
        let tokens = lex_all(b"12 0 R");
        assert_eq!(
            tokens,
            vec![
                Token::Integer(12),
                Token::Integer(0),
                Token::Keyword(b"R".to_vec()),
            ]
        );
    }

    #[test]
    fn lex_error_unterminated_string() {
        let mut lexer = Lexer::new(b"(unterminated");
        let result = lexer.next_token();
        assert!(result.is_err());
    }

    #[test]
    fn lex_error_unterminated_hex() {
        let mut lexer = Lexer::new(b"<48656C6C6F");
        let result = lexer.next_token();
        assert!(result.is_err());
    }

    #[test]
    fn lex_position_tracking() {
        let mut lexer = Lexer::new(b"  42  true");
        assert_eq!(lexer.position(), 0);
        let _ = lexer.next_token();
        assert_eq!(lexer.position(), 4);
        let _ = lexer.next_token();
        assert_eq!(lexer.position(), 10);
    }
}

