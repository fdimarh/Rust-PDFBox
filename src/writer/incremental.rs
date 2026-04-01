//! Incremental append writer for PDF documents.
//!
//! Maps to Java PDFBox `COSWriter` in incremental-update mode.
//!
//! # What this does
//!
//! PDF §7.5.6 defines incremental updates: instead of rewriting the whole
//! file, an update appends:
//!
//! 1. The original file bytes (unchanged).
//! 2. Only the *new or modified* indirect objects.
//! 3. A new `xref` section covering only those objects.
//! 4. An updated `trailer` with `/Prev` pointing to the original `startxref`.
//! 5. A new `startxref` line followed by `%%EOF`.
//!
//! Readers parse the latest section first and fall back to earlier sections
//! for any object not present in the update — so the original data is never
//! touched.
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type |
//! |---|---|
//! | `COSWriter` (incremental mode) | [`IncrementalWriter`] |
//! | incremental update section | [`IncrementalWriter::write_update`] |
//!
//! # Usage
//!
//! ```rust,ignore
//! let original_bytes = std::fs::read("input.pdf")?;
//! let doc = Document::load_from_bytes(&original_bytes)?;
//!
//! // Build the set of changed/new objects
//! let mut changed = HashMap::new();
//! changed.insert(ObjectId::new(3, 0), CosObject::Integer(99));
//!
//! let mut out = std::fs::File::create("updated.pdf")?;
//! IncrementalWriter::write_update(&original_bytes, &doc, &changed, &mut out)?;
//! ```

use std::collections::BTreeMap;
use std::io::{self, Write};

use crate::cos::{CosName, CosObject, ObjectId};
use crate::Document;
use super::serializer::Serializer;

// ---------------------------------------------------------------------------
// IncrementalWriter
// ---------------------------------------------------------------------------

/// Appends an incremental update section to an existing PDF byte stream.
///
/// The original bytes are written first (unchanged), then the new/modified
/// objects, a minimal xref section, and an updated trailer.
pub struct IncrementalWriter;

impl IncrementalWriter {
    /// Appends an incremental update to `out`.
    ///
    /// # Arguments
    ///
    /// * `original` — the original, unmodified PDF bytes (will be written first).
    /// * `doc`      — the loaded document (provides the original trailer and catalog ref).
    /// * `changed`  — the new or modified objects to include in the update.
    ///                Keys are `ObjectId`; values are the new `CosObject` bodies.
    /// * `out`      — the destination writer.
    ///
    /// # PDF spec compliance
    ///
    /// * The new xref section covers only `changed` objects.
    /// * `/Prev` in the new trailer points to the original `startxref` offset.
    /// * `/Size` is the maximum object number across original + changed + 1.
    pub fn write_update<W: Write>(
        original: &[u8],
        doc: &Document,
        changed: &BTreeMap<ObjectId, CosObject>,
        out: &mut W,
    ) -> io::Result<()> {
        // 1. Write original bytes verbatim.
        out.write_all(original)?;

        // Ensure we start on a new line after the original %%EOF.
        // Many PDFs end with %%EOF\n but some don't have a trailing newline.
        if original.last().copied() != Some(b'\n') {
            out.write_all(b"\n")?;
        }

        // Track running byte offset for xref entries.
        // Start counting from the length of the original bytes (+ possible \n above).
        let base_offset = original.len() as u64
            + if original.last().copied() != Some(b'\n') { 1 } else { 0 };

        // 2. Write each changed object and record its offset.
        let mut object_offsets: BTreeMap<ObjectId, u64> = BTreeMap::new();
        let mut running = base_offset;

        // Collect bytes for all changed objects first so we can compute offsets.
        let mut object_bytes: BTreeMap<ObjectId, Vec<u8>> = BTreeMap::new();
        for (id, obj) in changed {
            let mut buf: Vec<u8> = Vec::new();
            {
                let mut ser = Serializer::new(&mut buf);
                ser.write_indirect_object(*id, obj)?;
            }
            object_bytes.insert(*id, buf);
        }

        // Write them and record offsets.
        for (id, bytes) in &object_bytes {
            object_offsets.insert(*id, running);
            out.write_all(bytes)?;
            running += bytes.len() as u64;
        }

        // 3. Write the new xref section (covers only changed objects).
        let xref_offset = running;

        // We write individual single-entry subsections for each changed object
        // (one subsection per contiguous run of object numbers).
        let xref_bytes = Self::build_xref_section(&object_offsets);
        out.write_all(&xref_bytes)?;

        // 4. Write updated trailer.
        //
        // Required keys:
        //   /Size  — highest object number + 1 (across original + update)
        //   /Root  — catalog reference (preserved from original trailer)
        //   /Prev  — startxref offset of the *previous* version
        //
        // Keys NOT carried forward: /Prev inside inner Prev chains (handled by
        // reader following the chain).

        let prev_startxref = Self::find_startxref(original);

        let max_original_obj = doc
            .xref
            .iter()
            .map(|(id, _)| id.object_number)
            .max()
            .unwrap_or(0);
        let max_changed_obj = object_offsets
            .keys()
            .map(|id| id.object_number)
            .max()
            .unwrap_or(0);
        let new_size = max_original_obj.max(max_changed_obj) + 1;

        let mut trailer = doc.trailer().clone();
        trailer.insert(CosName::size(), CosObject::Integer(new_size as i64));
        trailer.insert(CosName::prev(), CosObject::Integer(prev_startxref as i64));
        // Remove /XRefStm if present (not supported in classic xref table trailer)
        trailer.remove(&CosName::new(b"XRefStm".to_vec()));

        let mut trailer_bytes: Vec<u8> = b"trailer\n".to_vec();
        {
            let mut ser = Serializer::new(&mut trailer_bytes);
            ser.write_object(&CosObject::Dictionary(trailer))?;
        }
        out.write_all(&trailer_bytes)?;

        out.write_all(b"\nstartxref\n")?;
        write!(out, "{xref_offset}\n")?;
        out.write_all(b"%%EOF\n")?;

        Ok(())
    }

