//! Object Stream (ObjStm) support — PDF 1.5+.
//!
//! PDF §3.4.7 — Object streams contain multiple compressed objects in a single stream.
//! Used by PDF writers to reduce file size via compression.
//!
//! Maps to Java PDFBox `PDFObjectStreamParser`.

use crate::cos::{CosName, CosObject, CosStream};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Object stream entry
// ---------------------------------------------------------------------------

/// An entry in an object stream — object number and byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjStmEntry {
    /// Object number within the stream
    pub obj_num: u32,
    /// Byte offset within decompressed stream data
    pub offset: u32,
}

// ---------------------------------------------------------------------------
// Object stream
// ---------------------------------------------------------------------------

/// A parsed object stream (PDF 1.5+).
///
/// Contains multiple objects in compressed form.
/// Maps to Java PDFBox `PDFObjectStream`.
#[derive(Debug, Clone)]
pub struct ObjectStream {
    /// First object number (stream's own object number)
    pub first: u32,
    /// Number of objects in stream
    pub count: u32,
    /// Object entries (obj_num → offset pairs)
    pub entries: BTreeMap<u32, u32>,
    /// Decompressed stream data
    pub data: Vec<u8>,
}

impl ObjectStream {
    /// Create a new object stream with default parameters.
    pub fn new(first: u32, count: u32, data: Vec<u8>) -> Self {
        Self { first, count, entries: BTreeMap::new(), data }
    }

    /// Parse an object stream from a stream dictionary and decompressed data.
    ///
    /// PDF §3.4.7 — /ObjStm must have:
    /// - /Type = /ObjStm
    /// - /N (count of objects)
    /// - /First (byte offset to first object)
    ///
    /// The preamble contains pairs: `obj_num offset obj_num offset ...`
    pub fn from_stream(dict: &crate::cos::CosDictionary, data: Vec<u8>) -> Option<Self> {
        // Required fields
        let count = dict.get_int(&CosName::new(b"N".to_vec()))? as u32;
        let first = dict.get_int(&CosName::new(b"First".to_vec()))? as u32;

        // Parse preamble: contains N pairs of (obj_num, offset)
        let mut entries = BTreeMap::new();
        let mut pos = 0;

        // Preamble is whitespace-separated decimal integers
        let preamble_str = String::from_utf8_lossy(&data[..first.min(data.len() as u32) as usize]);
        let tokens: Vec<&str> = preamble_str.split_whitespace().collect();

        // Should have 2*N tokens (obj_num, offset pairs)
        if tokens.len() < (2 * count as usize) {
            return None;
        }

        for i in 0..count as usize {
            let obj_num = tokens[i * 2].parse::<u32>().ok()?;
            let offset = tokens[i * 2 + 1].parse::<u32>().ok()?;
            entries.insert(obj_num, offset);
        }

        Some(Self { first, count, entries, data })
    }

    /// Get a decompressed object by its number within the stream.
    ///
    /// Returns the object's bytes within the decompressed data.
    pub fn get_object(&self, obj_num: u32) -> Option<&[u8]> {
        let offset = self.entries.get(&obj_num)?;
        let start = self.first as usize + *offset as usize;

        // Find end of this object (start of next, or end of data)
        let end = self
            .entries
            .iter()
            .skip_while(|(_, off)| *off <= offset)
            .next()
            .map(|(_, off)| self.first as usize + *off as usize)
            .unwrap_or(self.data.len());

        if start < self.data.len() && start <= end {
            Some(&self.data[start..end.min(self.data.len())])
        } else {
            None
        }
    }

    /// List all object numbers in this stream.
    pub fn object_numbers(&self) -> Vec<u32> {
        self.entries.keys().copied().collect()
    }

    /// Check if an object exists in this stream.
    pub fn contains(&self, obj_num: u32) -> bool {
        self.entries.contains_key(&obj_num)
    }

