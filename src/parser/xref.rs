//! XRef table and XRef stream parsing, plus `startxref` discovery.
//!
//! Maps to Java PDFBox `PDFXRefStream`, `PDFXref`, and the xref-reading
//! portions of `COSParser` / `PDFParser`.
//!
//! # Responsibilities
//!
//! - Scan from end-of-file to locate `startxref` offset.
//! - Parse a traditional cross-reference table (`xref` + `trailer`).
//! - Parse a cross-reference stream (PDF 1.5+).
//! - Follow `Prev` chains to build the merged xref map.
//! - Expose the final merged [`XRefTable`] and trailer [`CosDictionary`].
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type / function |
//! |---|---|
//! | `PDFXref` (entry) | [`XRefEntry`] |
//! | `COSParser.parseXref` | [`parse_xref_table`] |
//! | `PDFXRefStream` | [`parse_xref_stream`] |
//! | `COSParser.getStartXref` | [`find_startxref`] |
//! | merged table | [`XRefTable`] |

use std::collections::HashMap;

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};

use super::{ParseError, Parser};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single entry in the cross-reference table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XRefEntry {
    /// A free (deleted) object.
    Free {
        /// Object number of the next free object in the chain.
        next_free: u32,
        /// Generation number to use when this object is reused.
        generation: u16,
    },
    /// An object at a fixed byte offset in the file.
    InUse {
        /// Byte offset from the beginning of the PDF file.
        offset: u64,
        /// Generation number.
        generation: u16,
    },
    /// An object embedded inside an object stream (PDF 1.5+).
    Compressed {
        /// Object number of the object stream that contains this object.
        stream_object_number: u32,
        /// Index of this object within that object stream.
        index_in_stream: u32,
    },
}

/// The merged cross-reference map built from one or more xref sections.
///
/// The outermost (most recent) xref takes precedence for any given object ID,
/// consistent with the PDF incremental-update model.
#[derive(Debug, Default, Clone)]
pub struct XRefTable {
    entries: HashMap<ObjectId, XRefEntry>,
    /// The merged trailer dictionary (latest update wins per key).
    pub trailer: CosDictionary,
    /// PDF header version as `(major, minor)`, e.g. `Some((1, 7))`.
    /// Set from the `%PDF-M.m` header during loading; overridable for downgrade.
    pub pdf_version: Option<(u8, u8)>,
}

impl XRefTable {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts an entry. Earlier entries (higher-priority) are NOT overwritten.
    pub fn insert_if_absent(&mut self, id: ObjectId, entry: XRefEntry) {
        self.entries.entry(id).or_insert(entry);
    }

    /// Looks up an xref entry by object ID.
    pub fn get(&self, id: &ObjectId) -> Option<&XRefEntry> {
        self.entries.get(id)
    }

    /// Returns the number of known objects.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no entries are recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Merges the trailer dictionary, inserting keys that do not yet exist.
    pub fn merge_trailer(&mut self, trailer: &CosDictionary) {
        for (key, value) in trailer.iter() {
            if !self.trailer.contains_key(key) {
                self.trailer.insert(key.clone(), value.clone());
            }
        }
    }

    /// Iterates all entries.
    pub fn iter(&self) -> impl Iterator<Item = (&ObjectId, &XRefEntry)> {
        self.entries.iter()
    }
}

// ---------------------------------------------------------------------------
// `startxref` discovery
// ---------------------------------------------------------------------------

