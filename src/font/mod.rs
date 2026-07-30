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
pub mod font;
pub mod simple;
pub mod type0;

pub use cmap::{ToUnicodeCMap, parse_to_unicode_cmap};
pub use descriptor::{FontBBox, FontDescriptor, FontFlags};
pub use encoding::{BaseEncoding, Encoding, glyph_name_to_char};
pub use font::{FontResolver, PdfFont};
pub use simple::{GlyphWidths, SimpleFont, SimpleFontSubtype};
pub use type0::{CidFontType, CidSystemInfo, DescendantFont, Type0Font};
