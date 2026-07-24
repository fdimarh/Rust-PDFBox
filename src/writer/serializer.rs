//! Serializer for individual COS objects.
//!
//! Maps to Java PDFBox `COSWriter`. This module writes `CosObject` variants
//! to a byte buffer in their correct syntactic form (e.g. `(string)`,
//! `<hexstring>`, `/Name`, `[1 2 3]`, `<< /K 1 >>`).

use std::io::{self, Write};
use crate::crypto::handlers::StandardSecurityHandler;
use std::collections::HashSet;
use crate::cos::{CosObject, CosName, CosDictionary, CosStream, ObjectId};

/// Core CosObject serializer.
///
/// Converts in-memory tree back to PDF bytes (e.g., `<< /Type /Page >>`).
pub struct Serializer<'a, W: Write> {
    writer: &'a mut W,
    file_key: Option<Vec<u8>>,
    bypass_ids: HashSet<ObjectId>,
    current_object_id: Option<ObjectId>,
}

impl<'a, W: Write> Serializer<'a, W> {
    /// Creates a new serializer writing to the given writer (plaintext mode).
    pub fn new(writer: &'a mut W) -> Self {
        Serializer { writer, file_key: None, bypass_ids: HashSet::new(), current_object_id: None }
    }
    
    /// Creates a serializer that encrypts strings and streams on-the-fly.
    pub fn new_encrypted(
        writer: &'a mut W,
        file_key: Option<Vec<u8>>,
        bypass_ids: HashSet<ObjectId>,
    ) -> Self {
        Serializer { writer, file_key, bypass_ids, current_object_id: None }
    }

    /// Writes a single `CosObject`.
    pub fn write_object(&mut self, obj: &CosObject) -> io::Result<()> {
        match obj {
            CosObject::Null => self.writer.write_all(b"null")?,
            CosObject::Bool(b) => self.writer.write_all(if *b { b"true" } else { b"false" })?,
            CosObject::Integer(n) => write!(self.writer, "{n}")?,
            CosObject::Real(n) => write!(self.writer, "{n}")?,
            CosObject::String(bytes) => {
                if let Some(ref file_key) = self.file_key {
                    if let Some(id) = self.current_object_id {
                        if !self.bypass_ids.contains(&id) {
                            let obj_key = crate::crypto::handlers::StandardSecurityHandler::compute_object_key(file_key, id.object_number as u32, id.generation as u16, false);
                            let encrypted = crate::crypto::rc4::Rc4::crypt(&obj_key, bytes);
                            return self.write_hex_string(&encrypted); // Write ciphertext as Hex
                        }
                    }
                }
                self.write_string(bytes)?
            }
            CosObject::HexString(bytes) => {
                if let Some(ref file_key) = self.file_key {
                    if let Some(id) = self.current_object_id {
                        if !self.bypass_ids.contains(&id) {
                            let obj_key = crate::crypto::handlers::StandardSecurityHandler::compute_object_key(file_key, id.object_number as u32, id.generation as u16, false);
                            let encrypted = crate::crypto::rc4::Rc4::crypt(&obj_key, bytes);
                            return self.write_hex_string(&encrypted);
                        }
                    }
                }
                self.write_hex_string(bytes)?
            }
            CosObject::Name(name) => self.write_name(name)?,
            CosObject::Array(arr) => self.write_array(arr)?,
            CosObject::Dictionary(dict) => self.write_dictionary(dict)?,
            CosObject::Stream(stream) => {
                if let Some(ref file_key) = self.file_key {
                    if let Some(id) = self.current_object_id {
                        if !self.bypass_ids.contains(&id) {
                            let obj_key = crate::crypto::handlers::StandardSecurityHandler::compute_object_key(file_key, id.object_number as u32, id.generation as u16, false);
                            let mut encrypted_stream = stream.clone();
                            encrypted_stream.data = crate::crypto::rc4::Rc4::crypt(&obj_key, &stream.data);
                            encrypted_stream.dictionary.insert(crate::cos::CosName::new(b"Length".to_vec()), crate::cos::CosObject::Integer(encrypted_stream.data.len() as i64));
                            return self.write_stream(&encrypted_stream);
                        }
                    }
                }
                self.write_stream(stream)?
            }
            CosObject::Reference(id) => self.write_reference(id)?,
        }
        Ok(())
    }

    pub fn write_indirect_object(&mut self, id: crate::cos::ObjectId, obj: &crate::cos::CosObject) -> io::Result<()> {
        self.current_object_id = Some(id);
        write!(self.writer, "{} {} obj\n", id.object_number, id.generation)?;
        self.write_object(obj)?;
        write!(self.writer, "\nendobj\n")?;
        self.current_object_id = None;
        Ok(())
    }

    fn write_string(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(b"(")?;
        for &byte in bytes {
            match byte {
                b'(' | b')' | b'\\' => {
                    self.writer.write_all(&[b'\\', byte])?;
                }
                b'\r' => self.writer.write_all(b"\\r")?,
                b'\n' => self.writer.write_all(b"\\n")?,
                b'\t' => self.writer.write_all(b"\\t")?,
                b'\x08' => self.writer.write_all(b"\\b")?,
                b'\x0C' => self.writer.write_all(b"\\f")?,
                _ => self.writer.write_all(&[byte])?,
            }
        }
        self.writer.write_all(b")")?;
        Ok(())
    }

    fn write_hex_string(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(b"<")?;
        for &byte in bytes {
            write!(self.writer, "{:02X}", byte)?;
        }
        self.writer.write_all(b">")?;
        Ok(())
    }

    fn write_name(&mut self, name: &CosName) -> io::Result<()> {
        self.writer.write_all(b"/")?;
        for &byte in name.as_bytes() {
            match byte {
                // PDF regular characters are fine. Escape special ones.
                0x00..=0x20 | b'%' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'#' => {
                    write!(self.writer, "#{:02X}", byte)?;
                }
                _ => self.writer.write_all(&[byte])?,
            }
        }
        Ok(())
    }

    fn write_array(&mut self, arr: &[CosObject]) -> io::Result<()> {
        self.writer.write_all(b"[")?;
        for (i, item) in arr.iter().enumerate() {
            if i > 0 { self.writer.write_all(b" ")?; }
            self.write_object(item)?;
        }
        self.writer.write_all(b"]")?;
        Ok(())
    }

    fn write_dictionary(&mut self, dict: &CosDictionary) -> io::Result<()> {
        self.writer.write_all(b"<<\n")?;
        for (key, val) in dict.iter() {
            self.write_name(key)?;
            self.writer.write_all(b" ")?;
            self.write_object(val)?;
            self.writer.write_all(b"\n")?;
        }
        self.writer.write_all(b">>")?;
        Ok(())
    }

    fn write_stream(&mut self, stream: &CosStream) -> io::Result<()> {
        self.write_dictionary(&stream.dictionary)?;
        self.writer.write_all(b"\nstream\n")?;
        self.writer.write_all(&stream.data)?;
        self.writer.write_all(b"\nendstream")?;
        Ok(())
    }

    fn write_reference(&mut self, id: &ObjectId) -> io::Result<()> {
        write!(self.writer, "{} {} R", id.object_number, id.generation)?;
        Ok(())
    }
}
