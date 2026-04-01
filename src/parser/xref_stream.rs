//! XRef stream support (PDF 1.5+).
//!
//! PDF §8.6 — XRef streams replace the traditional ASCII xref table with a
//! stream object that contains binary cross-reference data.
//!
//! Maps to Java PDFBox `PDFXRefStream`.

use crate::cos::{CosDictionary, CosName, CosObject, CosStream};

// ---------------------------------------------------------------------------
// XRef entry (subsection)
// ---------------------------------------------------------------------------

/// A single entry in an xref stream subsection.
///
/// PDF §8.6.2 — Each entry is W bytes wide, where W is defined by `/W` array.
/// Default W = [1, 2, 2] means: 1 byte type, 2 bytes field1, 2 bytes field2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XRefEntry {
    /// Type 0: Free object (field1 = next free object number, field2 = generation)
    Free { next: u32, generation: u16 },
    /// Type 1: In-use object (field1 = byte offset, field2 = generation)
    InUse { offset: u64, generation: u16 },
    /// Type 2: Compressed object (field1 = object stream number, field2 = index in stream)
    Compressed { stream: u32, index: u32 },
}

impl XRefEntry {
    /// Encode entry to bytes using the specified widths [type_width, field1_width, field2_width].
    pub fn to_bytes(&self, widths: &[u16; 3]) -> Vec<u8> {
        let mut buf = Vec::new();

        // Type byte
        let type_byte = match self {
            XRefEntry::Free { .. } => 0u8,
            XRefEntry::InUse { .. } => 1u8,
            XRefEntry::Compressed { .. } => 2u8,
        };
        for _ in 0..widths[0] {
            buf.push(type_byte);
        }

        // Field 1 (variable width)
        let field1 = match self {
            XRefEntry::Free { next, .. } => *next as u64,
            XRefEntry::InUse { offset, .. } => *offset,
            XRefEntry::Compressed { stream, .. } => *stream as u64,
        };
        for i in (0..widths[1]).rev() {
            buf.push(((field1 >> (i * 8)) & 0xFF) as u8);
        }

        // Field 2 (variable width)
        let field2 = match self {
            XRefEntry::Free { generation, .. } => *generation as u32,
            XRefEntry::InUse { generation, .. } => *generation as u32,
            XRefEntry::Compressed { index, .. } => *index,
        };
        for i in (0..widths[2]).rev() {
            buf.push(((field2 >> (i * 8)) & 0xFF) as u8);
        }

        buf
    }

