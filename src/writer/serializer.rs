//! Serializer for individual COS objects.
//!
//! Maps to Java PDFBox `COSWriter`. This module writes `CosObject` variants
//! to a byte buffer in their correct syntactic form (e.g. `(string)`,
//! `<hexstring>`, `/Name`, `[1 2 3]`, `<< /K 1 >>`).

use std::io::{self, Write};
use crate::cos::{CosObject, CosName, CosDictionary, CosStream, ObjectId};

/// Writes COS objects to an output stream.
pub struct Serializer<'a, W: Write> {
    writer: &'a mut W,
}

impl<'a, W: Write> Serializer<'a, W> {
    /// Creates a new serializer writing to the given writer.
    pub fn new(writer: &'a mut W) -> Self {
        Self { writer }
    }

    /// Writes a single `CosObject`.
    pub fn write_object(&mut self, obj: &CosObject) -> io::Result<()> {
        match obj {
            CosObject::Null => self.writer.write_all(b"null")?,
            CosObject::Bool(b) => self.writer.write_all(if *b { b"true" } else { b"false" })?,
            CosObject::Integer(n) => write!(self.writer, "{n}")?,
            CosObject::Real(n) => write!(self.writer, "{n}")?,
            CosObject::String(bytes) => self.write_string(bytes)?,
            CosObject::Name(name) => self.write_name(name)?,
            CosObject::Array(arr) => self.write_array(arr)?,
            CosObject::Dictionary(dict) => self.write_dictionary(dict)?,
            CosObject::Stream(stream) => self.write_stream(stream)?,
            CosObject::Reference(id) => self.write_reference(id)?,
        }
        Ok(())
    }

    /// Writes an indirect object definition: `N G obj ... endobj`.
    pub fn write_indirect_object(&mut self, id: ObjectId, obj: &CosObject) -> io::Result<()> {
        write!(self.writer, "{} {} obj\n", id.object_number, id.generation)?;
        self.write_object(obj)?;
        self.writer.write_all(b"\nendobj\n")?;
        Ok(())
    }

    fn write_string(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(b"(")?;
        for &byte in bytes {
            match byte {
                b'(' | b')' | b'\\' => {
                    self.writer.write_all(&[b'\\', byte])?;
                }
                // Non-printable ASCII or high-bit bytes get octal escapes
                b if b < 0x20 || b > 0x7E => {
                    write!(self.writer, "\\{byte:03o}")?;
                }
                _ => {
                    self.writer.write_all(&[byte])?;
                }
            }
        }
        self.writer.write_all(b")")?;
        Ok(())
    }

    fn write_name(&mut self, name: &CosName) -> io::Result<()> {
        self.writer.write_all(b"/")?;
        // Names need hex escapes for non-regular chars
        for &byte in name.as_bytes() {
            match byte {
                // Regular characters are written as-is
                b'!'..=b'~' if !"#%()/<>[]{}".contains(byte as char) => {
                    self.writer.write_all(&[byte])?;
                }
                // Others get a #XX hex escape
                _ => {
                    write!(self.writer, "#{:02X}", byte)?;
                }
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
        self.writer.write_all(b"<<")?;
        for (key, value) in dict.iter() {
            self.writer.write_all(b" ")?;
            self.write_name(key)?;
            self.writer.write_all(b" ")?;
            self.write_object(value)?;
        }
        self.writer.write_all(b" >>")?;
        Ok(())
    }

    fn write_stream(&mut self, stream: &CosStream) -> io::Result<()> {
        // The stream dictionary must have a /Length key matching the data length.
        // We assume it's already correct.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn serialize_to_string(obj: &CosObject) -> String {
        let mut buffer = Vec::new();
        {
            let mut serializer = Serializer::new(&mut buffer);
            serializer.write_object(obj).unwrap();
        }
        String::from_utf8(buffer).unwrap()
    }

    #[test]
    fn serialize_primitives() {
        assert_eq!(serialize_to_string(&CosObject::Null), "null");
        assert_eq!(serialize_to_string(&CosObject::Bool(true)), "true");
        assert_eq!(serialize_to_string(&CosObject::Integer(123)), "123");
        assert_eq!(serialize_to_string(&CosObject::Real(1.23)), "1.23");
    }

    #[test]
    fn serialize_name() {
        assert_eq!(serialize_to_string(&CosObject::Name(CosName::new(b"Type".to_vec()))), "/Type");
    }

    #[test]
    fn serialize_name_with_escape() {
        assert_eq!(serialize_to_string(&CosObject::Name(CosName::new(b"A B".to_vec()))), "/A#20B");
    }

    #[test]
    fn serialize_string() {
        assert_eq!(serialize_to_string(&CosObject::String(b"Hello".to_vec())), "(Hello)");
    }

    #[test]
    fn serialize_string_with_escape() {
        assert_eq!(serialize_to_string(&CosObject::String(b"()\\".to_vec())), "(\\(\\)\\\\)");
    }

    #[test]
    fn serialize_array() {
        let arr = CosObject::Array(vec![CosObject::Integer(1), CosObject::Integer(2)]);
        assert_eq!(serialize_to_string(&arr), "[1 2]");
    }

    #[test]
    fn serialize_dictionary() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Page".to_vec())));
        dict.insert(CosName::new(b"Count".to_vec()), CosObject::Integer(1));
        let obj = CosObject::Dictionary(dict);
        // Note: HashMap iteration order is not guaranteed, so we check for parts
        let s = serialize_to_string(&obj);
        assert!(s.starts_with("<<"));
        assert!(s.ends_with(" >>"));
        assert!(s.contains("/Type /Page"));
        assert!(s.contains("/Count 1"));
    }

    #[test]
    fn serialize_indirect_object() {
        let mut buffer = Vec::new();
        let id = ObjectId::new(1, 0);
        let obj = CosObject::Integer(42);
        {
            let mut s = Serializer::new(&mut buffer);
            s.write_indirect_object(id, &obj).unwrap();
        }
        let result = String::from_utf8(buffer).unwrap();
        assert_eq!(result, "1 0 obj\n42\nendobj\n");
    }
}
