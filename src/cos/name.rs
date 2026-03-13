//! PDF Name object.
//!
//! Maps to Java PDFBox `COSName`. Names are interned byte sequences
//! prefixed by `/` in PDF syntax (e.g. `/Type`, `/Page`).

use std::fmt;

/// A PDF name object, stored as the decoded byte sequence (without the leading `/`).
///
/// In PDF, names like `/Type` or `/Pages` are identifiers used as
/// dictionary keys. This type stores the *decoded* form — `#XX` hex
/// escapes are resolved during parsing before constructing a `CosName`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CosName(Vec<u8>);

impl CosName {
    /// Creates a name from already-decoded bytes (no leading `/`).
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Returns the raw decoded bytes of this name.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Interprets the name bytes as UTF-8 if valid.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }

    // ----- Well-known names (commonly used dictionary keys) -----

    pub fn type_name() -> Self {
        Self::new(b"Type".to_vec())
    }

    pub fn pages() -> Self {
        Self::new(b"Pages".to_vec())
    }

    pub fn page() -> Self {
        Self::new(b"Page".to_vec())
    }

    pub fn kids() -> Self {
        Self::new(b"Kids".to_vec())
    }

    pub fn count() -> Self {
        Self::new(b"Count".to_vec())
    }

    pub fn catalog() -> Self {
        Self::new(b"Catalog".to_vec())
    }

    pub fn contents() -> Self {
        Self::new(b"Contents".to_vec())
    }

    pub fn resources() -> Self {
        Self::new(b"Resources".to_vec())
    }

    pub fn length() -> Self {
        Self::new(b"Length".to_vec())
    }

    pub fn filter() -> Self {
        Self::new(b"Filter".to_vec())
    }

    pub fn size() -> Self {
        Self::new(b"Size".to_vec())
    }

    pub fn root() -> Self {
        Self::new(b"Root".to_vec())
    }
}

impl fmt::Display for CosName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display with leading `/`, encoding non-printable bytes as `#XX`.
        write!(f, "/")?;
        for &b in &self.0 {
            if b == b'#' {
                write!(f, "#23")?;
            } else if b.is_ascii_graphic() && b != b'/' {
                write!(f, "{}", b as char)?;
            } else if b == b'/' {
                write!(f, "#2F")?;
            } else {
                write!(f, "#{:02X}", b)?;
            }
        }
        Ok(())
    }
}

impl<T: AsRef<[u8]>> From<T> for CosName {
    fn from(value: T) -> Self {
        Self::new(value.as_ref().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_simple() {
        let name = CosName::new(b"Type".to_vec());
        assert_eq!(name.to_string(), "/Type");
    }

    #[test]
    fn display_with_special_chars() {
        let name = CosName::new(b"A/B".to_vec());
        assert_eq!(name.to_string(), "/A#2FB");
    }

    #[test]
    fn as_str_valid_utf8() {
        let name = CosName::new(b"Pages".to_vec());
        assert_eq!(name.as_str(), Some("Pages"));
    }

    #[test]
    fn as_str_invalid_utf8() {
        let name = CosName::new(vec![0xFF, 0xFE]);
        assert!(name.as_str().is_none());
    }

    #[test]
    fn equality() {
        let a = CosName::new(b"Type".to_vec());
        let b = CosName::type_name();
        assert_eq!(a, b);
    }

    #[test]
    fn from_str_slice() {
        let name: CosName = "Page".into();
        assert_eq!(name.as_bytes(), b"Page");
    }

    #[test]
    fn well_known_names() {
        assert_eq!(CosName::root().as_str(), Some("Root"));
        assert_eq!(CosName::kids().as_str(), Some("Kids"));
        assert_eq!(CosName::count().as_str(), Some("Count"));
    }
}