/// Scans the tail of `data` (up to `tail_size` bytes) backwards to find the
/// `startxref` keyword and returns the byte offset it names.
///
/// Corresponds to `COSParser.getStartXref()` in Java PDFBox.
pub fn find_startxref(data: &[u8], tail_size: usize) -> Result<u64, ParseError> {
    let search_start = data.len().saturating_sub(tail_size);
    let tail = &data[search_start..];

    // Search backwards for "startxref"
    const MARKER: &[u8] = b"startxref";
    let marker_pos = tail
        .windows(MARKER.len())
        .rposition(|w| w == MARKER)
        .ok_or_else(|| ParseError::new("'startxref' keyword not found in tail", data.len()))?;

    // Skip past "startxref" and any whitespace / EOL.
    let after = &tail[marker_pos + MARKER.len()..];
    let trimmed = skip_whitespace_bytes(after);

    // Read the integer offset.
    let (num_slice, _) = read_ascii_integer_bytes(trimmed)
        .ok_or_else(|| ParseError::new("invalid startxref offset", search_start + marker_pos))?;

    let offset_str = std::str::from_utf8(num_slice)
        .map_err(|_| ParseError::new("non-UTF8 startxref offset", 0))?;
    let offset: u64 = offset_str
        .parse()
        .map_err(|_| ParseError::new("startxref offset not a valid integer", 0))?;

    Ok(offset)
}

// ---------------------------------------------------------------------------
// Traditional XRef table parsing
// ---------------------------------------------------------------------------

/// Parses a traditional `xref` table starting at `offset` within `data`.
///
/// Returns the entries found **in this section only** and the trailer dictionary.
/// Follows `Prev` chains automatically, merging earlier sections into `table`.
///
/// Corresponds to `COSParser.parseXref` in Java PDFBox.
pub fn parse_xref_table(
    data: &[u8],
    offset: u64,
    table: &mut XRefTable,
) -> Result<(), ParseError> {
    let start = offset as usize;
    if start >= data.len() {
        return Err(ParseError::new(
            format!("xref offset {offset} is past end of file ({} bytes)", data.len()),
            start,
        ));
    }

    let slice = &data[start..];

    // The xref section can be either a traditional table or an xref stream.
    // Detect which by looking at the first token.
    let first_nonws = skip_whitespace_bytes(slice);
    if first_nonws.starts_with(b"xref") {
        parse_xref_keyword_table(data, start, table)?;
    } else {
        // Probably an xref stream — parse with the object parser.
        parse_xref_stream(data, start, table)?;
    }

    Ok(())
}

