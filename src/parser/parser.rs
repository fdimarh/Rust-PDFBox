//! PDF object parser — consumes tokens from the lexer to produce COS objects.
//!
//! Maps to Java PDFBox `BaseParser`/`COSParser` object-level parsing.
//! The parser reads tokens from a [`Lexer`] and builds [`CosObject`] trees,
//! including arrays, dictionaries, and indirect object references.

use crate::cos::{CosDictionary, CosObject, CosStream, ObjectId};

use super::lexer::{LexError, Lexer, Token};

/// Parser error with context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

impl ParseError {
    pub fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }

    fn expected(what: &str, got: &Token, offset: usize) -> Self {
        Self {
            message: format!("expected {what}, got {got:?}"),
            offset,
        }
    }

    fn unexpected_eof(what: &str, offset: usize) -> Self {
        Self {
            message: format!("unexpected EOF while parsing {what}"),
            offset,
        }
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        Self {
            message: e.message,
            offset: e.offset,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// PDF object parser wrapping a lexer.
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    /// Lookahead buffer for up to 3 tokens (needed for indirect ref detection).
    lookahead: Vec<(Token, usize)>,
}

impl<'a> Parser<'a> {
    /// Creates a new parser over the given byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            lexer: Lexer::new(data),
            lookahead: Vec::new(),
        }
    }

    /// Creates a parser from an existing lexer.
    pub fn from_lexer(lexer: Lexer<'a>) -> Self {
        Self {
            lexer,
            lookahead: Vec::new(),
        }
    }

    /// Returns the current byte offset (approximate — may be past lookahead).
    pub fn position(&self) -> usize {
        if let Some((_, pos)) = self.lookahead.first() {
            *pos
        } else {
            self.lexer.position()
        }
    }

    /// Provides mutable access to the underlying lexer.
    pub fn lexer_mut(&mut self) -> &mut Lexer<'a> {
        self.lookahead.clear();
        &mut self.lexer
    }

    // ---- Token access with lookahead ----

    fn peek_token(&mut self) -> Result<Option<&Token>, ParseError> {
        if self.lookahead.is_empty() {
            let pos = self.lexer.position();
            if let Some(tok) = self.lexer.next_token()? {
                self.lookahead.push((tok, pos));
            } else {
                return Ok(None);
            }
        }
        Ok(self.lookahead.first().map(|(t, _)| t))
    }

    fn next_token(&mut self) -> Result<Option<(Token, usize)>, ParseError> {
        if let Some(item) = self.lookahead.first().cloned() {
            self.lookahead.remove(0);
            Ok(Some(item))
        } else {
            let pos = self.lexer.position();
            match self.lexer.next_token()? {
                Some(tok) => Ok(Some((tok, pos))),
                None => Ok(None),
            }
        }
    }

    fn push_back(&mut self, tok: Token, pos: usize) {
        self.lookahead.insert(0, (tok, pos));
    }

    // ---- Public parsing API ----

    /// Parses one COS object from the current position.
    ///
    /// Handles indirect references (`N G R`) by peeking ahead.
    pub fn parse_object(&mut self) -> Result<Option<CosObject>, ParseError> {
        let Some((tok, tok_pos)) = self.next_token()? else {
            return Ok(None);
        };

        match tok {
            Token::Bool(b) => Ok(Some(CosObject::Bool(b))),
            Token::Integer(n) => {
                // Check for indirect reference: <int> <int> R
                if let Some(obj) = self.try_indirect_reference(n, tok_pos)? {
                    return Ok(Some(obj));
                }
                Ok(Some(CosObject::Integer(n)))
            }
            Token::Real(n) => Ok(Some(CosObject::Real(n))),
            Token::LiteralString(s) => Ok(Some(CosObject::String(s))),
            Token::HexString(s) => Ok(Some(CosObject::String(s))),
            Token::Name(n) => Ok(Some(CosObject::Name(n))),
            Token::Keyword(ref kw) if kw == b"null" => Ok(Some(CosObject::Null)),
            Token::ArrayStart => self.parse_array().map(Some),
            Token::DictStart => self.parse_dict_or_stream().map(Some),
            // Other keywords (obj, endobj, etc.) are not COS values — push back.
            _ => {
                self.push_back(tok, tok_pos);
                Ok(None)
            }
        }
    }

    /// Tries to parse `<generation> R` after an integer to form an indirect reference.
    fn try_indirect_reference(
        &mut self,
        obj_num: i64,
        _first_pos: usize,
    ) -> Result<Option<CosObject>, ParseError> {
        // Peek at next token. Must be integer (generation).
        let Some((tok2, pos2)) = self.next_token()? else {
            return Ok(None);
        };
        let Token::Integer(generation) = tok2 else {
            self.push_back(tok2, pos2);
            return Ok(None);
        };
        // Peek at token after that. Must be keyword "R".
        let Some((tok3, pos3)) = self.next_token()? else {
            self.push_back(Token::Integer(generation), pos2);
            return Ok(None);
        };
        if tok3.is_keyword(b"R") {
            let id = ObjectId::new(obj_num as u32, generation as u16);
            Ok(Some(CosObject::Reference(id)))
        } else {
            // Not an indirect ref — push both tokens back.
            self.push_back(tok3, pos3);
            self.push_back(Token::Integer(generation), pos2);
            Ok(None)
        }
    }

    /// Parses an array `[ ... ]`.
    fn parse_array(&mut self) -> Result<CosObject, ParseError> {
        let mut items = Vec::new();
        loop {
            if let Some(tok) = self.peek_token()? {
                if matches!(tok, Token::ArrayEnd) {
                    self.next_token()?; // consume ]
                    break;
                }
            } else {
                return Err(ParseError::unexpected_eof("array", self.lexer.position()));
            }

            match self.parse_object()? {
                Some(obj) => items.push(obj),
                None => {
                    // Consume unexpected token to avoid infinite loop.
                    let _ = self.next_token()?;
                }
            }
        }
        Ok(CosObject::Array(items))
    }

    /// Parses a dictionary `<< ... >>`, and checks for a following `stream`.
    fn parse_dict_or_stream(&mut self) -> Result<CosObject, ParseError> {
        let dict = self.parse_dictionary_entries()?;

        // Check if followed by `stream` keyword (makes this a stream object).
        if let Some(tok) = self.peek_token()? {
            if tok.is_keyword(b"stream") {
                self.next_token()?; // consume "stream"

                // Stream data reading requires knowing Length from dict.
                // For now, we store an empty data vec; full stream reading
                // will be wired in when xref + object loading is complete.
                let stream = CosStream::new(dict, Vec::new());
                return Ok(CosObject::Stream(stream));
            }
        }

        Ok(CosObject::Dictionary(dict))
    }

    /// Reads key-value pairs until `>>`.
    fn parse_dictionary_entries(&mut self) -> Result<CosDictionary, ParseError> {
        let mut dict = CosDictionary::new();
        loop {
            // Peek for >>
            let Some(tok) = self.peek_token()? else {
                return Err(ParseError::unexpected_eof(
                    "dictionary",
                    self.lexer.position(),
                ));
            };
            if matches!(tok, Token::DictEnd) {
                self.next_token()?; // consume >>
                break;
            }

            // Key must be a Name.
            let (key_tok, key_pos) = self.next_token()?.ok_or_else(|| {
                ParseError::unexpected_eof("dictionary key", self.lexer.position())
            })?;
            let Token::Name(key) = key_tok else {
                return Err(ParseError::expected("name key", &key_tok, key_pos));
            };

            // Value is any object.
            let value = self.parse_object()?.ok_or_else(|| {
                ParseError::unexpected_eof("dictionary value", self.lexer.position())
            })?;

            dict.insert(key, value);
        }
        Ok(dict)
    }

    // ---- Indirect object definition parsing ----

    /// Parses an indirect object definition: `N G obj <value> endobj`.
    ///
    /// Caller has already consumed up to the point where `N G obj` is next.
    pub fn parse_indirect_object(&mut self) -> Result<Option<(ObjectId, CosObject)>, ParseError> {
        let Some((tok1, pos1)) = self.next_token()? else {
            return Ok(None);
        };
        let Token::Integer(obj_num) = tok1 else {
            self.push_back(tok1, pos1);
            return Ok(None);
        };

        let (tok2, _) = self.next_token()?.ok_or_else(|| {
            ParseError::unexpected_eof("indirect object generation", self.lexer.position())
        })?;
        let Token::Integer(generation) = tok2 else {
            return Err(ParseError::expected("generation number", &tok2, self.lexer.position()));
        };

        let (tok3, pos3) = self.next_token()?.ok_or_else(|| {
            ParseError::unexpected_eof("obj keyword", self.lexer.position())
        })?;
        if !tok3.is_keyword(b"obj") {
            return Err(ParseError::expected("'obj' keyword", &tok3, pos3));
        }

        let id = ObjectId::new(obj_num as u32, generation as u16);
        let value = self
            .parse_object()?
            .unwrap_or(CosObject::Null);

        // Consume optional `endobj`.
        if let Some(tok) = self.peek_token()? {
            if tok.is_keyword(b"endobj") {
                self.next_token()?;
            }
        }

        Ok(Some((id, value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::CosName;

    fn parse_one(input: &[u8]) -> CosObject {
        let mut parser = Parser::new(input);
        parser.parse_object().unwrap().unwrap()
    }

    #[test]
    fn parse_null() {
        assert_eq!(parse_one(b"null"), CosObject::Null);
    }

    #[test]
    fn parse_bool() {
        assert_eq!(parse_one(b"true"), CosObject::Bool(true));
        assert_eq!(parse_one(b"false"), CosObject::Bool(false));
    }

    #[test]
    fn parse_integer() {
        assert_eq!(parse_one(b"42"), CosObject::Integer(42));
        assert_eq!(parse_one(b"-7"), CosObject::Integer(-7));
    }

    #[test]
    fn parse_real() {
        assert_eq!(parse_one(b"3.14"), CosObject::Real(3.14));
    }

    #[test]
    fn parse_name() {
        assert_eq!(
            parse_one(b"/Type"),
            CosObject::Name(CosName::new(b"Type".to_vec()))
        );
    }

    #[test]
    fn parse_literal_string() {
        assert_eq!(
            parse_one(b"(hello)"),
            CosObject::String(b"hello".to_vec())
        );
    }

    #[test]
    fn parse_hex_string() {
        assert_eq!(
            parse_one(b"<48656C6C6F>"),
            CosObject::String(b"Hello".to_vec())
        );
    }

    #[test]
    fn parse_array() {
        let obj = parse_one(b"[1 2 3]");
        let arr = obj.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], CosObject::Integer(1));
        assert_eq!(arr[1], CosObject::Integer(2));
        assert_eq!(arr[2], CosObject::Integer(3));
    }

    #[test]
    fn parse_nested_array() {
        let obj = parse_one(b"[1 [2 3]]");
        let arr = obj.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_empty_array() {
        let obj = parse_one(b"[]");
        assert_eq!(obj.as_array().unwrap().len(), 0);
    }

    #[test]
    fn parse_dictionary() {
        let obj = parse_one(b"<< /Type /Page /Count 3 >>");
        let dict = obj.as_dictionary().unwrap();
        assert_eq!(dict.len(), 2);
        assert_eq!(
            dict.get_name(&CosName::type_name()),
            Some(&CosName::new(b"Page".to_vec()))
        );
        assert_eq!(dict.get_int(&CosName::count()), Some(3));
    }

    #[test]
    fn parse_empty_dictionary() {
        let obj = parse_one(b"<< >>");
        assert_eq!(obj.as_dictionary().unwrap().len(), 0);
    }

    #[test]
    fn parse_nested_dictionary() {
        let obj = parse_one(b"<< /Inner << /Key 42 >> >>");
        let dict = obj.as_dictionary().unwrap();
        let inner = dict
            .get_dictionary(&CosName::new(b"Inner".to_vec()))
            .unwrap();
        assert_eq!(
            inner.get_int(&CosName::new(b"Key".to_vec())),
            Some(42)
        );
    }

    #[test]
    fn parse_indirect_reference() {
        let obj = parse_one(b"12 0 R");
        assert_eq!(obj.as_reference(), Some(ObjectId::new(12, 0)));
    }

    #[test]
    fn parse_dict_with_reference() {
        let obj = parse_one(b"<< /Pages 4 0 R >>");
        let dict = obj.as_dictionary().unwrap();
        let pages_val = dict.get(&CosName::pages()).unwrap();
        assert_eq!(pages_val.as_reference(), Some(ObjectId::new(4, 0)));
    }

    #[test]
    fn parse_indirect_object_definition() {
        let input = b"1 0 obj\n<< /Type /Catalog >>\nendobj";
        let mut parser = Parser::new(input);
        let (id, value) = parser.parse_indirect_object().unwrap().unwrap();
        assert_eq!(id, ObjectId::new(1, 0));
        let dict = value.as_dictionary().unwrap();
        assert_eq!(
            dict.get_name(&CosName::type_name()),
            Some(&CosName::new(b"Catalog".to_vec()))
        );
    }

    #[test]
    fn parse_multiple_indirect_objects() {
        let input = b"1 0 obj\n42\nendobj\n2 0 obj\ntrue\nendobj";
        let mut parser = Parser::new(input);

        let (id1, val1) = parser.parse_indirect_object().unwrap().unwrap();
        assert_eq!(id1, ObjectId::new(1, 0));
        assert_eq!(val1, CosObject::Integer(42));

        let (id2, val2) = parser.parse_indirect_object().unwrap().unwrap();
        assert_eq!(id2, ObjectId::new(2, 0));
        assert_eq!(val2, CosObject::Bool(true));
    }

    #[test]
    fn parse_array_with_mixed_types() {
        let obj = parse_one(b"[1 3.14 true (hi) /Name null]");
        let arr = obj.as_array().unwrap();
        assert_eq!(arr.len(), 6);
        assert_eq!(arr[0], CosObject::Integer(1));
        assert_eq!(arr[1], CosObject::Real(3.14));
        assert_eq!(arr[2], CosObject::Bool(true));
        assert_eq!(arr[3], CosObject::String(b"hi".to_vec()));
        assert_eq!(arr[4], CosObject::Name(CosName::new(b"Name".to_vec())));
        assert_eq!(arr[5], CosObject::Null);
    }

    #[test]
    fn parse_array_with_indirect_refs() {
        let obj = parse_one(b"[1 0 R 2 0 R]");
        let arr = obj.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_reference(), Some(ObjectId::new(1, 0)));
        assert_eq!(arr[1].as_reference(), Some(ObjectId::new(2, 0)));
    }

    #[test]
    fn integer_not_confused_with_ref() {
        // "42" alone is just an integer, not start of a reference.
        assert_eq!(parse_one(b"42"), CosObject::Integer(42));
    }

    #[test]
    fn parse_error_unterminated_dict() {
        let mut parser = Parser::new(b"<< /Key 1");
        let result = parser.parse_object();
        assert!(result.is_err());
    }
}

