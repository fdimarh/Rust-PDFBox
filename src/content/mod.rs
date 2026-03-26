//! Content stream tokenizer, instruction parser, and graphics state machine.
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type |
//! |---|---|
//! | `PDFStreamEngine` (token loop) | [`ContentTokenizer`] |
//! | `Operator` | [`Operator`] |
//! | `COSBase` operand stack | [`ContentToken`] |
//! | `PDGraphicsState` + `PDTextState` | [`graphics_state::GraphicsState`] |

pub mod graphics_state;

pub use graphics_state::{GraphicsState, Matrix, TextState};

use crate::cos::{CosDictionary, CosObject};
use crate::parser::lexer::{LexError, Lexer, Token};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single token from a content stream — either an operand or an operator.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentToken {
    /// An operand value (number, string, name, array, dict, bool, null).
    Operand(CosObject),
    /// An operator keyword (e.g. `BT`, `Tf`, `Tj`, `ET`, `cm`, `q`, `Q`).
    Operator(Operator),
}

/// A PDF content stream operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operator {
    /// The raw operator bytes (e.g. `b"BT"`, `b"Tj"`, `b"cm"`).
    pub name: Vec<u8>,
}

impl Operator {
    pub fn new(name: impl Into<Vec<u8>>) -> Self {
        Self { name: name.into() }
    }

    /// Returns the operator as a UTF-8 string if valid.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.name).ok()
    }

    // ----- Well-known operator predicates -----

    /// `BT` — begin text object.
    pub fn is_begin_text(&self) -> bool { self.name == b"BT" }
    /// `ET` — end text object.
    pub fn is_end_text(&self) -> bool { self.name == b"ET" }
    /// `Tf` — set text font and size.
    pub fn is_set_font(&self) -> bool { self.name == b"Tf" }
    /// `Tj` — show text string.
    pub fn is_show_text(&self) -> bool { self.name == b"Tj" }
    /// `TJ` — show text with individual glyph positioning.
    pub fn is_show_text_positioned(&self) -> bool { self.name == b"TJ" }
    /// `Td` — move text position.
    pub fn is_move_text(&self) -> bool { self.name == b"Td" }
    /// `TD` — move text position and set leading.
    pub fn is_move_text_set_leading(&self) -> bool { self.name == b"TD" }
    /// `Tm` — set text matrix.
    pub fn is_set_text_matrix(&self) -> bool { self.name == b"Tm" }
    /// `T*` — move to next line.
    pub fn is_next_line(&self) -> bool { self.name == b"T*" }
    /// `q` — save graphics state.
    pub fn is_save_state(&self) -> bool { self.name == b"q" }
    /// `Q` — restore graphics state.
    pub fn is_restore_state(&self) -> bool { self.name == b"Q" }
    /// `cm` — concatenate matrix.
    pub fn is_concat_matrix(&self) -> bool { self.name == b"cm" }
    /// `Do` — invoke named XObject.
    pub fn is_do(&self) -> bool { self.name == b"Do" }
}

impl std::fmt::Display for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.name))
    }
}

// ---------------------------------------------------------------------------
// ContentTokenizer
// ---------------------------------------------------------------------------

/// Tokenizes a PDF content stream byte slice into [`ContentToken`] values.
///
/// Wraps the low-level [`Lexer`] and classifies each output token as either
/// an operand (COS value) or an operator keyword.
pub struct ContentTokenizer<'a> {
    lexer: Lexer<'a>,
    peeked: Option<(Token, usize)>,
}