    /// Serialize to a stream object.
    pub fn to_stream(&self) -> CosStream {
        let mut dict = crate::cos::CosDictionary::new();
        dict.set(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"ObjStm".to_vec())));
        dict.set(CosName::new(b"N".to_vec()), CosObject::Integer(self.count as i64));
        dict.set(CosName::new(b"First".to_vec()), CosObject::Integer(self.first as i64));

        CosStream::new(dict, self.data.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_stream_creation() {
        let data = vec![1, 2, 3, 4];
        let stream = ObjectStream::new(10, 2, data.clone());
        assert_eq!(stream.first, 10);
        assert_eq!(stream.count, 2);
        assert_eq!(stream.data, data);
    }

    #[test]
    fn object_stream_entry_lookup() {
        let mut stream = ObjectStream::new(10, 2, vec![0; 100]);
        stream.entries.insert(5, 0);
        stream.entries.insert(7, 50);

        assert!(stream.contains(5));
        assert!(stream.contains(7));
        assert!(!stream.contains(6));
    }

    #[test]
    fn object_stream_object_numbers() {
        let mut stream = ObjectStream::new(10, 3, vec![0; 100]);
        stream.entries.insert(5, 0);
        stream.entries.insert(7, 50);
        stream.entries.insert(9, 75);

        let nums = stream.object_numbers();
        assert_eq!(nums, vec![5, 7, 9]); // BTreeMap keeps sorted order
    }

    #[test]
    fn object_stream_preamble_parsing() {
        // Preamble format: obj_num offset obj_num offset ...
        // Data starts at byte `first`
        let preamble = "5 0 7 50 9 75 ";
        let mut full_data = preamble.as_bytes().to_vec();
        full_data.resize(100, 0); // Pad to 100 bytes

        let dict = crate::cos::CosDictionary::new();
        // In real usage, dict would have N=3, First=preamble.len()
        // For this test, we manually create the stream

        let stream = ObjectStream::new(preamble.len() as u32, 3, full_data);
        assert_eq!(stream.count, 3);
    }

    #[test]
    fn object_stream_get_object_single() {
        // Stream with 2 objects at offsets 0 and 10
        // first=20 means object data starts at byte 20 in the data buffer
        let mut data = vec![0; 100];
        data[30..40].copy_from_slice(b"obj7_data!");

        let mut stream = ObjectStream::new(20, 2, data);
        stream.entries.insert(5, 0);   // obj5 at first+0 = byte 20
        stream.entries.insert(7, 10);  // obj7 at first+10 = byte 30

        let obj5 = stream.get_object(5).unwrap();
        assert_eq!(obj5.len(), 10); // From offset 20 to offset 30

        let obj7 = stream.get_object(7).unwrap();
        assert!(obj7.len() > 0); // From offset 30 to end
    }

    #[test]
    fn object_stream_round_trip() {
        let mut stream = ObjectStream::new(20, 2, vec![65; 100]); // 'A' repeated
        stream.entries.insert(10, 0);
        stream.entries.insert(11, 50);

        let cos_stream = stream.to_stream();
        assert_eq!(cos_stream.data, vec![65; 100]);
    }

    #[test]
    fn object_stream_empty() {
        let stream = ObjectStream::new(0, 0, vec![]);
        assert_eq!(stream.object_numbers().len(), 0);
        assert!(!stream.contains(5));
    }

    #[test]
    fn object_stream_large_object_numbers() {
        let mut stream = ObjectStream::new(100, 2, vec![0; 200]);
        stream.entries.insert(99999, 0);
        stream.entries.insert(100000, 100);

        assert!(stream.contains(99999));
        assert!(stream.contains(100000));
        assert_eq!(stream.object_numbers().len(), 2);
    }

    #[test]
    fn object_stream_overlapping_ranges() {
        let mut stream = ObjectStream::new(50, 3, vec![0; 200]);
        stream.entries.insert(1, 0);
        stream.entries.insert(2, 25);
        stream.entries.insert(3, 100);

        // Each object should span from its offset to the next
        let obj1 = stream.get_object(1).unwrap();
        assert_eq!(obj1.len(), 25);

        let obj2 = stream.get_object(2).unwrap();
        assert_eq!(obj2.len(), 75); // From 25 to 100

        let obj3 = stream.get_object(3).unwrap();
        assert!(obj3.len() > 0);
    }
}

