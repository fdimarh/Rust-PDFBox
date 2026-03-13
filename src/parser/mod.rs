//! Lexer and parser entry points for PDF syntax and xref structures.
//!
//! This module provides:
//! - [`Lexer`]: tokenizes raw PDF bytes into [`Token`] values.
//! - [`Parser`]: consumes tokens to build [`CosObject`] trees.
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type |
//! |---|---|
//! | `BaseParser` (token reading) | [`Lexer`] |
//! | `BaseParser` / `COSParser` (object building) | [`Parser`] |

pub mod lexer;
pub mod parser;

pub use lexer::{LexError, Lexer, Token};
pub use parser::{ParseError, Parser};
