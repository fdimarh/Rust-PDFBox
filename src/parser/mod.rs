//! Lexer, parser, xref, and regression tests for PDF syntax and xref structures.
//!
//! This module provides:
//! - [`Lexer`]: tokenizes raw PDF bytes into [`Token`] values.
//! - [`Parser`]: consumes tokens to build [`CosObject`] trees.
//! - [`xref`]: cross-reference table/stream parsing and `startxref` discovery.
//! - [`xref_stream`]: XRef stream support (PDF 1.5+).
//! - [`objstm`]: Object stream support (PDF 1.5+) for compressed objects.
//! - [`malformed`]: regression tests for malformed and edge-case inputs.
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type |
//! |---|---|
//! | `BaseParser` (token reading) | [`Lexer`] |
//! | `BaseParser` / `COSParser` (object building) | [`Parser`] |
//! | `COSParser.parseXref` | [`xref`] module (ASCII) |
//! | `PDFXRefStream` | [`xref_stream`] module (binary, PDF 1.5+) |
//! | `PDFObjectStream` | [`objstm`] module (compressed objects, PDF 1.5+) |

pub mod lexer;
pub mod malformed;
pub mod objstm;
pub mod parser;
pub mod xref;
pub mod xref_stream;

pub use xref::{XRefEntry, XRefTable, find_startxref, load_xref, parse_xref_table, read_stream_data};
pub use xref_stream::{XRefStream, XRefSubsection};
// Binary xref entry type (PDF 1.5+) — kept separate from ASCII xref entry
pub use xref_stream::XRefEntry as BinaryXRefEntry;

pub use objstm::ObjectStream;

pub use lexer::{LexError, Lexer, Token};
pub use parser::{ParseError, Parser};
