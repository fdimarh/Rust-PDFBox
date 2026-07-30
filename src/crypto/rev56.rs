//! Revision 5 and 6 encryption key support for AES-256 (PDF 2.0).
//!
//! ISO 32000-2 §7.6.4.3 (Algorithm 2.B — "Computing a hash")
//!
//! For R=6, the hash is computed via repeated AES-128-CBC (64 reps) + SHA-2.
//! The file encryption key is stored in /UE (encrypted with intermediate key).

use aes::{Aes128Enc, Aes256Dec};
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use digest::generic_array::GenericArray;
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::convert::TryInto;

/// Implements Algorithm 2.B from ISO 32000-2 §7.6.4.3.5 (qpdf's hash_V5).
///
/// 1. K = SHA-256(password || salt || udata)
/// 2. For R=6:
///    a. K1 = password || K || udata (NOT padded)
///    b. AES-128-CBC(key=K[0..15], IV=K[16..31]) encrypt K1 64 times via pipeline
///       Full blocks output, partial final block discarded
///    c. E_mod_3 = sum(bytes E[0..15]) % 3
///    d. K = SHA2_256/384/512(E)
///    e. If round ≥ 64 and E.last_byte ≤ round - 32 → done
/// 3. Result: K[0..31]
pub fn hash_v5(password: &[u8], salt: &[u8], udata: &[u8], revision: u8) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    hasher.update(udata);
    let mut k = hasher.finalize().to_vec();

    if revision < 6 {
        return k[..32].to_vec();
    }

    let mut round_number: u32 = 0;
    loop {
        round_number += 1;

        let mut k1 = Vec::with_capacity(password.len() + k.len() + udata.len());
        k1.extend_from_slice(password);
        k1.extend_from_slice(&k);
        k1.extend_from_slice(udata);

        let key16: [u8; 16] = k[..16].try_into().unwrap();
        let iv16: [u8; 16] = k[16..32].try_into().unwrap();
        let e = aes128_cbc_pipeline(&key16, &iv16, &k1, 64);

        let e_mod_3: u32 = e[..16].iter().map(|&b| b as u32).sum::<u32>() % 3;

        k = match e_mod_3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };

        if round_number >= 64 {
            let last_byte = e[e.len() - 1] as u32;
            if last_byte <= round_number - 32 {
                break;
            }
        }
    }

    k[..32].to_vec()
}

/// Simulate AES-128-CBC pipeline: write `data` through pipeline `repetitions` times.
fn aes128_cbc_pipeline(key: &[u8; 16], iv: &[u8; 16], data: &[u8], repetitions: u32) -> Vec<u8> {
    let cipher = Aes128Enc::new_from_slice(key).expect("AES-128 key");
    let mut e = Vec::new();
    let mut buffer = Vec::with_capacity(16);
    let mut prev = *iv;

    for _ in 0..repetitions {
        for &byte in data {
            buffer.push(byte);
            if buffer.len() == 16 {
                let mut block = [0u8; 16];
                block.copy_from_slice(&buffer);
                for j in 0..16 {
                    block[j] ^= prev[j];
                }
                let mut ga = GenericArray::from(block);
                cipher.encrypt_block(&mut ga);
                let encrypted = ga.as_slice();
                e.extend_from_slice(encrypted);
                prev.copy_from_slice(encrypted);
                buffer.clear();
            }
        }
    }
    // disablePadding: partial block discarded
    e
}

/// Recovers the file encryption key for Rev 6.
///
/// /U structure:
///   U[0..31]  = hash(password, validation_salt, "")
///   U[32..39] = validation_salt (8 bytes)
///   U[40..47] = key_salt (8 bytes)
pub fn recover_encryption_key_r6(
    password: &[u8],
    u_entry: &[u8],
    ue_entry: &[u8],
) -> Option<Vec<u8>> {
    if u_entry.len() < 48 || ue_entry.len() < 32 {
        return None;
    }

    let validation_salt = &u_entry[32..40];
    let key_salt = &u_entry[40..48];

    let expected_u = hash_v5(password, validation_salt, &[], 6);
    if expected_u[..32] != u_entry[..32] {
        return None;
    }

    let intermediate_key = hash_v5(password, key_salt, &[], 6);

    let zero_iv = [0u8; 16];
    let file_key = aes256_cbc_decrypt_no_pad(&intermediate_key[..32], &zero_iv, &ue_entry[..32]);
    Some(file_key)
}

pub fn recover_encryption_key_r6_owner(
    password: &[u8],
    o_entry: &[u8],
    u_entry: &[u8],
    oe_entry: &[u8],
) -> Option<Vec<u8>> {
    if o_entry.len() < 48 || oe_entry.len() < 32 {
        return None;
    }

    let validation_salt = &o_entry[32..40];
    let key_salt = &o_entry[40..48];

    // O = hash_v5(password, validation_salt, U) (udata = full U entry)
    let expected_o = hash_v5(password, validation_salt, u_entry, 6);
    if expected_o[..32] != o_entry[..32] {
        return None;
    }

    let intermediate_key = hash_v5(password, key_salt, u_entry, 6);

    let zero_iv = [0u8; 16];
    let file_key = aes256_cbc_decrypt_no_pad(&intermediate_key[..32], &zero_iv, &oe_entry[..32]);
    Some(file_key)
}

/// AES-256-CBC decrypt with no padding.
fn aes256_cbc_decrypt_no_pad(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Dec::new_from_slice(key).expect("AES-256 key");
    let block_count = ciphertext.len() / 16;
    let mut result = ciphertext.to_vec();
    let mut prev = [0u8; 16];
    prev.copy_from_slice(iv);

    for block_idx in 0..block_count {
        let start = block_idx * 16;
        let original_block: [u8; 16] = result[start..start + 16].try_into().unwrap();
        let block = &mut result[start..start + 16];

        let mut ga = GenericArray::clone_from_slice(&original_block);
        cipher.decrypt_block(&mut ga);
        let decrypted = ga.as_slice();

        for j in 0..16 {
            block[j] = decrypted[j] ^ prev[j];
        }

        prev = original_block;
    }

    result
}

pub fn compute_encryption_key_rev5(password: &[u8], validation_salt: &[u8]) -> Vec<u8> {
    hash_v5(password, validation_salt, &[], 5)
}

// Legacy stub
pub fn compute_encryption_key_rev6(
    _password: &[u8],
    _validation_salt: &[u8],
    _user_key: &[u8],
) -> Vec<u8> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_v5_r5_empty() {
        let result = hash_v5(b"", b"", &[], 5);
        assert_eq!(result.len(), 32);
    }
}
