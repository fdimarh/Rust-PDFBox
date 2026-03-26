//! Lexer, parser, xref, and regression tests for PDF syntax and xref structures.
//!
//! This module provides:
//! - [`Lexer`]: tokenizes raw PDF bytes into [`Token`] values.
//! - [`Parser`]: consumes tokens to build [`CosObject`] trees.
//! - [`xref`]: cross-reference table/stream parsing and `startxref` discovery.
//! - [`malformed`]: regression tests for malformed and edge-case inputs.
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type |
//! |---|---|
//! | `BaseParser` (token reading) | [`Lexer`] |
//! | `BaseParser` / `COSParser` (object building) | [`Parser`] |
//! | `COSParser.parseXref` / `PDFXRefStream` | [`xref`] module |

pub mod lexer;
pub mod malformed;
pub mod parser;
pub mod xref;

pub use lexer::{LexError, Lexer, Token};
pub use parser::{ParseError, Parser};
pub use xref::{XRefEntry, XRefTable, find_startxref, load_xref, parse_xref_table, read_stream_data};