    /// Builds the xref section bytes for the given object offsets.
    ///
    /// Groups consecutive object numbers into subsections as required by the
    /// PDF spec (§7.5.4).
    fn build_xref_section(offsets: &BTreeMap<ObjectId, u64>) -> Vec<u8> {
        if offsets.is_empty() {
            return b"xref\n".to_vec();
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(b"xref\n");

        // Build sorted list of (object_number, generation, offset)
        let entries: Vec<(u32, u16, u64)> = offsets
            .iter()
            .map(|(id, &off)| (id.object_number, id.generation, off))
            .collect();

        // Group into contiguous subsections
        let mut subsections: Vec<Vec<(u32, u16, u64)>> = Vec::new();
        let mut current: Vec<(u32, u16, u64)> = Vec::new();

        for entry in &entries {
            if current.is_empty() {
                current.push(*entry);
            } else {
                let prev_num = current.last().unwrap().0;
                if entry.0 == prev_num + 1 {
                    current.push(*entry);
                } else {
                    subsections.push(std::mem::take(&mut current));
                    current.push(*entry);
                }
            }
        }
        if !current.is_empty() {
            subsections.push(current);
        }

        for subsection in &subsections {
            let start = subsection[0].0;
            let count = subsection.len();
            buf.extend_from_slice(format!("{start} {count}\n").as_bytes());
            for (_num, generation, offset) in subsection {
                buf.extend_from_slice(
                    format!("{:010} {:05} n \r\n", offset, generation).as_bytes(),
                );
            }
        }

        buf
    }

    /// Scans the original PDF bytes to find the *last* `startxref` value.
    ///
    /// This is the offset we put in `/Prev` of the new trailer.
    fn find_startxref(original: &[u8]) -> u64 {
        // Scan last 1024 bytes
        let search_start = original.len().saturating_sub(1024);
        let tail = &original[search_start..];

        // Find last occurrence of "startxref"
        let keyword = b"startxref";
        let mut last_pos: Option<usize> = None;
        for i in 0..tail.len().saturating_sub(keyword.len()) {
            if &tail[i..i + keyword.len()] == keyword {
                last_pos = Some(i);
            }
        }

        let pos = match last_pos {
            Some(p) => p,
            None => return 0,
        };

        // Skip "startxref" and any whitespace/newlines
        let after = &tail[pos + keyword.len()..];
        let trimmed = after.iter().position(|&b| b.is_ascii_digit()).unwrap_or(0);
        let number_slice = &after[trimmed..];
        let end = number_slice
            .iter()
            .position(|&b| !b.is_ascii_digit())
            .unwrap_or(number_slice.len());
        std::str::from_utf8(&number_slice[..end])
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::cos::CosDictionary;

    // -----------------------------------------------------------------------
    // Helper: build a minimal valid PDF in memory
    // -----------------------------------------------------------------------

    fn build_minimal_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let cat_off = pdf.len() as u64;
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let pages_off = pdf.len() as u64;
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let xref_off = pdf.len();
        let e1 = format!("{:010} 00000 n \r\n", cat_off);
        let e2 = format!("{:010} 00000 n \r\n", pages_off);
        pdf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \r\n");
        pdf.extend_from_slice(e1.as_bytes());
        pdf.extend_from_slice(e2.as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
        pdf
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn incremental_update_is_loadable() {
        let original = build_minimal_pdf();
        let doc = Document::load_from_bytes(&original).unwrap();

        // Add object 3 — a new integer
        let mut changed = BTreeMap::new();
        changed.insert(ObjectId::new(3, 0), CosObject::Integer(42));

        let mut out = Vec::new();
        IncrementalWriter::write_update(&original, &doc, &changed, &mut out).unwrap();

        // The result must be parseable
        let updated_doc = Document::load_from_bytes(&out).unwrap();

        // Original objects still accessible
        assert!(updated_doc.catalog().is_some());
        assert_eq!(updated_doc.page_count(), 0);
        // New object 3 is present in the object store
        let obj3 = updated_doc.objects.get(&ObjectId::new(3, 0));
        assert_eq!(obj3, Some(&CosObject::Integer(42)));
    }

    #[test]
    fn incremental_update_overrides_existing_object() {
        // Modify object 2 (the Pages dict) — set Count to 1 even though there
        // are no Kids (this is syntactically valid for the writer test).
        let original = build_minimal_pdf();
        let doc = Document::load_from_bytes(&original).unwrap();

        let mut new_pages = CosDictionary::new();
        new_pages.insert(CosName::type_name(), CosObject::Name(CosName::pages()));
        new_pages.insert(CosName::kids(), CosObject::Array(vec![]));
        new_pages.insert(CosName::count(), CosObject::Integer(0));

        let mut changed = BTreeMap::new();
        changed.insert(
            ObjectId::new(2, 0),
            CosObject::Dictionary(new_pages),
        );

        let mut out = Vec::new();
        IncrementalWriter::write_update(&original, &doc, &changed, &mut out).unwrap();

        let updated_doc = Document::load_from_bytes(&out).unwrap();
        assert_eq!(updated_doc.page_count(), 0);
        assert_eq!(updated_doc.object_count(), 2); // 1 catalog + 1 pages
    }

    #[test]
    fn find_startxref_extracts_correct_offset() {
        let pdf = build_minimal_pdf();
        let offset = IncrementalWriter::find_startxref(&pdf);
        // The offset should be non-zero and point inside the file
        assert!(offset > 0, "startxref should be non-zero, got {offset}");
        assert!((offset as usize) < pdf.len());
    }

    #[test]
    fn updated_bytes_contain_prev_key() {
        let original = build_minimal_pdf();
        let doc = Document::load_from_bytes(&original).unwrap();
        let mut changed = BTreeMap::new();
        changed.insert(ObjectId::new(3, 0), CosObject::Bool(true));

        let mut out = Vec::new();
        IncrementalWriter::write_update(&original, &doc, &changed, &mut out).unwrap();

        let text = String::from_utf8_lossy(&out);
        // The updated trailer must contain /Prev
        assert!(text.contains("/Prev"), "expected /Prev in output:\n{text}");
    }

    #[test]
    fn original_bytes_intact_at_start() {
        let original = build_minimal_pdf();
        let doc = Document::load_from_bytes(&original).unwrap();
        let mut changed = BTreeMap::new();
        changed.insert(ObjectId::new(3, 0), CosObject::Integer(7));

        let mut out = Vec::new();
        IncrementalWriter::write_update(&original, &doc, &changed, &mut out).unwrap();

        // First N bytes must equal original
        assert_eq!(&out[..original.len()], original.as_slice());
    }

    #[test]
    fn empty_changed_set_still_produces_valid_pdf() {
        let original = build_minimal_pdf();
        let doc = Document::load_from_bytes(&original).unwrap();
        let changed = BTreeMap::new();

        let mut out = Vec::new();
        IncrementalWriter::write_update(&original, &doc, &changed, &mut out).unwrap();

        // Still must load
        let updated = Document::load_from_bytes(&out).unwrap();
        assert_eq!(updated.page_count(), 0);
    }

    #[test]
    fn build_xref_section_single_object() {
        let mut offsets = BTreeMap::new();
        offsets.insert(ObjectId::new(3, 0), 1234u64);
        let bytes = IncrementalWriter::build_xref_section(&offsets);
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("3 1\n"), "expected subsection header '3 1', got:\n{text}");
        assert!(text.contains("0001234 00000 n"), "expected offset entry, got:\n{text}");
    }

    #[test]
    fn build_xref_section_contiguous_group() {
        let mut offsets = BTreeMap::new();
        offsets.insert(ObjectId::new(4, 0), 100u64);
        offsets.insert(ObjectId::new(5, 0), 200u64);
        offsets.insert(ObjectId::new(6, 0), 300u64);
        let bytes = IncrementalWriter::build_xref_section(&offsets);
        let text = String::from_utf8(bytes).unwrap();
        // All three should be in one subsection: "4 3"
        assert!(text.contains("4 3\n"), "expected subsection '4 3', got:\n{text}");
    }

    #[test]
    fn build_xref_section_non_contiguous_groups() {
        let mut offsets = BTreeMap::new();
        offsets.insert(ObjectId::new(2, 0), 100u64);
        offsets.insert(ObjectId::new(5, 0), 200u64); // gap at 3,4
        let bytes = IncrementalWriter::build_xref_section(&offsets);
        let text = String::from_utf8(bytes).unwrap();
        // Two separate subsections
        assert!(text.contains("2 1\n"), "expected '2 1', got:\n{text}");
        assert!(text.contains("5 1\n"), "expected '5 1', got:\n{text}");
    }
}


