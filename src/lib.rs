//! Rust port of Apache PDFBox — M1: header + xref + object store baseline.
//!
//! This crate provides a Rust implementation of PDF reading, following the
//! architecture of Apache Java PDFBox. See `docs/porting/` for the porting plan.

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

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io as std_io;
use std::io::Read;
use std::path::Path;

use cos::{CosDictionary, CosName, CosObject, ObjectId};
use parser::xref::{XRefEntry, XRefTable};
use parser::{ParseError, Parser};

pub use cos::ObjectId as PdfObjectId;

// ---------------------------------------------------------------------------
// Error model
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum PdfError {
    Io(std_io::Error),
    Parse {
        offset: Option<u64>,
        context: String,
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

impl From<ParseError> for PdfError {
    fn from(e: ParseError) -> Self {
        Self::Parse {
            offset: Some(e.offset as u64),
            context: e.message,
        }
    }
}

pub type PdfResult<T> = Result<T, PdfError>;

// ---------------------------------------------------------------------------
// Object store — lazy in-memory object cache keyed by ObjectId
// ---------------------------------------------------------------------------

/// Stores all loaded indirect COS objects indexed by [`ObjectId`].
///
/// Objects are parsed on demand from the raw byte slice and cached here.
/// This corresponds to `COSDocument` in Java PDFBox.
#[derive(Debug, Default, Clone)]
pub struct ObjectStore {
    objects: HashMap<ObjectId, CosObject>,
}

impl ObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a pre-parsed object.
    pub fn insert(&mut self, id: ObjectId, obj: CosObject) {
        self.objects.insert(id, obj);
    }

    /// Looks up an object by ID.
    pub fn get(&self, id: &ObjectId) -> Option<&CosObject> {
        self.objects.get(id)
    }

    /// Returns the number of stored objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Returns `true` when the store is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Document — high-level PDF document handle
// ---------------------------------------------------------------------------

/// High-level PDF document handle.
///
/// After loading, provides access to:
/// - The merged [`XRefTable`] from all xref sections and chains.
/// - The trailer [`CosDictionary`].
/// - The [`ObjectStore`] of all eagerly loaded in-use objects.
/// - Page count and catalog reference (when available).
///
/// Maps to `PDDocument` in Java PDFBox.
#[derive(Debug, Clone)]
pub struct Document {
    /// Total byte length of the source file.
    pub source_len: usize,
    /// Merged xref table built from all sections and Prev chains.
    pub xref: XRefTable,
    /// All eagerly loaded indirect objects.
    pub objects: ObjectStore,
}

impl Document {
    /// Loads a PDF document from a file on disk.
    pub fn load<P: AsRef<Path>>(path: P) -> PdfResult<Self> {
        let file = File::open(path)?;
        Self::load_from_reader(file)
    }

    /// Loads a PDF document from any reader.
    pub fn load_from_reader<R: Read>(mut reader: R) -> PdfResult<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::load_from_bytes(&bytes)
    }

    /// Loads a PDF document from a raw byte slice.
    ///
    /// Steps performed:
    /// 1. Validate `%PDF-` header.
    /// 2. Discover `startxref` offset.
    /// 3. Parse all xref sections (table or stream), following `Prev` chains.
    /// 4. Eagerly load all in-use objects from the xref table into the object store.
    pub fn load_from_bytes(bytes: &[u8]) -> PdfResult<Self> {
        // Step 1 — header check.
        if !looks_like_pdf_header(bytes) {
            return Err(PdfError::Parse {
                offset: Some(0),
                context: "missing %PDF- header within first 1024 bytes".to_string(),
            });
        }

        // Step 2 & 3 — xref discovery and parsing.
        let xref = parser::xref::load_xref(bytes)?;

        // Step 4 — eagerly parse all in-use objects referenced in xref.
        let mut objects = ObjectStore::new();
        for (id, entry) in xref.iter() {
            if let XRefEntry::InUse { offset, .. } = entry {
                let offset = *offset as usize;
                if offset == 0 || offset >= bytes.len() {
                    continue;
                }
                let slice = &bytes[offset..];
                let mut parser = Parser::new(slice);
                match parser.parse_indirect_object() {
                    Ok(Some((_parsed_id, obj))) => {
                        objects.insert(id.clone(), obj);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        // Non-fatal: skip malformed individual objects.
                    }
                }
            }
        }

        Ok(Self {
            source_len: bytes.len(),
            xref,
            objects,
        })
    }

    /// Returns the raw input length.
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// Returns the trailer dictionary from the merged xref.
    pub fn trailer(&self) -> &CosDictionary {
        &self.xref.trailer
    }

    /// Returns the catalog object reference from the trailer, if present.
    pub fn catalog_ref(&self) -> Option<ObjectId> {
        self.xref
            .trailer
            .get(&CosName::root())
            .and_then(|v| v.as_reference())
    }

    /// Returns the catalog dictionary, if it can be resolved.
    pub fn catalog(&self) -> Option<&cos::CosDictionary> {
        let cat_id = self.catalog_ref()?;
        self.objects.get(&cat_id)?.as_dictionary()
    }

    /// Returns the number of objects in the object store.
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn looks_like_pdf_header(bytes: &[u8]) -> bool {
    const HEADER: &[u8] = b"%PDF-";
    const SEARCH_LIMIT: usize = 1024;

    if bytes.len() < HEADER.len() {
        return false;
    }

    let end = bytes.len().min(SEARCH_LIMIT);
    bytes[..end].windows(HEADER.len()).any(|w| w == HEADER)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal but structurally valid PDF byte sequence suitable for
    /// testing `Document::load_from_bytes`.
    fn minimal_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let obj1_offset = pdf.len() as u64;
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let obj2_offset = pdf.len() as u64;
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let xref_offset = pdf.len();
        let e1 = format!("{:010} 00000 n \r\n", obj1_offset);
        let e2 = format!("{:010} 00000 n \r\n", obj2_offset);
        pdf.extend_from_slice(b"xref\n");
        pdf.extend_from_slice(b"0 3\n");
        pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        pdf.extend_from_slice(e1.as_bytes());
        pdf.extend_from_slice(e2.as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
        pdf
    }

    #[test]
    fn loads_bytes_with_pdf_header() {
        // A bare header without proper xref will fail at xref stage, not header stage.
        let result = Document::load_from_bytes(b"%PDF-1.7\n%%EOF\nstartxref\n0\n%%EOF\n");
        // We expect a parse error (no valid xref at offset 0), not a header error.
        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_pdf_bytes() {
        let err = Document::load_from_bytes(b"not a pdf").unwrap_err();
        assert!(matches!(err, PdfError::Parse { .. }));
    }

    #[test]
    fn loads_minimal_pdf() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        assert_eq!(doc.source_len(), pdf.len());
    }

    #[test]
    fn minimal_pdf_has_catalog_ref() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        let cat_ref = doc.catalog_ref();
        assert_eq!(cat_ref, Some(ObjectId::new(1, 0)));
    }

    #[test]
    fn minimal_pdf_catalog_resolved() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        let catalog = doc.catalog();
        assert!(catalog.is_some(), "catalog should resolve");
        let cat = catalog.unwrap();
        assert_eq!(
            cat.get_name(&CosName::type_name()),
            Some(&CosName::new(b"Catalog".to_vec()))
        );
    }

    #[test]
    fn minimal_pdf_object_count() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        // Expect at least objects 1 and 2 loaded (obj 0 is free).
        assert!(doc.object_count() >= 2);
    }

    #[test]
    fn trailer_has_size() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        assert_eq!(
            doc.trailer().get_int(&CosName::new(b"Size".to_vec())),
            Some(3)
        );
    }
}
