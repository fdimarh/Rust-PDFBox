//! MD5 hash using RustCrypto md-5 crate.
//!
//! Used by the PDF Standard Security Handler (Rev 2, 3, 4) for key derivation
//! and password validation.
//!
//! Reference: RFC 1321.

use digest::Digest;
use md5::Md5;

/// Computes the MD5 digest of `input` and returns the 16-byte hash.
pub fn md5(input: &[u8]) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(input);
    let result = hasher.finalize();
    result.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// RFC 1321 test vectors
    #[test]
    fn rfc1321_empty() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn rfc1321_a() {
        assert_eq!(hex(&md5(b"a")), "0cc175b9c0f1b6a831c399e269772661");
    }

    #[test]
    fn rfc1321_abc() {
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn rfc1321_message_digest() {
        assert_eq!(
            hex(&md5(b"message digest")),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
    }

    #[test]
    fn rfc1321_alphabet() {
        assert_eq!(
            hex(&md5(b"abcdefghijklmnopqrstuvwxyz")),
            "c3fcd3d76192e4007dfb496cca67e13b"
        );
    }

    #[test]
    fn rfc1321_alphanumeric() {
        assert_eq!(
            hex(&md5(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")),
            "d174ab98d277d9f5a5611c2c9f419d9f"
        );
    }

    #[test]
    fn deterministic() {
        assert_eq!(md5(b"pdf"), md5(b"pdf"));
    }

    #[test]
    fn different_inputs_different_output() {
        assert_ne!(md5(b"hello"), md5(b"world"));
    }
}

