//! AES encryption support for PDF.
//!
//! Provides AES-128 and AES-256 CBC encryption with random IV generation.
//! Used by the serializer to re-encrypt objects in AES-encrypted PDFs.

use aes::{Aes128, Aes256};
use block_padding::Pkcs7;
use cbc::Encryptor;
use cipher::{BlockEncryptMut, KeyIvInit};

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

/// AES-256-CBC encrypt with PKCS7 padding, returning only the ciphertext (no IV).
/// Used for computing /UE and /OE entries in Rev 6 encryption.
pub fn aes256_cbc_encrypt_noiv(key: &[u8], iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    use aes::Aes256Enc;
    use cipher::{BlockEncrypt, KeyInit};
    use digest::generic_array::GenericArray;

    let cipher = Aes256Enc::new_from_slice(key).expect("AES-256 key");

    // PKCS7 pad to next 16-byte boundary
    let block_count = (plaintext.len() + 15) / 16;
    let total = block_count * 16;
    let pad_byte = (total - plaintext.len()) as u8;
    let mut padded = plaintext.to_vec();
    padded.resize(total, pad_byte);

    let mut result = Vec::with_capacity(total);
    let mut prev = *iv;

    for chunk in padded.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        for j in 0..16 {
            block[j] ^= prev[j];
        }
        let mut ga = GenericArray::from(block);
        cipher.encrypt_block(&mut ga);
        let encrypted = ga.as_slice();
        result.extend_from_slice(encrypted);
        prev.copy_from_slice(encrypted);
    }
    result
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
        let decrypted =
            super::super::aes::aes_cbc_decrypt(key, &encrypted[..16], &encrypted[16..]).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn aes256_encrypt_decrypt_roundtrip() {
        let key = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20";
        let iv = b"\x21\x22\x23\x24\x25\x26\x27\x28\x29\x2a\x2b\x2c\x2d\x2e\x2f\x30";
        let plaintext = b"Hello, AES-256 CBC world! Pad this properly.";

        let encrypted = aes256_cbc_encrypt(key, iv, plaintext).unwrap();
        assert!(encrypted.len() > plaintext.len());

        let decrypted =
            super::super::aes::aes256_cbc_decrypt(key, &encrypted[..16], &encrypted[16..]).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn aes128_invalid_key_len_returns_none() {
        let key = b"short"; // not 16 bytes
        let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f";
        assert!(aes_cbc_encrypt(key, iv, b"data").is_none());
    }

    #[test]
    fn aes128_invalid_iv_len_returns_none() {
        let key = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10";
        let iv = b"short"; // not 16 bytes
        assert!(aes_cbc_encrypt(key, iv, b"data").is_none());
    }

    #[test]
    fn aes128_empty_plaintext_returns_none() {
        let key = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10";
        let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f";
        assert!(aes_cbc_encrypt(key, iv, b"").is_none());
    }

    #[test]
    fn aes256_invalid_key_len_returns_none() {
        let key = b"short"; // not 32 bytes
        let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f";
        assert!(aes256_cbc_encrypt(key, iv, b"data").is_none());
    }

    #[test]
    fn aes256_cbc_encrypt_noiv_ok() {
        let key = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f";
        let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f";
        let plaintext = b"Hello world!";
        let result = aes256_cbc_encrypt_noiv(key, iv, plaintext);
        assert!(!result.is_empty());
        assert!(result.len() >= plaintext.len());
    }

    #[test]
    fn aes256_encrypt_noiv_vs_standard() {
        let key = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f";
        let iv = b"\xaa\xbb\xcc\xdd\xee\xff\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99";
        let pt = b"Hello world! This is a test.  ";
        let standard = aes256_cbc_encrypt(key, iv, pt).unwrap();
        let noiv = aes256_cbc_encrypt_noiv(key, iv, pt);
        // standard prepends IV (16 bytes), noiv doesn't
        assert_eq!(noiv, &standard[16..]);
    }

    #[test]
    fn aes256_noiv_invalid_key_panics() {
        let short_key = b"too short";
        let iv = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f";
        let result = std::panic::catch_unwind(|| aes256_cbc_encrypt_noiv(short_key, iv, b"data"));
        assert!(result.is_err());
    }
}