/// Parses the traditional `xref … trailer …` form.
fn parse_xref_keyword_table(
    data: &[u8],
    start: usize,
    table: &mut XRefTable,
) -> Result<(), ParseError> {
    let slice = &data[start..];
    let mut pos = 0usize;

    // Consume "xref" keyword.
    pos = skip_ws_at(slice, pos);
    if !slice[pos..].starts_with(b"xref") {
        return Err(ParseError::new("expected 'xref' keyword", start + pos));
    }
    pos += 4;

    // Parse subsections: <first_obj> <count> followed by `count` 20-byte entries.
    loop {
        pos = skip_ws_at(slice, pos);

        // Check if we've reached the trailer.
        if slice[pos..].starts_with(b"trailer") {
            pos += 7;
            break;
        }
        if pos >= slice.len() {
            return Err(ParseError::new(
                "unexpected EOF in xref table, missing 'trailer'",
                start + pos,
            ));
        }

        // Read first_obj and count.
        let (first_obj, consumed) = read_ascii_u32(&slice[pos..])
            .ok_or_else(|| ParseError::new("expected xref subsection first obj", start + pos))?;
        pos += consumed;

        pos = skip_ws_at(slice, pos);

        let (count, consumed) = read_ascii_u32(&slice[pos..])
            .ok_or_else(|| ParseError::new("expected xref subsection count", start + pos))?;
        pos += consumed;

        // Skip EOL after header line.
        pos = skip_eol(slice, pos);

        // Each entry per PDF spec is exactly 20 bytes, but some writers emit 21.
        // Layout (20-byte): "oooooooooo ggggg X\r\n" or "oooooooooo ggggg X \n"
        //   where X ('n'/'f') is at byte 17.
        // Layout (21-byte, common in practice): "oooooooooo ggggg X \r\n"
        //   where X is at byte 17, followed by space+CR+LF.
        // We detect the actual entry length by looking at byte 20.
        for i in 0..count {
            if pos + 20 > slice.len() {
                return Err(ParseError::new(
                    format!("xref entry {i} truncated"),
                    start + pos,
                ));
            }

            // Determine entry length (20 or 21 bytes).
            let entry_len = if pos + 21 <= slice.len()
                && matches!(slice[pos + 20], b'\r' | b'\n')
            {
                21usize
            } else {
                20usize
            };

            let entry_bytes = &slice[pos..pos + entry_len];

            // Type byte is at index 17 ('n' or 'f').
            let kind_idx = find_xref_entry_type(entry_bytes).ok_or_else(|| {
                ParseError::new(
                    format!("cannot locate 'n'/'f' type byte in xref entry {i}"),
                    start + pos,
                )
            })?;
            let kind = entry_bytes[kind_idx];

            // Offset: bytes 0-9; generation: bytes 11-15.
            let offset_str = std::str::from_utf8(&entry_bytes[0..10])
                .map_err(|_| ParseError::new("xref entry offset not UTF-8", start + pos))?
                .trim();
            let gen_str = std::str::from_utf8(&entry_bytes[11..16])
                .map_err(|_| ParseError::new("xref entry generation not UTF-8", start + pos))?
                .trim();

            let byte_offset: u64 = offset_str.parse().map_err(|_| {
                ParseError::new(
                    format!("invalid xref entry offset '{offset_str}'"),
                    start + pos,
                )
            })?;
            let generation: u16 = gen_str.parse().map_err(|_| {
                ParseError::new(
                    format!("invalid xref entry generation '{gen_str}'"),
                    start + pos,
                )
            })?;

            let obj_num = first_obj + i;
            let id = ObjectId::new(obj_num, generation);

            let entry = match kind {
                b'f' => XRefEntry::Free {
                    next_free: byte_offset as u32,
                    generation,
                },
                b'n' => XRefEntry::InUse {
                    offset: byte_offset,
                    generation,
                },
                other => {
                    return Err(ParseError::new(
                        format!("unknown xref entry type '{}'", other as char),
                        start + pos,
                    ))
                }
            };

            table.insert_if_absent(id, entry);
            pos += entry_len;
        }
    }

    // Parse trailer dictionary.
    let trailer_slice = &data[start + pos..];
    let mut parser = Parser::new(trailer_slice);
    let trailer_obj = parser.parse_object()?.ok_or_else(|| {
        ParseError::new("expected trailer dictionary", start + pos)
    })?;

    let trailer_dict = match trailer_obj {
        CosObject::Dictionary(d) => d,
        other => {
            return Err(ParseError::new(
                format!("trailer must be a dictionary, got {other:?}"),
                start + pos,
            ))
        }
    };

    // Follow Prev chain before merging, so current trailer takes precedence.
    if let Some(prev_offset) = trailer_dict.get_int(&CosName::prev()) {
        if prev_offset > 0 {
            parse_xref_table(data, prev_offset as u64, table)?;
        }
    }

    table.merge_trailer(&trailer_dict);
    Ok(())
}

// ---------------------------------------------------------------------------
// XRef stream parsing (PDF 1.5+)
// ---------------------------------------------------------------------------