    /// Parse entry from bytes using the specified widths.
    pub fn from_bytes(data: &[u8], widths: &[u16; 3]) -> Option<Self> {
        let mut pos = 0;

        // Parse type byte (read last byte if width > 1)
        if pos + widths[0] as usize > data.len() {
            return None;
        }
        let type_byte = data[pos + widths[0] as usize - 1];
        pos += widths[0] as usize;

        // Parse field1
        if pos + widths[1] as usize > data.len() {
            return None;
        }
        let mut field1 = 0u64;
        for i in 0..widths[1] as usize {
            field1 = (field1 << 8) | data[pos + i] as u64;
        }
        pos += widths[1] as usize;

        // Parse field2
        if pos + widths[2] as usize > data.len() {
            return None;
        }
        let mut field2 = 0u32;
        for i in 0..widths[2] as usize {
            field2 = (field2 << 8) | data[pos + i] as u32;
        }

        match type_byte {
            0 => Some(XRefEntry::Free { next: field1 as u32, generation: field2 as u16 }),
            1 => Some(XRefEntry::InUse { offset: field1, generation: field2 as u16 }),
            2 => Some(XRefEntry::Compressed { stream: field1 as u32, index: field2 }),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// XRef stream subsection
// ---------------------------------------------------------------------------

/// A subsection of an xref stream (contiguous object numbers).
///
/// PDF §8.6.2 — subsections are [start_obj_num, count, ...entries]
#[derive(Debug, Clone)]
pub struct XRefSubsection {
    /// Starting object number
    pub start: u32,
    /// Number of entries
    pub count: u32,
    /// Entries in order
    pub entries: Vec<XRefEntry>,
}

impl XRefSubsection {
    pub fn new(start: u32) -> Self {
        Self { start, count: 0, entries: Vec::new() }
    }

    pub fn add_entry(&mut self, entry: XRefEntry) {
        self.entries.push(entry);
        self.count = self.entries.len() as u32;
    }

    /// Serialize subsection to binary format.
    pub fn to_bytes(&self, widths: &[u16; 3]) -> Vec<u8> {
        let mut buf = Vec::new();
        // Entries are written in order; start/count are in the /Index array
        for entry in &self.entries {
            buf.extend(entry.to_bytes(widths));
        }
        buf
    }
}

// ---------------------------------------------------------------------------
// XRef stream
// ---------------------------------------------------------------------------

/// A parsed XRef stream (PDF 1.5+).
///
/// Replaces traditional ASCII xref tables with a stream object.
/// Maps to Java PDFBox `PDFXRefStream`.
#[derive(Debug, Clone)]
pub struct XRefStream {
    /// Size of the xref (highest object number + 1)
    pub size: u32,
    /// Width array [type_width, field1_width, field2_width]
    pub widths: [u16; 3],
    /// Subsections: object ranges and entries
    pub subsections: Vec<XRefSubsection>,
    /// Root object reference (optional)
    pub root: Option<u32>,
    /// Info object reference (optional)
    pub info: Option<u32>,
    /// Prev xref offset (for incremental updates)
    pub prev: Option<u64>,
}

impl XRefStream {
    /// Create a new XRef stream with default widths.
    pub fn new(size: u32) -> Self {
        Self {
            size,
            widths: [1, 2, 2],
            subsections: Vec::new(),
            root: None,
            info: None,
            prev: None,
        }
    }

    /// Parse an XRef stream from a dictionary and data stream.
    ///
    /// Returns `None` if the stream is malformed.
    pub fn from_stream(dict: &CosDictionary, data: &[u8]) -> Option<Self> {
        // /Size (required)
        let size = dict.get_int(&CosName::new(b"Size".to_vec()))? as u32;

        // /W (required) — widths array
        let w_arr = dict
            .get_array(&CosName::new(b"W".to_vec()))
            .and_then(|arr| {
                if arr.len() != 3 {
                    return None;
                }
                Some([
                    arr[0].as_integer()? as u16,
                    arr[1].as_integer()? as u16,
                    arr[2].as_integer()? as u16,
                ])
            })?;

        // /Index (optional) — subsection ranges
        let indices = dict
            .get_array(&CosName::new(b"Index".to_vec()))
            .map(|arr| {
                (0..arr.len())
                    .step_by(2)
                    .filter_map(|i| {
                        if i + 1 < arr.len() {
                            Some((
                                arr[i].as_integer()? as u32,
                                arr[i + 1].as_integer()? as u32,
                            ))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![(0, size)]);

        // Parse subsections
        let mut subsections = Vec::new();
        let mut pos = 0;
        let entry_width = (w_arr[0] + w_arr[1] + w_arr[2]) as usize;

        for (start, count) in indices {
            let mut subsec = XRefSubsection::new(start);
            for _ in 0..count {
                if pos + entry_width > data.len() {
                    return None;
                }
                let entry = XRefEntry::from_bytes(&data[pos..], &w_arr)?;
                subsec.add_entry(entry);
                pos += entry_width;
            }
            subsections.push(subsec);
        }

        // Optional fields
        let root = dict
            .get(&CosName::new(b"Root".to_vec()))
            .and_then(|v| v.as_reference())
            .map(|id| id.object_number);

        let info = dict
            .get(&CosName::new(b"Info".to_vec()))
            .and_then(|v| v.as_reference())
            .map(|id| id.object_number);

        let prev = dict
            .get_int(&CosName::new(b"Prev".to_vec()))
            .map(|n| n as u64);

        Some(Self { size, widths: w_arr, subsections, root, info, prev })
    }

    /// Serialize to a stream object.
    pub fn to_stream(&self) -> CosStream {
        let mut dict = CosDictionary::new();
        dict.set(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"XRef".to_vec())));
        dict.set(CosName::new(b"Size".to_vec()), CosObject::Integer(self.size as i64));

        // /W array
        dict.set(
            CosName::new(b"W".to_vec()),
            CosObject::Array(vec![
                CosObject::Integer(self.widths[0] as i64),
                CosObject::Integer(self.widths[1] as i64),
                CosObject::Integer(self.widths[2] as i64),
            ]),
        );

        // /Index array (if non-default)
        if self.subsections.len() != 1 || self.subsections[0].start != 0 {
            let mut index = Vec::new();
            for subsec in &self.subsections {
                index.push(CosObject::Integer(subsec.start as i64));
                index.push(CosObject::Integer(subsec.count as i64));
            }
            dict.set(CosName::new(b"Index".to_vec()), CosObject::Array(index));
        }

        // Optional fields
        if let Some(root) = self.root {
            dict.set(
                CosName::new(b"Root".to_vec()),
                CosObject::Reference(crate::cos::ObjectId::new(root, 0)),
            );
        }
        if let Some(info) = self.info {
            dict.set(
                CosName::new(b"Info".to_vec()),
                CosObject::Reference(crate::cos::ObjectId::new(info, 0)),
            );
        }
        if let Some(prev) = self.prev {
            dict.set(CosName::new(b"Prev".to_vec()), CosObject::Integer(prev as i64));
        }

        // Serialize subsections to data
        let mut data = Vec::new();
        for subsec in &self.subsections {
            data.extend(subsec.to_bytes(&self.widths));
        }

        CosStream::new(dict, data)
    }

    /// Look up an entry by object number.
    pub fn lookup(&self, obj_num: u32) -> Option<XRefEntry> {
        for subsec in &self.subsections {
            if obj_num >= subsec.start && obj_num < subsec.start + subsec.count {
                let idx = (obj_num - subsec.start) as usize;
                return subsec.entries.get(idx).copied();
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xref_entry_in_use_serialization() {
        let entry = XRefEntry::InUse { offset: 1234, generation: 0 };
        let widths = [1, 2, 2];
        let bytes = entry.to_bytes(&widths);
        assert_eq!(bytes.len(), 5);
        assert_eq!(bytes[0], 1); // type
    }

    #[test]
    fn xref_entry_free_serialization() {
        let entry = XRefEntry::Free { next: 99, generation: 5 };
        let widths = [1, 2, 2];
        let bytes = entry.to_bytes(&widths);
        assert_eq!(bytes[0], 0); // type 0
    }

    #[test]
    fn xref_entry_compressed_serialization() {
        let entry = XRefEntry::Compressed { stream: 10, index: 5 };
        let widths = [1, 2, 2];
        let bytes = entry.to_bytes(&widths);
        assert_eq!(bytes[0], 2); // type 2
    }

    #[test]
    fn xref_entry_roundtrip() {
        let entry = XRefEntry::InUse { offset: 5678, generation: 1 };
        let widths = [1, 2, 2];
        let bytes = entry.to_bytes(&widths);
        let parsed = XRefEntry::from_bytes(&bytes, &widths).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn xref_subsection_creation() {
        let mut subsec = XRefSubsection::new(0);
        subsec.add_entry(XRefEntry::InUse { offset: 100, generation: 0 });
        subsec.add_entry(XRefEntry::InUse { offset: 200, generation: 0 });
        assert_eq!(subsec.count, 2);
        assert_eq!(subsec.entries.len(), 2);
    }

    #[test]
    fn xref_stream_creation() {
        let xref = XRefStream::new(10);
        assert_eq!(xref.size, 10);
        assert_eq!(xref.widths, [1, 2, 2]);
    }

    #[test]
    fn xref_stream_lookup() {
        let mut xref = XRefStream::new(10);
        let mut subsec = XRefSubsection::new(0);
        subsec.add_entry(XRefEntry::InUse { offset: 100, generation: 0 });
        subsec.add_entry(XRefEntry::InUse { offset: 200, generation: 0 });
        xref.subsections.push(subsec);

        let entry = xref.lookup(0).unwrap();
        assert!(matches!(entry, XRefEntry::InUse { offset: 100, .. }));

        let entry = xref.lookup(1).unwrap();
        assert!(matches!(entry, XRefEntry::InUse { offset: 200, .. }));

        assert!(xref.lookup(99).is_none());
    }

    #[test]
    fn xref_entry_width_edge_cases() {
        // Test with different width combinations
        let entry = XRefEntry::InUse { offset: 0xFFFFFFFF, generation: 0xFFFF };
        let widths = [1, 4, 2]; // Allow large offset
        let bytes = entry.to_bytes(&widths);
        assert_eq!(bytes.len(), 7);
    }

    #[test]
    fn xref_entry_free_next_object() {
        let entry = XRefEntry::Free { next: 5, generation: 65535 };
        let widths = [1, 2, 2];
        let bytes = entry.to_bytes(&widths);
        let parsed = XRefEntry::from_bytes(&bytes, &widths).unwrap();
        assert!(matches!(parsed, XRefEntry::Free { next: 5, generation: 65535 }));
    }

    #[test]
    fn xref_compressed_object_reference() {
        let entry = XRefEntry::Compressed { stream: 20, index: 7 };
        let widths = [1, 2, 2];
        let bytes = entry.to_bytes(&widths);
        let parsed = XRefEntry::from_bytes(&bytes, &widths).unwrap();
        assert!(matches!(parsed, XRefEntry::Compressed { stream: 20, index: 7 }));
    }
}

