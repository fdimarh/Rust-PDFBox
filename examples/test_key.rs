//! Quick test for Rev 6 key derivation and AES-256 encrypt/decrypt.
//! Uses only deps already available in rust-pdfbox.

use sha2::{Digest, Sha256, Sha384, Sha512};

fn main() {
    let u_entry_hex_full = "a3c7058bc5e93fcd9119925facd687151f87ffd80deef556c30e451d5868aa5211edeabdb166277f086472d5642824f8";
    let u_entry: Vec<u8> = (0..u_entry_hex_full.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&u_entry_hex_full[i..i+2], 16).unwrap())
        .collect();
    let validation_salt = &u_entry[..8];
    let key_salt = &u_entry[8..16];
    let password = b"admin123";

    // Compute file key using our rev56 algorithm
    let file_key = {
        let mut data = Vec::new();
        data.extend_from_slice(password);
        data.extend_from_slice(validation_salt);
        data.extend_from_slice(key_salt);
        let mut k = Sha256::digest(&data).to_vec();

        let iterations = std::cmp::max(64, (k[0] as u32) + 64) as usize;
        for i in 0..iterations.min(64) {
            let algo = k[0] % 3;
            let mut k1 = Vec::new();
            k1.extend_from_slice(&k);
            k1.extend_from_slice(password);
            k1.extend_from_slice(&u_entry);
            k1.push((i & 0xFF) as u8);
            k1.push(((i >> 8) & 0xFF) as u8);
            k1.push(((i >> 16) & 0xFF) as u8);

            k = match algo {
                0 => Sha256::digest(&k1).to_vec(),
                1 => Sha384::digest(&k1).to_vec(),
                _ => Sha512::digest(&k1).to_vec(),
            };
        }
        k[..32].to_vec()
    };

    println!("File key: {}", file_key.iter().map(|b| format!("{:02x}", b)).collect::<String>());

    // Test AES-256 encrypt/decrypt using the SAME approach as aes_encrypt.rs
    // But fixing the key.into() issue by using GenericArray from digest crate
    use digest::generic_array::GenericArray;
    use aes::Aes256;
    use cbc::Encryptor;
    use cipher::{KeyIvInit, BlockEncryptMut};
    use block_padding::Pkcs7;

    let plaintext = b"Signature1";
    let mut iv = [0u8; 16];
    rand::thread_rng().fill(&mut iv);

    // CORRECT way: use GenericArray::from_slice
    let key_ga = GenericArray::from_slice(&file_key);
    let iv_ga = GenericArray::from_slice(&iv);

    let cipher = Encryptor::<Aes256>::new(key_ga, iv_ga);
    let mut buf = plaintext.to_vec();
    buf.resize(buf.len() + 16, 0);
    
    match cipher.encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len()) {
        Ok(enc) => {
            let mut result = Vec::with_capacity(16 + enc.len());
            result.extend_from_slice(&iv);
            result.extend_from_slice(enc);
            let hex_str: String = result.iter().map(|b| format!("{:02x}", b)).collect();
            println!("Encrypted: {} ({} bytes)", hex_str, result.len());
        }
        Err(e) => println!("Encrypt failed: {:?}", e),
    }

    // Also test the WRONG way - what does key.into() with &[u8] actually do?
    // Let's see if the Encryptor::<Aes256>::new(key.into(), iv.into()) compiles
    // when key is &[u8] and not &[u8; 32]
}
