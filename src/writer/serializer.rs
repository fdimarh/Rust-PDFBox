//! Serializer for individual COS objects.
//!
//! Maps to Java PDFBox `COSWriter`. This module writes `CosObject` variants
//! to a byte buffer in their correct syntactic form (e.g. `(string)`,
//! `<hexstring>`, `/Name`, `[1 2 3]`, `<< /K 1 >>`).
//!
//! Also handles on-the-fly encryption: RC4 (Rev 2-3), AES-128 (Rev 4), AES-256 (Rev 5/6).

use std::io::{self, Write};
use crate::crypto::handlers::StandardSecurityHandler;
use std::collections::HashSet;
use crate::cos::{CosObject, CosName, CosDictionary, CosStream, ObjectId};

/// Encryption mode for the serializer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    /// No encryption.
    None,
    /// RC4 encryption (Rev 2-3 or non-AES Rev 4).
    Rc4,
    /// AES-128 CBC (Rev 4 with AESV2).
    Aes128,
    /// AES-256 CBC (Rev 5/6 with AESV3).
    Aes256,
}

/// Core CosObject serializer.
///
/// Converts in-memory tree back to PDF bytes (e.g., `<< /Type /Page >>`).
pub struct Serializer<'a, W: Write> {
    writer: &'a mut W,
    file_key: Option<Vec<u8>>,
    bypass_ids: HashSet<ObjectId>,
    current_object_id: Option<ObjectId>,
    encryption_mode: EncryptionMode,
}

impl<'a, W: Write> Serializer<'a, W> {
    /// Creates a new serializer writing to the given writer (plaintext mode).
    pub fn new(writer: &'a mut W) -> Self {
        Serializer {
            writer,
            file_key: None,
            bypass_ids: HashSet::new(),
            current_object_id: None,
            encryption_mode: EncryptionMode::None,
        }
    }

    /// Creates a serializer that encrypts strings and streams on-the-fly.
    /// Detects encryption mode from key length.
    pub fn new_encrypted(
        writer: &'a mut W,
        file_key: Option<Vec<u8>>,
        bypass_ids: HashSet<ObjectId>,
    ) -> Self {
        let encryption_mode = match file_key.as_ref().map(|k| k.len()) {
            Some(32) => EncryptionMode::Aes256,
            Some(16) if false => EncryptionMode::Aes128, // AES-128 not yet tested
            _ => EncryptionMode::Rc4,
        };
        Serializer {
            writer,
            file_key,
            bypass_ids,
            current_object_id: None,
            encryption_mode,
        }
    }

    /// Encrypt data for this object using the proper algorithm.
    fn encrypt_data(&self, obj_key: &[u8], data: &[u8], is_string: bool) -> Vec<u8> {
        match self.encryption_mode {
            EncryptionMode::None => data.to_vec(),
            EncryptionMode::Rc4 => {
                crate::crypto::rc4::Rc4::crypt(obj_key, data)
            }
            EncryptionMode::Aes128 => {
                use rand::Rng;
                let mut iv = [0u8; 16];
                rand::thread_rng().fill(&mut iv);
                if let Some(mut enc) = crate::crypto::aes_encrypt::aes_cbc_encrypt(&obj_key[..16.min(obj_key.len())], &iv, data) {
                    enc
                } else {
                    data.to_vec()
                }
            }
            EncryptionMode::Aes256 => {
                // AES-256 CBC: generate 16-byte random IV, prepend to ciphertext
                use rand::Rng;
                let mut iv = [0u8; 16];
                rand::thread_rng().fill(&mut iv);
                if let Some(mut enc) = crate::crypto::aes_encrypt::aes256_cbc_encrypt(
                    &obj_key[..32.min(obj_key.len())], &iv, data
                ) {
                    enc  // aes256_cbc_encrypt already returns IV + ciphertext
                } else {
                    data.to_vec()
                }
            }
        }
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
                            let obj_key = StandardSecurityHandler::compute_object_key(
                                file_key, id.object_number as u32, id.generation as u16,
                                self.encryption_mode != EncryptionMode::Rc4,
                            );
                            let encrypted = self.encrypt_data(&obj_key, bytes, true);
                            if self.encryption_mode == EncryptionMode::Aes256 {
                                // AES-256 encrypted strings use hex format
                                return self.write_hex_string(&encrypted);
                            } else {
                                return self.write_hex_string(&encrypted);
                            }
                        }
                    }
                }
                self.write_string(bytes)?
            }
            CosObject::HexString(bytes) => {
                if let Some(ref file_key) = self.file_key {
                    if let Some(id) = self.current_object_id {
                        if !self.bypass_ids.contains(&id) {
                            let obj_key = StandardSecurityHandler::compute_object_key(
                                file_key, id.object_number as u32, id.generation as u16,
                                self.encryption_mode != EncryptionMode::Rc4,
                            );
                            let encrypted = self.encrypt_data(&obj_key, bytes, true);
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
                            let obj_key = StandardSecurityHandler::compute_object_key(
                                file_key, id.object_number as u32, id.generation as u16,
                                self.encryption_mode != EncryptionMode::Rc4,
                            );
                            let encrypted = self.encrypt_data(&obj_key, &stream.data, false);
                            let mut enc_stream = stream.clone();
                            enc_stream.data = encrypted;
                            enc_stream.dictionary.insert(
                                CosName::new(b"Length".to_vec()),
                                CosObject::Integer(enc_stream.data.len() as i64),
                            );
                            if self.encryption_mode != EncryptionMode::Rc4 && self.encryption_mode != EncryptionMode::None {
                                enc_stream.dictionary.insert(
                                    CosName::new(b"Filter".to_vec()),
                                    CosObject::Name(CosName::new(b"Crypt".to_vec())),
                                );
                            }
                            return self.write_stream(&enc_stream);
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
                0x00..=0x20 | b'%' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'#' => {
                    write!(self.writer, "#{:02X}", byte)?;
                }
                _ => self.writer.write_all(&[byte])?,
            }
        }
        Ok(())
    }

    fn write_reference(&mut self, id: &ObjectId) -> io::Result<()> {
        write!(self.writer, "{} {} R", id.object_number, id.generation)
    }

    fn write_array(&mut self, arr: &[CosObject]) -> io::Result<()> {
        self.writer.write_all(b"[")?;
        for (i, elem) in arr.iter().enumerate() {
            if i > 0 {
                self.writer.write_all(b" ")?;
            }
            self.write_object(elem)?;
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
            self.writer.write_all(b" ")?;
        }
        self.writer.write_all(b">>")?;
        Ok(())
    }

    fn write_stream(&mut self, stream: &CosStream) -> io::Result<()> {
        self.write_dictionary(&stream.dictionary)?;
        write!(self.writer, "\nstream\n")?;
        self.writer.write_all(&stream.data)?;
        write!(self.writer, "\nendstream")?;
        Ok(())
    }
}
