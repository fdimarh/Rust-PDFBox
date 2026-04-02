//! Minimal DER / ASN.1 encoding helpers for CMS SignedData.
//!
//! Implements just enough of RFC 5652 to build a `adbe.pkcs7.detached`
//! CMS blob suitable for PDF digital signatures.
//!
//! Mirrors `org.bouncycastle.asn1.*` usage in Java PDFBox / rust_pdf_signing.
//!
//! # Structure of CMS SignedData (RFC 5652 §5)
//!
//! ```text
//! ContentInfo {
//!   contentType = id-signedData (1.2.840.113549.1.7.2)
//!   content [0] EXPLICIT SignedData {
//!     version           = 1
//!     digestAlgorithms  = SET { AlgorithmIdentifier { id-sha256 } }
//!     encapContentInfo  = EncapsulatedContentInfo { id-data, absent }
//!     certificates [0]  = cert DER bytes …
//!     signerInfos       = SET { SignerInfo { … } }
//!   }
//! }
//! ```

// ---------------------------------------------------------------------------
// Low-level DER primitives
// ---------------------------------------------------------------------------

/// Encode a DER length.
pub fn der_length(n: usize) -> Vec<u8> {
    if n < 0x80 {
        vec![n as u8]
    } else if n <= 0xFF {
        vec![0x81, n as u8]
    } else if n <= 0xFFFF {
        vec![0x82, (n >> 8) as u8, n as u8]
    } else {
        let b3 = (n >> 16) as u8;
        let b2 = (n >> 8)  as u8;
        let b1 =  n         as u8;
        vec![0x83, b3, b2, b1]
    }
}

/// Wrap `content` in a DER TLV with tag `tag`.
pub fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(der_length(content.len()));
    out.extend_from_slice(content);
    out
}

/// DER SEQUENCE.
pub fn der_seq(content: &[u8]) -> Vec<u8> { der_tlv(0x30, content) }

/// DER SET.
pub fn der_set(content: &[u8]) -> Vec<u8> { der_tlv(0x31, content) }

/// DER [n] EXPLICIT context tag.
pub fn der_ctx_explicit(n: u8, content: &[u8]) -> Vec<u8> {
    der_tlv(0xa0 | n, content)
}

/// DER [n] IMPLICIT context tag (constructed).
pub fn der_ctx_implicit_constructed(n: u8, content: &[u8]) -> Vec<u8> {
    der_tlv(0xa0 | n, content)
}

/// DER INTEGER from a big-endian byte slice.
pub fn der_integer(bytes: &[u8]) -> Vec<u8> {
    // Strip leading zeros, but always keep at least one byte.
    let stripped = bytes.iter().position(|&b| b != 0).map(|i| &bytes[i..]).unwrap_or(bytes);
    // Add leading 0x00 if high bit set (would make it negative otherwise).
    let needs_pad = stripped.first().map(|&b| b & 0x80 != 0).unwrap_or(false);
    let mut body = Vec::new();
    if needs_pad { body.push(0x00); }
    body.extend_from_slice(stripped);
    der_tlv(0x02, &body)
}

/// DER OCTET STRING.
pub fn der_octet_string(bytes: &[u8]) -> Vec<u8> { der_tlv(0x04, bytes) }

/// DER NULL.
pub fn der_null() -> Vec<u8> { vec![0x05, 0x00] }

/// DER OID from dot-notation string.
///
/// Only the OIDs used in CMS/PDF signing need to be supported.
pub fn der_oid(oid: &str) -> Vec<u8> {
    let arcs: Vec<u64> = oid.split('.').map(|s| s.parse().unwrap()).collect();
    let mut body: Vec<u8> = Vec::new();
    // First two arcs encoded as 40*a0 + a1
    body.push((arcs[0] * 40 + arcs[1]) as u8);
    for &arc in &arcs[2..] {
        // Base-128 encoding
        if arc == 0 {
            body.push(0);
        } else {
            let mut tmp: Vec<u8> = Vec::new();
            let mut v = arc;
            while v > 0 {
                tmp.push((v & 0x7f) as u8);
                v >>= 7;
            }
            tmp.reverse();
            for i in 0..tmp.len() - 1 {
                body.push(tmp[i] | 0x80);
            }
            body.push(*tmp.last().unwrap());
        }
    }
    der_tlv(0x06, &body)
}

/// DER PrintableString / UTF8String — we just use UTF8String (tag 0x0c).
pub fn der_utf8_string(s: &str) -> Vec<u8> { der_tlv(0x0c, s.as_bytes()) }

// ---------------------------------------------------------------------------
// Well-known OID strings
// ---------------------------------------------------------------------------

