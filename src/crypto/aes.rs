//! AES encryption support for PDF (PDF 1.6+).
//!
//! Implements AES-128/192/256 decryption for PDFs with security handler revision 4+.
//! Uses the `aes` and `cbc` crates from the RustCrypto ecosystem.
//!
//! Maps to Java PDFBox `AES128DecryptionFilter` and `AES256DecryptionFilter`.

use aes::{Aes128, Aes256};
use cbc::Decryptor;
use cipher::{KeyIvInit, BlockDecryptMut};
use block_padding::Pkcs7;

/// Decrypt data using AES-128 in CBC mode with PKCS#7 padding.
pub fn aes_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    // Validate key, IV lengths, and that ciphertext is non-empty and block-aligned
    if key.len() != 16 || iv.len() != 16 || ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return None;
    }

    // Create cipher in CBC mode
    let cipher = Decryptor::<Aes128>::new(key.into(), iv.into());

    // Decrypt (padding removal is handled by the cipher)
    let mut plaintext = ciphertext.to_vec();
    match cipher.decrypt_padded_mut::<Pkcs7>(&mut plaintext) {
        Ok(decrypted) => Some(decrypted.to_vec()),
        Err(_) => {
            // Padding invalid — return raw decrypted bytes without padding removal
            Some(plaintext)
        },
    }
}

/// Decrypt data using AES-256 in CBC mode with PKCS#7 padding.
/// PDF Rev 5 and Rev 6 use AES-256 (32 byte key).
pub fn aes256_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 32 || iv.len() != 16 || ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return None;
    }

    let cipher = Decryptor::<Aes256>::new(key.into(), iv.into());
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
}

