//! Full-rewrite PDF writer.
//!
//! Maps to Java PDFBox `COSWriter`. This writer serializes a `Document`
//! from scratch, creating a new body, xref table, and trailer. It does not
//! support incremental updates.

use std::io::{self, Write, Seek, SeekFrom};
use std::collections::BTreeMap;
use crate::cos::{CosObject, CosName, ObjectId};
use crate::Document;
use super::serializer::Serializer;

/// Writes a `Document` to an output stream.
pub struct Writer<W: Write> {
    writer: W,
}

impl<W: Write + Seek> Writer<W> {
    /// Creates a new writer for the given output stream.
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Writes the entire `Document` to the output stream.
    pub fn write_document(&mut self, doc: &Document) -> io::Result<()> {
        // 1. Write header
        self.writer.write_all(b"%PDF-1.7\n")?;
        // A binary comment to mark the file as binary
        self.writer.write_all(b"%\xe2\xe3\xcf\xd3\n")?;

        // 2. Write all indirect objects from the object store
        let mut object_offsets = BTreeMap::new();
        // Sort objects by ID for deterministic output
        let sorted_ids: BTreeMap<_, _> = doc.objects.objects.iter().collect();

        for (id, obj) in sorted_ids {
            let offset = self.writer.seek(SeekFrom::Current(0))?;
            object_offsets.insert(*id, offset);
            let mut serializer = Serializer::new(&mut self.writer);
            serializer.write_indirect_object(*id, obj)?;
        }

        // 3. Write the new xref table
        let xref_offset = self.writer.seek(SeekFrom::Current(0))?;
        self.write_xref_table(&object_offsets)?;

        // 4. Write the trailer
        self.write_trailer(doc, xref_offset, object_offsets.len() + 1)?;

        Ok(())
    }

    /// Writes the `xref` table section.
    fn write_xref_table(&mut self, offsets: &BTreeMap<ObjectId, u64>) -> io::Result<()> {
        self.writer.write_all(b"xref\n")?;
        // For simplicity, we write one subsection for all objects from 0 to max_id.
        let max_id = offsets.keys().map(|id| id.object_number).max().unwrap_or(0);
        write!(self.writer, "0 {}\n", max_id + 1)?;

        // Object 0 is always the free list head
        self.writer.write_all(b"0000000000 65535 f \r\n")?;

        for i in 1..=max_id {
            let found = offsets.keys().find(|id| id.object_number == i);
            if let Some(id) = found {
                let offset = offsets[id];
                write!(self.writer, "{:010} {:05} n \r\n", offset, id.generation)?;
            } else {
                // This object ID is unused in the document
                self.writer.write_all(b"0000000000 65535 f \r\n")?;
            }
        }
        Ok(())
    }

    /// Writes the `trailer` dictionary and `startxref` pointer.
    fn write_trailer(&mut self, doc: &Document, xref_offset: u64, size: usize) -> io::Result<()> {
        self.writer.write_all(b"trailer\n")?;
        let mut trailer = doc.trailer().clone();
        // Ensure /Size is correct
        trailer.insert(CosName::size(), CosObject::Integer(size as i64));
        // /Root must be present
        if doc.catalog_ref().is_none() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Document has no catalog/root"));
        }

        let mut serializer = Serializer::new(&mut self.writer);
        serializer.write_object(&CosObject::Dictionary(trailer))?;

        self.writer.write_all(b"\nstartxref\n")?;
        write!(self.writer, "{}\n", xref_offset)?;
        self.writer.write_all(b"%%EOF\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use crate::{Document, ObjectStore, cos::CosDictionary};

    #[test]
    fn write_minimal_document() {
        // Create a simple document in memory
        let mut objects = ObjectStore::new();
        let cat_id = ObjectId::new(1, 0);
        let pages_id = ObjectId::new(2, 0);

        let mut catalog = CosDictionary::new();
        catalog.insert(CosName::type_name(), CosObject::Name(CosName::catalog()));
        catalog.insert(CosName::pages(), CosObject::Reference(pages_id));
        objects.insert(cat_id, CosObject::Dictionary(catalog));

        let mut pages = CosDictionary::new();
        pages.insert(CosName::type_name(), CosObject::Name(CosName::pages()));
        pages.insert(CosName::kids(), CosObject::Array(vec![]));
        pages.insert(CosName::count(), CosObject::Integer(0));
        objects.insert(pages_id, CosObject::Dictionary(pages));

        let mut trailer = CosDictionary::new();
        trailer.insert(CosName::root(), CosObject::Reference(cat_id));

        let doc = Document {
            source_len: 0,
            xref: Default::default(),
            objects,
        };
        // Override trailer for the test
        let mut doc_with_trailer = doc.clone();
        doc_with_trailer.xref.trailer = trailer;


        // Write it to a buffer
        let mut buffer = Cursor::new(Vec::new());
        let mut writer = Writer::new(&mut buffer);
        writer.write_document(&doc_with_trailer).unwrap();

        // Reload and verify
        let written_bytes = buffer.into_inner();
        let reloaded_doc = Document::load_from_bytes(&written_bytes).unwrap();

        assert_eq!(reloaded_doc.object_count(), 2);
        assert!(reloaded_doc.catalog().is_some());
        assert_eq!(reloaded_doc.page_count(), 0);
    }
}
