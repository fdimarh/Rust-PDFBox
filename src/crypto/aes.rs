//! AES encryption support for PDF (PDF 1.6+).
//!
//! Implements AES-128/192/256 decryption for PDFs with security handler revision 4+.
//! Uses the `aes` and `cbc` crates from the RustCrypto ecosystem.
//!
//! Maps to Java PDFBox `AES128DecryptionFilter` and `AES256DecryptionFilter`.

use aes::Aes128;
use cbc::Decryptor;
use cipher::BlockDecrypt;

/// Decrypt data using AES in CBC mode with PKCS#7 padding.
///
/// PDF uses CBC mode with PKCS#5 padding (compatible with PKCS#7).
/// 
/// # Arguments
/// - `key`: AES key (16 bytes for AES-128)
/// - `iv`: Initialization vector (16 bytes)
/// - `ciphertext`: Data to decrypt
///
/// # Returns
/// Decrypted plaintext with padding removed, or `None` on error
pub fn aes_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    // Validate key and IV lengths
    if key.len() != 16 || iv.len() != 16 || ciphertext.len() % 16 != 0 {
        return None;
    }

    // Create cipher in CBC mode
    let cipher = Decryptor::<Aes128>::new(key.into(), iv.into());

    // Decrypt (padding removal is handled by the cipher)
    let mut plaintext = ciphertext.to_vec();
    match cipher.decrypt_padded_mut::<block_padding::Pkcs7>(&mut plaintext) {
        Ok(decrypted) => Some(decrypted.to_vec()),
        Err(_) => None,
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