/// Parses an xref stream at `start` within `data`.
///
/// An xref stream is an indirect object whose stream data encodes the
/// cross-reference entries in a compact binary format. The entry format
/// is controlled by the `/W` array in the stream dictionary.
///
/// Corresponds to `PDFXRefStream` in Java PDFBox.
fn parse_xref_stream(
    data: &[u8],
    start: usize,
    table: &mut XRefTable,
) -> Result<(), ParseError> {
    let slice = &data[start..];
    let mut parser = Parser::new(slice);

    // The xref stream is an indirect object definition: N G obj << ... >> stream ... endstream endobj
    let (_id, obj) = parser
        .parse_indirect_object()?
        .ok_or_else(|| ParseError::new("expected indirect xref stream object", start))?;

    // Backfill stream data — the parser leaves `data` empty as a placeholder.
    // We re-read the raw bytes using the /Length entry and the `stream` keyword position.
    let stream = match obj {
        CosObject::Stream(mut s) => {
            if s.data.is_empty() {
                let length = s.dictionary
                    .get(&CosName::new(b"Length".to_vec()))
                    .and_then(|v| v.as_integer())
                    .unwrap_or(0) as usize;
                if length > 0 {
                    if let Some(kw_pos) = slice.windows(6).position(|w| w == b"stream") {
                        let data_start = kw_pos + 6;
                        // skip one \r\n or \n after the keyword
                        let data_start = if data_start < slice.len() && slice[data_start] == b'\r' {
                            data_start + 2
                        } else if data_start < slice.len() && slice[data_start] == b'\n' {
                            data_start + 1
                        } else {
                            data_start
                        };
                        let end = (data_start + length).min(slice.len());
                        s.data = slice[data_start..end].to_vec();
                    }
                }
            }
            s
        }
        other => {
            return Err(ParseError::new(
                format!("xref stream must be a stream object, got {other:?}"),
                start,
            ))
        }
    };

    let dict = &stream.dictionary;

    // Validate /Type = /XRef
    let type_name = dict.get_name(&CosName::type_name());
    if type_name != Some(&CosName::new(b"XRef".to_vec())) {
        return Err(ParseError::new(
            "xref stream /Type is not /XRef",
            start,
        ));
    }

    // Read /W — field widths [type, field2, field3]
    let w_array = dict
        .get_array(&CosName::new(b"W".to_vec()))
        .ok_or_else(|| ParseError::new("xref stream missing /W array", start))?;
    if w_array.len() != 3 {
        return Err(ParseError::new("xref stream /W must have 3 elements", start));
    }
    let w: [usize; 3] = [
        w_array[0].as_integer().unwrap_or(0) as usize,
        w_array[1].as_integer().unwrap_or(0) as usize,
        w_array[2].as_integer().unwrap_or(0) as usize,
    ];
    let entry_size = w[0] + w[1] + w[2];
    if entry_size == 0 {
        return Err(ParseError::new("xref stream /W sums to zero", start));
    }

    // Read /Index — pairs of [first_obj, count]. Defaults to [0, /Size].
    let size = dict
        .get_int(&CosName::new(b"Size".to_vec()))
        .unwrap_or(0) as u32;

    let index_pairs: Vec<(u32, u32)> = if let Some(idx) = dict.get_array(&CosName::new(b"Index".to_vec())) {
        if idx.len() % 2 != 0 {
            return Err(ParseError::new("xref stream /Index length must be even", start));
        }
        idx.chunks(2)
            .map(|pair| {
                let first = pair[0].as_integer().unwrap_or(0) as u32;
                let count = pair[1].as_integer().unwrap_or(0) as u32;
                (first, count)
            })
            .collect()
    } else {
        vec![(0, size)]
    };

    // Decode stream data (apply /Filter if present, e.g. FlateDecode)
    let decoded_data: Vec<u8> = {
        let filter = dict.get(&CosName::new(b"Filter".to_vec()));
        let raw = &stream.data;
        if filter.is_some() {
            crate::io::decode_stream(raw, filter)
                .unwrap_or_else(|_| raw.to_vec())
        } else {
            raw.to_vec()
        }
    };

    {
        // Parse decoded stream data.
        let stream_data = &decoded_data;
        let mut byte_pos = 0usize;

        for (first_obj, count) in &index_pairs {
            for i in 0..*count {
                if byte_pos + entry_size > stream_data.len() {
                    return Err(ParseError::new(
                        format!("xref stream data truncated at entry {i}"),
                        start,
                    ));
                }

                let entry_bytes = &stream_data[byte_pos..byte_pos + entry_size];
                byte_pos += entry_size;

                let entry_type = if w[0] == 0 {
                    1u8 // default type is 1 (in-use) when /W[0] is 0
                } else {
                    read_be_uint(entry_bytes, 0, w[0]) as u8
                };

                let field2 = read_be_uint(entry_bytes, w[0], w[1]);
                let field3 = read_be_uint(entry_bytes, w[0] + w[1], w[2]);

                let obj_num = first_obj + i;
                // For type-2 (Compressed) entries, field3 is the index-within-stream,
                // not the generation number. Compressed objects always have generation 0.
                let generation = match entry_type { 2 => 0u16, _ => field3 as u16 };
                let id = ObjectId::new(obj_num, generation);

                let entry = match entry_type {
                    0 => XRefEntry::Free {
                        next_free: field2 as u32,
                        generation: field3 as u16,
                    },
                    1 => XRefEntry::InUse {
                        offset: field2,
                        generation: field3 as u16,
                    },
                    2 => XRefEntry::Compressed {
                        stream_object_number: field2 as u32,
                        index_in_stream: field3 as u32,
                    },
                    other => {
                        return Err(ParseError::new(
                            format!("unknown xref stream entry type {other}"),
                            start,
                        ))
                    }
                };

                table.insert_if_absent(id, entry);
            }
        }
    }

    // Follow Prev chain.
    if let Some(prev_offset) = dict.get_int(&CosName::prev()) {
        if prev_offset > 0 {
            parse_xref_table(data, prev_offset as u64, table)?;
        }
    }

    table.merge_trailer(dict);
    Ok(())
}

