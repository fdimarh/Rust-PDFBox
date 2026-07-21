//! PDF Standard Security Handler (Revisions 5 and 6)
//!
//! AES-256 encryption introduced in PDF 1.7 Extension Level 3 (Rev 5)
//! and standardized in PDF 2.0 / ISO 32000-2 (Rev 6).

use sha2::{Sha256, Digest};

/// Calculates the encryption key for Revision 5.
/// 
/// Revision 5 uses SHA-256 to hash the password and the validation salt 
/// stored in the first 8 bytes of the `O` or `U` entry.
pub fn compute_encryption_key_rev5(password: &[u8], validation_salt: &[u8]) -> Vec<u8> {
    // 1. Compute truncated password
    let trunc_pwd = if password.len() > 127 { &password[..127] } else { password };
    
    // 2. Compute SHA-256 of (truncated_pwd || validation_salt)
    let mut hasher = Sha256::new();
    hasher.update(trunc_pwd);
    hasher.update(validation_salt);
    
    hasher.finalize().to_vec()
}

/// Calculate the encryption key for Revision 6.
/// 
/// Revision 6 uses an iterated hashing algorithm based on SHA-256, SHA-384, and SHA-512.
pub fn compute_encryption_key_rev6(_password: &[u8], _validation_salt: &[u8]) -> Vec<u8> {
    // Stub for Rev 6 logic (ISO 32000-2 7.6.4.3.3)
    // The spec requires parsing the User/Owner dictionary and applying
    // AES-128 blocks to iteratively hash the key.
    
    // Fallback stub: return 32-byte empty vector until iterative hashing is wired
    vec![0u8; 32]
}