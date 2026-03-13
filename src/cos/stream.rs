//! PDF stream object.
//!
//! Maps to Java PDFBox `COSStream`. A stream is a dictionary plus
//! an associated sequence of bytes (the stream data). Filters like
//! FlateDecode are recorded in the dictionary and applied lazily.

use std::fmt;

use super::dictionary::CosDictionary;

/// A PDF stream: a dictionary of metadata plus raw (possibly encoded) bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct CosStream {
    /// The stream's associated dictionary (contains `/Length`, `/Filter`, etc.).
    pub dictionary: CosDictionary,
    /// The raw (encoded) stream data bytes as stored in the PDF file.
    /// Decoding (e.g. FlateDecode) is performed on demand by consumers.
    pub data: Vec<u8>,
}

impl CosStream {
    /// Creates a new stream with the given dictionary and raw data.
    pub fn new(dictionary: CosDictionary, data: Vec<u8>) -> Self {
        Self { dictionary, data }
    }

    /// Creates an empty stream with an empty dictionary.
    pub fn empty() -> Self {
        Self {
            dictionary: CosDictionary::new(),
            data: Vec::new(),
        }
    }

    /// Returns the raw (encoded) data length.
    #[inline]
    pub fn raw_len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the raw data is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl fmt::Display for CosStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}\nstream\n[{} bytes]\nendstream",
            self.dictionary,
            self.data.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::name::CosName;
    use crate::cos::object::CosObject;

    #[test]
    fn empty_stream() {
        let s = CosStream::empty();
        assert!(s.is_empty());
        assert_eq!(s.raw_len(), 0);
        assert!(s.dictionary.is_empty());
    }

    #[test]
    fn stream_with_data() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::length(), CosObject::Integer(5));
        let s = CosStream::new(dict, b"hello".to_vec());
        assert_eq!(s.raw_len(), 5);
        assert!(!s.is_empty());
    }

    #[test]
    fn display_format() {
        let s = CosStream::new(CosDictionary::new(), b"abc".to_vec());
        let display = s.to_string();
        assert!(display.contains("stream"));
        assert!(display.contains("3 bytes"));
        assert!(display.contains("endstream"));
    }
}

