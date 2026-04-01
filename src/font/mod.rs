//! Font dictionaries, encodings, glyph mapping, and text decoding.
//!
//! # Module overview
//!
//! | Module | What it implements | Java PDFBox mapping |
//! |---|---|---|
//! | [`cmap`] | ToUnicode CMap parser | `CMapParser`, `ToUnicodeWriter` |
//! | [`descriptor`] | Font descriptor + metrics + flags | `PDFontDescriptor` |
//! | [`encoding`] | Single-byte encodings + glyph names | `Encoding`, `WinAnsiEncoding`, etc. |
//! | [`simple`] | Type1 / TrueType / MMType1 / Type3 | `PDSimpleFont` subtypes |
//! | [`type0`] | Type0 composite + CIDFont | `PDType0Font` |
//! | [`font`] | Unified `PdfFont` enum + `FontResolver` | `PDFont`, `PDResources.getFont` |

pub mod cmap;
pub mod descriptor;
pub mod encoding;
pub mod simple;
pub mod type0;
pub mod font;

pub use cmap::{ToUnicodeCMap, parse_to_unicode_cmap};
pub use descriptor::{FontDescriptor, FontFlags, FontBBox};
pub use encoding::{BaseEncoding, Encoding, glyph_name_to_char};
pub use simple::{SimpleFont, SimpleFontSubtype, GlyphWidths};
pub use type0::{Type0Font, DescendantFont, CidFontType, CidSystemInfo};
pub use font::{PdfFont, FontResolver};