// ---------------------------------------------------------------------------
// Top-level PDF xref load entry point
// ---------------------------------------------------------------------------

/// Loads the complete xref table from a full PDF byte slice.
///
/// This is the primary entry point used by `Document::load_from_bytes`.
/// It discovers `startxref`, then recursively loads all xref sections
/// (following `Prev` chains), producing the merged [`XRefTable`].
pub fn load_xref(data: &[u8]) -> Result<XRefTable, ParseError> {
    // PDF spec allows startxref anywhere in the last 1024 bytes of the file.
    let startxref_offset = find_startxref(data, 1024)?;
    let mut table = XRefTable::new();
    parse_xref_table(data, startxref_offset, &mut table)?;
    Ok(table)
}

// ---------------------------------------------------------------------------
// Stream data extraction helper
// ---------------------------------------------------------------------------

/// Reads the raw bytes of the stream body for an indirect object whose
/// dictionary declares a `/Length`. This is used when re-reading stream
/// data for xref or content streams at a given offset.
///
/// Returns `(end_of_stream_pos_in_data, raw_bytes)`.
pub fn read_stream_data(
    data: &[u8],
    stream_start: usize,
    length: usize,
) -> Result<Vec<u8>, ParseError> {
    // Skip a single `\n` or `\r\n` after the `stream` keyword.
    let mut pos = stream_start;
    if pos < data.len() && data[pos] == b'\r' {
        pos += 1;
    }
    if pos < data.len() && data[pos] == b'\n' {
        pos += 1;
    }

    if pos + length > data.len() {
        return Err(ParseError::new(
            format!("stream data extends past EOF (need {length} bytes at pos {pos})"),
            pos,
        ));
    }

    Ok(data[pos..pos + length].to_vec())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Skips ASCII whitespace at the front of `bytes`, returning the remaining slice.
fn skip_whitespace_bytes(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | 0x0C | 0x00) {
        i += 1;
    }
    &bytes[i..]
}

/// Returns the index after leading whitespace within `slice`, starting at `pos`.
fn skip_ws_at(slice: &[u8], mut pos: usize) -> usize {
    while pos < slice.len()
        && matches!(slice[pos], b' ' | b'\t' | b'\r' | b'\n' | 0x0C | 0x00)
    {
        pos += 1;
    }
    pos
}

