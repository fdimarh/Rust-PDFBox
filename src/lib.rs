//! Rust port scaffolding for Apache PDFBox.

pub mod content;
pub mod cos;
pub mod crypto;
pub mod font;
pub mod io;
pub mod parser;
pub mod pdmodel;
pub mod render;
pub mod text;
pub mod writer;

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io as std_io;
use std::io::Read;
use std::path::Path;

pub use cos::ObjectId;

#[derive(Debug)]
pub enum PdfError {
    Io(std_io::Error),
    Parse {
        offset: Option<u64>,
        context: &'static str,
    },
    Xref {
        object_id: Option<ObjectId>,
    },
    Font {
        font_name: String,
    },
    Crypto,
    Unsupported {
        feature: &'static str,
    },
}

impl Display for PdfError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Parse { offset, context } => {
                if let Some(offset) = offset {
                    write!(f, "parse error at byte {offset}: {context}")
                } else {
                    write!(f, "parse error: {context}")
                }
            }
            Self::Xref { object_id } => {
                if let Some(object_id) = object_id {
                    write!(
                        f,
                        "xref resolution error for object {} {}",
                        object_id.object_number, object_id.generation
                    )
                } else {
                    write!(f, "xref resolution error")
                }
            }
            Self::Font { font_name } => write!(f, "font error: {font_name}"),
            Self::Crypto => write!(f, "crypto error"),
            Self::Unsupported { feature } => write!(f, "unsupported feature: {feature}"),
        }
    }
}

impl Error for PdfError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std_io::Error> for PdfError {
    fn from(value: std_io::Error) -> Self {
        Self::Io(value)
    }
}

pub type PdfResult<T> = Result<T, PdfError>;

/// High-level PDF document handle.
#[derive(Debug, Clone)]
pub struct Document {
    source_len: usize,
}

impl Document {
    /// Loads a PDF document from disk.
    pub fn load<P: AsRef<Path>>(path: P) -> PdfResult<Self> {
        let file = File::open(path)?;
        Self::load_from_reader(file)
    }

    /// Loads a PDF document from any reader into an initial in-memory scaffold.
    pub fn load_from_reader<R: Read>(mut reader: R) -> PdfResult<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::load_from_bytes(&bytes)
    }

    /// Loads a PDF document from raw bytes.
    pub fn load_from_bytes(bytes: &[u8]) -> PdfResult<Self> {
        if !looks_like_pdf_header(bytes) {
            return Err(PdfError::Parse {
                offset: Some(0),
                context: "missing %PDF- header within first 1024 bytes",
            });
        }

        Ok(Self {
            source_len: bytes.len(),
        })
    }

    /// Returns the raw input length used for this scaffold.
    pub fn source_len(&self) -> usize {
        self.source_len
    }
}

fn looks_like_pdf_header(bytes: &[u8]) -> bool {
    const HEADER: &[u8] = b"%PDF-";
    const SEARCH_LIMIT: usize = 1024;

    if bytes.len() < HEADER.len() {
        return false;
    }

    let end = bytes.len().min(SEARCH_LIMIT);
    bytes[..end].windows(HEADER.len()).any(|w| w == HEADER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_bytes_with_pdf_header() {
        let doc = Document::load_from_bytes(b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n");
        assert!(doc.is_ok());
    }

    #[test]
    fn rejects_non_pdf_bytes() {
        let err = Document::load_from_bytes(b"not a pdf").unwrap_err();
        assert!(matches!(err, PdfError::Parse { .. }));
    }
}
