//! Font dictionaries, encodings, and glyph mapping logic.
//!
//! Phase 3 — M3: ToUnicode CMap parser implemented.

pub mod cmap;

pub use cmap::{ToUnicodeCMap, parse_to_unicode_cmap};