/// Skips an end-of-line sequence (`\r\n`, `\r`, or `\n`) at `pos`.
fn skip_eol(slice: &[u8], mut pos: usize) -> usize {
    if pos < slice.len() && slice[pos] == b'\r' {
        pos += 1;
    }
    if pos < slice.len() && slice[pos] == b'\n' {
        pos += 1;
    }
    pos
}

/// Finds the index of the `n` or `f` type byte in a raw xref entry.
///
/// Scans backwards from position 17 (or 16) to find the first occurrence
/// of `b'n'` or `b'f'` that is preceded by a space at index-1.
/// Returns `None` if neither is found.
fn find_xref_entry_type(entry: &[u8]) -> Option<usize> {
    // Try standard positions: 17 (20-byte) or 16 (if offset/gen are present).
    for &idx in &[17usize, 16usize] {
        if idx < entry.len() && matches!(entry[idx], b'n' | b'f') {
            return Some(idx);
        }
    }
    // Fallback: linear search for 'n'/'f' preceded by space.
    for i in 1..entry.len() {
        if matches!(entry[i], b'n' | b'f') && entry[i - 1] == b' ' {
            return Some(i);
        }
    }
    None
}

/// Reads a decimal `u32` from the front of `bytes`, returning the value and the
/// number of bytes consumed. Returns `None` if no digit is found.
fn read_ascii_u32(bytes: &[u8]) -> Option<(u32, usize)> {
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let s = std::str::from_utf8(&bytes[..i]).ok()?;
    let val: u32 = s.parse().ok()?;
    Some((val, i))
}

/// Reads a decimal integer from the front of `bytes`, returning the slice of
/// digits and the remainder.
fn read_ascii_integer_bytes(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    Some((&bytes[..i], &bytes[i..]))
}