pub const OID_DATA:             &str = "1.2.840.113549.1.7.1";
pub const OID_SIGNED_DATA:      &str = "1.2.840.113549.1.7.2";
pub const OID_SHA256:           &str = "2.16.840.1.101.3.4.2.1";
pub const OID_RSA_ENCRYPTION:   &str = "1.2.840.113549.1.1.1";
pub const OID_SHA256_WITH_RSA:  &str = "1.2.840.113549.1.1.11";
pub const OID_ECDSA_WITH_SHA256:&str = "1.2.840.10045.4.3.2";
pub const OID_CONTENT_TYPE:     &str = "1.2.840.113549.1.9.3";
pub const OID_MESSAGE_DIGEST:   &str = "1.2.840.113549.1.9.4";
pub const OID_SIGNING_TIME:     &str = "1.2.840.113549.1.9.5";
/// id-smime-aa-signingCertificateV2  (RFC 5035 / ESS)
pub const OID_SIGNING_CERT_V2:  &str = "1.2.840.113549.1.9.16.2.47";

// ---------------------------------------------------------------------------
// AlgorithmIdentifier
// ---------------------------------------------------------------------------

/// `AlgorithmIdentifier { algorithm OID, parameters NULL }`.
pub fn alg_id_sha256() -> Vec<u8> {
    let mut body = der_oid(OID_SHA256);
    body.extend(der_null());
    der_seq(&body)
}

/// `AlgorithmIdentifier { rsaEncryption, NULL }`.
pub fn alg_id_rsa() -> Vec<u8> {
    let mut body = der_oid(OID_RSA_ENCRYPTION);
    body.extend(der_null());
    der_seq(&body)
}

/// `AlgorithmIdentifier { sha256WithRSAEncryption, NULL }`.
pub fn alg_id_sha256_with_rsa() -> Vec<u8> {
    let mut body = der_oid(OID_SHA256_WITH_RSA);
    body.extend(der_null());
    der_seq(&body)
}

/// `AlgorithmIdentifier { ecdsaWithSHA256 }` (no parameters for ECDSA).
pub fn alg_id_ecdsa_sha256() -> Vec<u8> {
    der_seq(&der_oid(OID_ECDSA_WITH_SHA256))
}

// ---------------------------------------------------------------------------
// IssuerAndSerialNumber — extracted from a certificate DER blob
// ---------------------------------------------------------------------------

/// Extracts issuer Name bytes and serial number bytes from a DER certificate.
///
/// Minimal parser — walks the outermost TBSCertificate SEQUENCE to find:
/// - version [0] EXPLICIT (optional, skip)
/// - serialNumber INTEGER
/// - signature AlgorithmIdentifier
/// - issuer Name
pub fn issuer_and_serial(cert_der: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    // Outer SEQUENCE (Certificate)
    let (_, outer_body) = parse_tlv(cert_der)?;
    // Inner SEQUENCE (TBSCertificate)
    let (_, tbs_body) = parse_tlv(outer_body)?;
    let mut pos = 0;
    // Optional version [0]
    if tbs_body[pos] == 0xa0 {
        let (used, _) = parse_tlv_at(tbs_body, pos)?;
        pos += used;
    }
    // serialNumber INTEGER
    let serial_start = pos;
    let (serial_used, _) = parse_tlv_at(tbs_body, pos)?;
    let serial_bytes = &tbs_body[serial_start..serial_start + serial_used];
    pos += serial_used;
    // signature AlgorithmIdentifier — skip
    let (alg_used, _) = parse_tlv_at(tbs_body, pos)?;
    pos += alg_used;
    // issuer Name SEQUENCE
    let issuer_start = pos;
    let (issuer_used, _) = parse_tlv_at(tbs_body, pos)?;
    let issuer_bytes = &tbs_body[issuer_start..issuer_start + issuer_used];

    Some((issuer_bytes.to_vec(), serial_bytes.to_vec()))
}

/// Parse one TLV at position `pos` in `data`.
/// Returns `(total_bytes_consumed, &value_bytes)`.
pub fn parse_tlv_at(data: &[u8], pos: usize) -> Option<(usize, &[u8])> {
    let (consumed, val) = parse_tlv(&data[pos..])?;
    Some((consumed, val))
}

/// Parse one TLV from the start of `data`.
/// Returns `(total_bytes_consumed, &value_bytes)`.
pub fn parse_tlv(data: &[u8]) -> Option<(usize, &[u8])> {
    if data.is_empty() { return None; }
    let mut pos = 1usize; // skip tag
    let len = if data[pos] & 0x80 == 0 {
        let l = data[pos] as usize;
        pos += 1;
        l
    } else {
        let n = (data[pos] & 0x7f) as usize;
        pos += 1;
        let mut l = 0usize;
        for _ in 0..n { l = (l << 8) | data[pos] as usize; pos += 1; }
        l
    };
    Some((pos + len, &data[pos..pos + len]))
}

