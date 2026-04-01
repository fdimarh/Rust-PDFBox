//! RC4 stream cipher — pure Rust, no external crates.
//!
//! Used by PDF Standard Security Handler revisions 2 and 3 (40-bit and 128-bit
//! RC4 encryption). Maps to Java PDFBox `ARCFourEncryption`.
//!
//! RC4 is a symmetric stream cipher: encrypt and decrypt are the same operation.

/// RC4 stream cipher state.
pub struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    /// Initialises RC4 with the given key (1–256 bytes).
    pub fn new(key: &[u8]) -> Self {
        assert!(!key.is_empty() && key.len() <= 256, "RC4 key must be 1-256 bytes");
        let mut s: [u8; 256] = core::array::from_fn(|i| i as u8);
        let mut j: u8 = 0;
        for i in 0u8..=255 {
            j = j.wrapping_add(s[i as usize]).wrapping_add(key[i as usize % key.len()]);
            s.swap(i as usize, j as usize);
        }
        Self { s, i: 0, j: 0 }
    }

    /// Processes one byte (XOR with keystream).
    #[inline]
    pub fn next_byte(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[self.i as usize]);
        self.s.swap(self.i as usize, self.j as usize);
        self.s[self.s[self.i as usize].wrapping_add(self.s[self.j as usize]) as usize]
    }

    /// Encrypts/decrypts `data` in-place.
    pub fn apply_keystream(&mut self, data: &mut [u8]) {
        for byte in data.iter_mut() {
            *byte ^= self.next_byte();
        }
    }

    /// Convenience: encrypt/decrypt `plaintext` → new Vec.
    pub fn crypt(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let mut rc4 = Self::new(key);
        let mut out = plaintext.to_vec();
        rc4.apply_keystream(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6229 test vector — Key "Key", plaintext "Plaintext"
    #[test]
    fn rfc_vector_key_plaintext() {
        let key = b"Key";
        let pt = b"Plaintext";
        let ct = Rc4::crypt(key, pt);
        // Known RC4("Key", "Plaintext") ciphertext
        let expected: &[u8] = &[0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3];
        assert_eq!(ct, expected);
    }

    /// RFC 6229 test vector — Key "Wiki", plaintext "pedia"
    #[test]
    fn rfc_vector_wiki_pedia() {
        let key = b"Wiki";
        let pt = b"pedia";
        let ct = Rc4::crypt(key, pt);
        let expected: &[u8] = &[0x10, 0x21, 0xBF, 0x04, 0x20];
        assert_eq!(ct, expected);
    }

    /// RC4 is its own inverse — decrypt(encrypt(x)) == x
    #[test]
    fn encrypt_then_decrypt_roundtrip() {
        let key = b"secret";
        let plaintext = b"Hello, PDF!";
        let ciphertext = Rc4::crypt(key, plaintext);
        let recovered = Rc4::crypt(key, &ciphertext);
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn empty_plaintext() {
        let result = Rc4::crypt(b"key", b"");
        assert!(result.is_empty());
    }

    #[test]
    fn single_byte() {
        let ct = Rc4::crypt(b"k", &[0x00]);
        let rt = Rc4::crypt(b"k", &ct);
        assert_eq!(rt, &[0x00]);
    }

    #[test]
    fn different_keys_produce_different_output() {
        let pt = b"test";
        let ct1 = Rc4::crypt(b"key1", pt);
        let ct2 = Rc4::crypt(b"key2", pt);
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn apply_keystream_matches_crypt() {
        let key = b"mykey";
        let data = b"some data here";
        let via_crypt = Rc4::crypt(key, data);
        let mut buf = data.to_vec();
        Rc4::new(key).apply_keystream(&mut buf);
        assert_eq!(buf, via_crypt);
    }
}