impl<'a> ContentTokenizer<'a> {
    /// Creates a new tokenizer over the given content stream bytes.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            lexer: Lexer::new(data),
            peeked: None,
        }
    }

    /// Returns the current byte position.
    pub fn position(&self) -> usize {
        if let Some((_, pos)) = &self.peeked {
            *pos
        } else {
            self.lexer.position()
        }
    }

    /// Advances and returns the next content token, or `None` at end of stream.
    pub fn next_token(&mut self) -> Result<Option<ContentToken>, LexError> {
        let (tok, _pos) = match self.next_raw()? {
            Some(t) => t,
            None => return Ok(None),
        };

        let ct = match tok {
            Token::Bool(b) => ContentToken::Operand(CosObject::Bool(b)),
            Token::Integer(n) => ContentToken::Operand(CosObject::Integer(n)),
            Token::Real(r) => ContentToken::Operand(CosObject::Real(r)),
            Token::LiteralString(s) => ContentToken::Operand(CosObject::String(s)),
            Token::HexString(s) => ContentToken::Operand(CosObject::String(s)),
            Token::Name(n) => ContentToken::Operand(CosObject::Name(n)),
            Token::ArrayStart => {
                let arr = self.read_array()?;
                ContentToken::Operand(CosObject::Array(arr))
            }
            Token::DictStart => {
                let dict = self.read_dict()?;
                ContentToken::Operand(CosObject::Dictionary(dict))
            }
            Token::Keyword(kw) => {
                if kw == b"null" {
                    ContentToken::Operand(CosObject::Null)
                } else {
                    ContentToken::Operator(Operator::new(kw))
                }
            }
            Token::ArrayEnd | Token::DictEnd => {
                return self.next_token();
            }
        };

        Ok(Some(ct))
    }

    /// Collects all tokens into a flat `Vec`.
    pub fn collect_all(&mut self) -> Result<Vec<ContentToken>, LexError> {
        let mut out = Vec::new();
        while let Some(t) = self.next_token()? {
            out.push(t);
        }
        Ok(out)
    }

    fn next_raw(&mut self) -> Result<Option<(Token, usize)>, LexError> {
        if let Some(item) = self.peeked.take() {
            return Ok(Some(item));
        }
        let pos = self.lexer.position();
        match self.lexer.next_token()? {
            Some(tok) => Ok(Some((tok, pos))),
            None => Ok(None),
        }
    }

    fn read_array(&mut self) -> Result<Vec<CosObject>, LexError> {
        let mut items = Vec::new();
        loop {
            match self.next_raw()? {
                None => break,
                Some((Token::ArrayEnd, _)) => break,
                Some((tok, p)) => {
                    if let Some(obj) = self.token_to_object(tok, p)? {
                        items.push(obj);
                    }
                }
            }
        }
        Ok(items)
    }

    fn read_dict(&mut self) -> Result<CosDictionary, LexError> {
        let mut dict = CosDictionary::new();
        loop {
            match self.next_raw()? {
                None => break,
                Some((Token::DictEnd, _)) => break,
                Some((Token::Name(key), _)) => {
                    match self.next_raw()? {
                        Some((vtok, vp)) => {
                            if let Some(val) = self.token_to_object(vtok, vp)? {
                                dict.insert(key, val);
                            }
                        }
                        None => break,
                    }
                }
                Some(_) => {}
            }
        }
        Ok(dict)
    }

    fn token_to_object(&mut self, tok: Token, _pos: usize) -> Result<Option<CosObject>, LexError> {
        let obj = match tok {
            Token::Bool(b) => CosObject::Bool(b),
            Token::Integer(n) => CosObject::Integer(n),
            Token::Real(r) => CosObject::Real(r),
            Token::LiteralString(s) => CosObject::String(s),
            Token::HexString(s) => CosObject::String(s),
            Token::Name(n) => CosObject::Name(n),
            Token::Keyword(kw) if kw == b"null" => CosObject::Null,
            Token::ArrayStart => CosObject::Array(self.read_array()?),
            Token::DictStart => CosObject::Dictionary(self.read_dict()?),
            _ => return Ok(None),
        };
        Ok(Some(obj))
    }
}

// ---------------------------------------------------------------------------
// Instruction — grouped (operands, operator) unit
// ---------------------------------------------------------------------------

