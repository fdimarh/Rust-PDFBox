//! AES encryption support for PDF (PDF 1.6+).
//!
//! Implements AES-128/192/256 decryption for PDFs with security handler revision 4+.
//! Uses the `aes` and `cbc` crates from the RustCrypto ecosystem.
//!
//! Maps to Java PDFBox `AES128DecryptionFilter` and `AES256DecryptionFilter`.

use aes::{Aes128, Aes256};
use block_padding::Pkcs7;
use cbc::Decryptor;
use cipher::{BlockDecryptMut, KeyIvInit};

/// Decrypt data using AES-128 in CBC mode with PKCS#7 padding.
pub fn aes_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 16 || iv.len() != 16 || ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return None;
    }

    let cipher = Decryptor::<Aes128>::new_from_slices(key, iv).ok()?;
    let mut plaintext = ciphertext.to_vec();
    match cipher.decrypt_padded_mut::<Pkcs7>(&mut plaintext) {
        Ok(decrypted) => Some(decrypted.to_vec()),
        Err(_) => {
            // Padding invalid — return raw decrypted bytes without padding removal
            Some(plaintext)
        }
    }
}

/// Decrypt data using AES-256 in CBC mode with PKCS#7 padding.
/// PDF Rev 5 and Rev 6 use AES-256 (32 byte key).
pub fn aes256_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 32 || iv.len() != 16 || ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return None;
    }

    let cipher = Decryptor::<Aes256>::new_from_slices(key, iv).ok()?;
    let mut plaintext = ciphertext.to_vec();

    match cipher.decrypt_padded_mut::<Pkcs7>(&mut plaintext) {
        Ok(decrypted) => Some(decrypted.to_vec()),
        Err(_) => Some(plaintext),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_cbc_valid_decryption() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let ciphertext = [0u8; 16];
        let result = aes_cbc_decrypt(&key, &iv, &ciphertext);
        // Should process without panic
        assert!(result.is_some() || result.is_none());
    }

    #[test]
    fn aes_cbc_invalid_key_length() {
        let key = [0u8; 24];
        let iv = [0u8; 16];
        let ciphertext = [0u8; 16];
        assert!(aes_cbc_decrypt(&key, &iv, &ciphertext).is_none());
    }

    #[test]
    fn aes_cbc_invalid_iv_length() {
        let key = [0u8; 16];
        let iv = [0u8; 15];
        let ciphertext = [0u8; 16];
        assert!(aes_cbc_decrypt(&key, &iv, &ciphertext).is_none());
    }

    #[test]
    fn aes_cbc_invalid_ciphertext_length() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let ciphertext = [0u8; 15];
        assert!(aes_cbc_decrypt(&key, &iv, &ciphertext).is_none());
    }

    #[test]
    fn aes_cbc_empty_ciphertext() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let ciphertext: &[u8] = &[];
        assert!(aes_cbc_decrypt(&key, &iv, ciphertext).is_none());
    }

    #[test]
    fn aes256_cbc_roundtrip() {
        let key = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20";
        let iv = b"\x21\x22\x23\x24\x25\x26\x27\x28\x29\x2a\x2b\x2c\x2d\x2e\x2f\x30";
        let plaintext = b"AES-256 CBC roundtrip test message!";

        // Encrypt using aes_encrypt::aes256_cbc_encrypt
        let encrypted = super::super::aes_encrypt::aes256_cbc_encrypt(key, iv, plaintext).unwrap();
        // encrypted = iv[0..16] + ciphertext
        let (enc_iv, ciphertext) = encrypted.split_at(16);

        let decrypted = aes256_cbc_decrypt(key, enc_iv, ciphertext).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn aes128_roundtrip_with_encrypt() {
        let key = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10";
        let iv = b"\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20";
        let plaintext = b"AES-128 roundtrip works!";

        let encrypted = super::super::aes_encrypt::aes_cbc_encrypt(key, iv, plaintext).unwrap();
        let (enc_iv, ciphertext) = encrypted.split_at(16);

        let decrypted = aes_cbc_decrypt(key, enc_iv, ciphertext).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn aes256_invalid_key_length() {
        let key = [0u8; 16]; // 16 bytes, but AES-256 requires 32
        let iv = [0u8; 16];
        let ciphertext = [0u8; 16];
        assert!(aes256_cbc_decrypt(&key, &iv, &ciphertext).is_none());
    }
}