/// Reads `width` bytes from `entry` starting at `offset` as a big-endian unsigned integer.
fn read_be_uint(entry: &[u8], offset: usize, width: usize) -> u64 {
    if width == 0 {
        return 0;
    }
    let mut val = 0u64;
    for i in 0..width {
        val = (val << 8) | entry[offset + i] as u64;
    }
    val
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- find_startxref ----

    #[test]
    fn find_startxref_basic() {
        let data = b"%PDF-1.4\n%%EOF\nstartxref\n42\n%%EOF\n";
        let offset = find_startxref(data, 1024).unwrap();
        assert_eq!(offset, 42);
    }

    #[test]
    fn find_startxref_with_crlf() {
        let data = b"%PDF-1.4\r\n%%EOF\r\nstartxref\r\n100\r\n%%EOF\r\n";
        let offset = find_startxref(data, 1024).unwrap();
        assert_eq!(offset, 100);
    }

    #[test]
    fn find_startxref_missing_returns_error() {
        let data = b"%PDF-1.4\n%%EOF\n";
        let result = find_startxref(data, 1024);
        assert!(result.is_err());
    }

    #[test]
    fn find_startxref_only_in_tail() {
        // Put a fake startxref early that points to 0, real one at end pointing to 99.
        let mut data = b"startxref\n0\n".to_vec();
        data.extend_from_slice(&[b'x'; 2000]);
        data.extend_from_slice(b"startxref\n99\n%%EOF\n");
        // Tail search of 1024 bytes should find the last one (99).
        let offset = find_startxref(&data, 1024).unwrap();
        assert_eq!(offset, 99);
    }

    // ---- parse_xref_table (traditional) ----

    fn make_minimal_pdf_xref_table() -> Vec<u8> {
        // Object 1: a simple dictionary.
        // Object 0 is always free in a minimal PDF.
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
        let xref_offset = pdf.len();
        let obj1_off_str = format!("{:010} 00000 n \r\n", obj1_offset);
        pdf.extend_from_slice(b"xref\n");
        pdf.extend_from_slice(b"0 2\n");
        pdf.extend_from_slice(b"0000000000 65535 f \r\n"); // obj 0 free
        pdf.extend_from_slice(obj1_off_str.as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 2 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());
        pdf
    }

    #[test]
    fn parse_traditional_xref_table_basic() {
        let pdf = make_minimal_pdf_xref_table();
        let mut table = XRefTable::new();
        let xref_offset = find_startxref(&pdf, 1024).unwrap();
        parse_xref_table(&pdf, xref_offset, &mut table).unwrap();

        // Object 0 should be free.
        let id0 = ObjectId::new(0, 65535);
        assert!(matches!(table.get(&id0), Some(XRefEntry::Free { .. })));

        // Object 1 should be in-use at some offset > 0.
        let id1 = ObjectId::new(1, 0);
        assert!(matches!(table.get(&id1), Some(XRefEntry::InUse { offset, .. }) if *offset > 0));

        // Trailer should have /Size.
        assert_eq!(table.trailer.get_int(&CosName::new(b"Size".to_vec())), Some(2));
    }

    #[test]
    fn load_xref_end_to_end() {
        let pdf = make_minimal_pdf_xref_table();
        let table = load_xref(&pdf).unwrap();
        assert!(table.len() >= 2);

        let id1 = ObjectId::new(1, 0);
        assert!(table.get(&id1).is_some());
    }

    // ---- read_stream_data ----

    #[test]
    fn read_stream_data_lf() {
        let data = b"\nhello world";
        let bytes = read_stream_data(data, 0, 11).unwrap();
        assert_eq!(bytes, b"hello world");
    }

    #[test]
    fn read_stream_data_crlf() {
        let data = b"\r\nhello world";
        let bytes = read_stream_data(data, 0, 11).unwrap();
        assert_eq!(bytes, b"hello world");
    }

    #[test]
    fn read_stream_data_past_eof() {
        let data = b"\nhello";
        let result = read_stream_data(data, 0, 100);
        assert!(result.is_err());
    }

    // ---- XRefTable ----

    #[test]
    fn xref_table_insert_if_absent() {
        let mut table = XRefTable::new();
        let id = ObjectId::new(1, 0);
        table.insert_if_absent(id.clone(), XRefEntry::InUse { offset: 100, generation: 0 });
        // Second insert should not overwrite.
        table.insert_if_absent(id.clone(), XRefEntry::InUse { offset: 999, generation: 0 });
        assert!(matches!(table.get(&id), Some(XRefEntry::InUse { offset: 100, .. })));
    }

    #[test]
    fn xref_table_merge_trailer() {
        let mut table = XRefTable::new();
        let mut d1 = CosDictionary::new();
        d1.insert(CosName::new(b"Size".to_vec()), CosObject::Integer(5));
        table.merge_trailer(&d1);

        let mut d2 = CosDictionary::new();
        d2.insert(CosName::new(b"Size".to_vec()), CosObject::Integer(999)); // should NOT overwrite
        d2.insert(CosName::new(b"Root".to_vec()), CosObject::Null);
        table.merge_trailer(&d2);

        assert_eq!(table.trailer.get_int(&CosName::new(b"Size".to_vec())), Some(5));
        assert!(table.trailer.get(&CosName::new(b"Root".to_vec())).is_some());
    }

    // ---- Helper unit tests ----

    #[test]
    fn read_ascii_u32_basic() {
        assert_eq!(read_ascii_u32(b"123 abc"), Some((123, 3)));
        assert_eq!(read_ascii_u32(b"abc"), None);
    }

    #[test]
    fn read_be_uint_basic() {
        assert_eq!(read_be_uint(&[0x00, 0x01, 0x86, 0xA0], 0, 4), 100_000);
        assert_eq!(read_be_uint(&[0xFF], 0, 1), 255);
        assert_eq!(read_be_uint(&[], 0, 0), 0);
    }
}