/// A single content stream instruction: operands followed by one operator.
///
/// Maps to the operation dispatch loop in `PDFStreamEngine` (Java PDFBox).
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub operands: Vec<CosObject>,
    pub operator: Operator,
}

impl Instruction {
    pub fn new(operands: Vec<CosObject>, operator: Operator) -> Self {
        Self { operands, operator }
    }
}

/// Parses a content stream byte slice into a sequence of [`Instruction`] values.
///
/// Groups operands with the operator that follows them — the natural unit
/// consumed by graphics-state and text-extraction engines.
pub fn parse_content_stream(data: &[u8]) -> Result<Vec<Instruction>, LexError> {
    let mut tokenizer = ContentTokenizer::new(data);
    let mut instructions = Vec::new();
    let mut operand_stack: Vec<CosObject> = Vec::new();

    while let Some(ct) = tokenizer.next_token()? {
        match ct {
            ContentToken::Operand(obj) => operand_stack.push(obj),
            ContentToken::Operator(op) => {
                instructions.push(Instruction::new(
                    std::mem::take(&mut operand_stack),
                    op,
                ));
            }
        }
    }

    Ok(instructions)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::CosName;

    fn tokenize(data: &[u8]) -> Vec<ContentToken> {
        let mut tok = ContentTokenizer::new(data);
        tok.collect_all().unwrap()
    }

    fn instructions(data: &[u8]) -> Vec<Instruction> {
        parse_content_stream(data).unwrap()
    }

    #[test]
    fn tokenize_simple_operator() {
        let tokens = tokenize(b"q");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], ContentToken::Operator(op) if op.is_save_state()));
    }

    #[test]
    fn tokenize_number_operands_and_operator() {
        let tokens = tokenize(b"612 0 0 792 0 0 cm");
        assert_eq!(tokens.len(), 7);
        assert!(matches!(&tokens[6], ContentToken::Operator(op) if op.is_concat_matrix()));
    }

    #[test]
    fn tokenize_begin_end_text() {
        let tokens = tokenize(b"BT ET");
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[0], ContentToken::Operator(op) if op.is_begin_text()));
        assert!(matches!(&tokens[1], ContentToken::Operator(op) if op.is_end_text()));
    }

    #[test]
    fn tokenize_show_text() {
        let tokens = tokenize(b"(Hello World) Tj");
        assert_eq!(tokens.len(), 2);
        assert_eq!(
            tokens[0],
            ContentToken::Operand(CosObject::String(b"Hello World".to_vec()))
        );
        assert!(matches!(&tokens[1], ContentToken::Operator(op) if op.is_show_text()));
    }

    #[test]
    fn tokenize_set_font() {
        let tokens = tokenize(b"/F1 12 Tf");
        assert_eq!(tokens.len(), 3);
        assert_eq!(
            tokens[0],
            ContentToken::Operand(CosObject::Name(CosName::new(b"F1".to_vec())))
        );
        assert_eq!(tokens[1], ContentToken::Operand(CosObject::Integer(12)));
        assert!(matches!(&tokens[2], ContentToken::Operator(op) if op.is_set_font()));
    }

    #[test]
    fn tokenize_array_operand() {
        let tokens = tokenize(b"[(Hello) -20 (World)] TJ");
        assert_eq!(tokens.len(), 2);
        if let ContentToken::Operand(CosObject::Array(arr)) = &tokens[0] {
            assert_eq!(arr.len(), 3);
        } else {
            panic!("expected array operand");
        }
        assert!(matches!(&tokens[1], ContentToken::Operator(op) if op.is_show_text_positioned()));
    }

    #[test]
    fn tokenize_null_operand() {
        let tokens = tokenize(b"null Tj");
        assert_eq!(tokens[0], ContentToken::Operand(CosObject::Null));
    }

    #[test]
    fn tokenize_save_restore_state() {
        let tokens = tokenize(b"q Q");
        assert!(matches!(&tokens[0], ContentToken::Operator(op) if op.is_save_state()));
        assert!(matches!(&tokens[1], ContentToken::Operator(op) if op.is_restore_state()));
    }

    #[test]
    fn tokenize_do_operator() {
        let tokens = tokenize(b"/Im1 Do");
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], ContentToken::Operator(op) if op.is_do()));
    }

    #[test]
    fn tokenize_empty_stream() {
        assert!(tokenize(b"").is_empty());
    }

    #[test]
    fn tokenize_comment_skipped() {
        let tokens = tokenize(b"% this is a comment\nBT");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], ContentToken::Operator(op) if op.is_begin_text()));
    }

    #[test]
    fn instructions_simple_sequence() {
        let instrs = instructions(b"BT /F1 12 Tf (Hello) Tj ET");
        assert_eq!(instrs.len(), 4);

        assert_eq!(instrs[0].operator.name, b"BT");
        assert!(instrs[0].operands.is_empty());

        assert_eq!(instrs[1].operator.name, b"Tf");
        assert_eq!(instrs[1].operands.len(), 2);
        assert_eq!(instrs[1].operands[0], CosObject::Name(CosName::new(b"F1".to_vec())));
        assert_eq!(instrs[1].operands[1], CosObject::Integer(12));

        assert_eq!(instrs[2].operator.name, b"Tj");
        assert_eq!(instrs[2].operands.len(), 1);
        assert_eq!(instrs[2].operands[0], CosObject::String(b"Hello".to_vec()));

        assert_eq!(instrs[3].operator.name, b"ET");
        assert!(instrs[3].operands.is_empty());
    }

    #[test]
    fn instructions_graphics_state() {
        let instrs = instructions(b"q 1 0 0 1 100 200 cm Q");
        assert_eq!(instrs.len(), 3);
        assert!(instrs[0].operator.is_save_state());
        assert!(instrs[1].operator.is_concat_matrix());
        assert_eq!(instrs[1].operands.len(), 6);
        assert!(instrs[2].operator.is_restore_state());
    }

    #[test]
    fn instructions_move_show_text() {
        let instrs = instructions(b"100 200 Td (line one) Tj T* (line two) Tj");
        assert_eq!(instrs.len(), 4);
        assert!(instrs[0].operator.is_move_text());
        assert!(instrs[1].operator.is_show_text());
        assert!(instrs[2].operator.is_next_line());
        assert!(instrs[3].operator.is_show_text());
    }

    #[test]
    fn instructions_tj_array() {
        let instrs = instructions(b"[(A) -10 (B)] TJ");
        assert_eq!(instrs.len(), 1);
        assert!(instrs[0].operator.is_show_text_positioned());
        assert!(matches!(&instrs[0].operands[0], CosObject::Array(arr) if arr.len() == 3));
    }

    #[test]
    fn operator_display() {
        let op = Operator::new(b"BT".to_vec());
        assert_eq!(op.to_string(), "BT");
    }

    #[test]
    fn instruction_text_matrix() {
        let instrs = instructions(b"1 0 0 1 72 720 Tm");
        assert_eq!(instrs.len(), 1);
        assert!(instrs[0].operator.is_set_text_matrix());
        assert_eq!(instrs[0].operands.len(), 6);
    }

    #[test]
    fn instruction_next_line() {
        let instrs = instructions(b"T*");
        assert_eq!(instrs.len(), 1);
        assert!(instrs[0].operator.is_next_line());
        assert!(instrs[0].operands.is_empty());
    }

    #[test]
    fn instruction_multiline_stream() {
        let data = b"BT\n/F1 12 Tf\n72 720 Td\n(First line) Tj\nT*\n(Second line) Tj\nET\n";
        let instrs = instructions(data);
        assert_eq!(instrs.len(), 7);
        let texts: Vec<_> = instrs
            .iter()
            .filter(|i| i.operator.is_show_text())
            .map(|i| i.operands[0].as_string().unwrap().to_vec())
            .collect();
        assert_eq!(texts[0], b"First line");
        assert_eq!(texts[1], b"Second line");
    }
}
