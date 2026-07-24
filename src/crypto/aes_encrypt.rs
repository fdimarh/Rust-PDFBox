//! AES encryption support for PDF.
//!
//! Provides AES-128 and AES-256 CBC encryption with random IV generation.
//! Used by the serializer to re-encrypt objects in AES-encrypted PDFs.

use aes::{Aes128, Aes256};
use cbc::Encryptor;
use cipher::{KeyIvInit, BlockEncryptMut};
use block_padding::Pkcs7;

/// Encrypt data using AES-128 in CBC mode with PKCS#7 padding.
/// Returns IV + ciphertext (PDF AES encryption format).
pub fn aes_cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 16 || iv.len() != 16 || plaintext.is_empty() {
        return None;
    }

    let cipher = Encryptor::<Aes128>::new_from_slices(key, iv).ok()?;
    let mut buf = plaintext.to_vec();
    buf.resize(buf.len() + 16, 0);
    match cipher.encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len()) {
        Ok(encrypted) => {
            // Return IV + ciphertext (for PDF AES encryption format)
            let mut result = Vec::with_capacity(iv.len() + encrypted.len());
            result.extend_from_slice(iv);
            result.extend_from_slice(encrypted);
            Some(result)
        }
        Err(_) => None,
    }
}

/// Encrypt data using AES-256 in CBC mode with PKCS#7 padding.
/// Returns IV + ciphertext (PDF AES encryption format).
pub fn aes256_cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 32 || iv.len() != 16 || plaintext.is_empty() {
        return None;
    }

    let cipher = Encryptor::<Aes256>::new_from_slices(key, iv).ok()?;
    let mut buf = plaintext.to_vec();
    buf.resize(buf.len() + 16, 0);
    match cipher.encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len()) {
        Ok(encrypted) => {
            let mut result = Vec::with_capacity(iv.len() + encrypted.len());
            result.extend_from_slice(iv);
            result.extend_from_slice(encrypted);
            Some(result)
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes128_encrypt_decrypt_roundtrip() {
        let key = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10";
        let iv = b"\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20";
        let plaintext = b"Hello, AES-128 CBC world!";

        let encrypted = aes_cbc_encrypt(key, iv, plaintext).unwrap();
        assert!(encrypted.len() > plaintext.len());

        // Decrypt back (IV is first 16 bytes, then ciphertext)
        let decrypted = super::super::aes::aes_cbc_decrypt(key, &encrypted[..16], &encrypted[16..]).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn aes256_encrypt_decrypt_roundtrip() {
        let key = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20";
        let iv = b"\x21\x22\x23\x24\x25\x26\x27\x28\x29\x2a\x2b\x2c\x2d\x2e\x2f\x30";
        let plaintext = b"Hello, AES-256 CBC world! Pad this properly.";

        let encrypted = aes256_cbc_encrypt(key, iv, plaintext).unwrap();
        assert!(encrypted.len() > plaintext.len());

        let decrypted = super::super::aes::aes256_cbc_decrypt(key, &encrypted[..16], &encrypted[16..]).unwrap();
        assert_eq!(&decrypted, plaintext);
    }
}
