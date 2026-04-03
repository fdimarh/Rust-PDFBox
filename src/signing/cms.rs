//! CMS builder — exact same logic as rust_pdf_signing `digitally_sign_document`.
//!
//! # How it works (mirrors rust_pdf_signing `digitally_sign_document`)
//!
//! 1. Parse the PEM private key and certificate chain using `x509-certificate`.
//! 2. Build a `SignerBuilder` from the private key + signer cert.
//! 3. Add the **`id-smime-aa-signingCertificateV2`** signed attribute
//!    (required by Adobe/Foxit for signature validity).
//! 4. Feed the **raw signed byte ranges** (not a pre-computed digest) to
//!    `SignedDataBuilder::content_external()`.
//! 5. The crate internally computes SHA-256, builds all signed attributes
//!    (`contentType`, `signingTime`, `messageDigest`), signs them, and
//!    emits a complete DER-encoded CMS `ContentInfo { SignedData }`.
//!
//! # Output structure (adbe.pkcs7.detached)
//!
//! ```text
//! ContentInfo
//!   contentType = id-signedData
//!   content     = SignedData
//!     version           = 1
//!     digestAlgorithms  = { sha-256 }
//!     encapContentInfo  = { id-data }          ← no content (detached)
//!     certificates      = [signer + chain]
//!     signerInfos       = {
//!       SignerInfo
//!         version            = 1
//!         sid                = IssuerAndSerialNumber
//!         digestAlgorithm    = sha-256
//!         signedAttrs        = {
//!           contentType                 = id-data
//!           signingTime                 = UTCTime
//!           messageDigest               = SHA-256(signed_bytes)
//!           signingCertificateV2        = ESSCertIDv2 { SHA-256(signer_cert) }
//!         }
//!         signatureAlgorithm = sha256WithRSA / ecdsaWithSHA256
//!         signature          = <cryptographic signature>
//!     }
//! ```

use crate::PdfError;
use crate::signing::asn1::*;

// ---------------------------------------------------------------------------
// CMS build options
// ---------------------------------------------------------------------------

/// Controls which SubFilter and optional attributes to include.
#[derive(Debug, Clone)]
pub struct CmsOptions {
    /// SubFilter string to embed: `"adbe.pkcs7.detached"` or `"ETSI.CAdES.detached"`.
    pub sub_filter: &'static str,
    /// RFC 3161 TSA URL for a signature timestamp (unsigned attribute).
    pub timestamp_url: Option<String>,
}

impl Default for CmsOptions {
    fn default() -> Self {
        Self {
            sub_filter: "adbe.pkcs7.detached",
            timestamp_url: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Public: build_cms_signed_data
// ---------------------------------------------------------------------------

/// Build a DER-encoded CMS `SignedData` blob exactly like rust_pdf_signing.
///
/// * `signed_content`  — raw bytes of the two signed ranges concatenated.
///   The crate internally SHA-256s these and builds `messageDigest`.
/// * `cert_chain_pem`  — PEM cert chain (signer cert FIRST).
/// * `private_key_pem` — PKCS#8 PEM private key (RSA or EC).
/// * `cms_opts`        — optional sub-filter / TSA URL overrides.
pub fn build_cms_signed_data(
    signed_content: &[u8],
    cert_chain_pem: &str,
    private_key_pem: &str,
) -> Result<Vec<u8>, PdfError> {
    build_cms_signed_data_with_opts(
        signed_content,
        cert_chain_pem,
        private_key_pem,
        &CmsOptions::default(),
    )
}

/// Like `build_cms_signed_data` but allows passing TSA URL and other options.
pub fn build_cms_signed_data_with_opts(
    signed_content: &[u8],
    cert_chain_pem: &str,
    private_key_pem: &str,
    opts: &CmsOptions,
) -> Result<Vec<u8>, PdfError> {
    use bcder::encode::Values;
    use bcder::{Mode::Der, OctetString};
    use cryptographic_message_syntax::{Bytes, Oid, SignedDataBuilder, SignerBuilder};
    use sha2::{Digest, Sha256};
    use x509_certificate::rfc5652::AttributeValue;
    use x509_certificate::{CapturedX509Certificate, InMemorySigningKeyPair};

    // ── parse cert chain ──────────────────────────────────────────────────
    let certs = CapturedX509Certificate::from_pem_multiple(cert_chain_pem)
        .map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("cert chain PEM parse error: {e}"),
        })?;
    if certs.is_empty() {
        return Err(PdfError::Parse {
            offset: None,
            context: "cert chain PEM contains no certificates".into(),
        });
    }
    let signer_cert = &certs[0];

    // ── parse private key ─────────────────────────────────────────────────
    let signing_key = InMemorySigningKeyPair::from_pkcs8_pem(private_key_pem)
        .map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("private key PEM parse error: {e}"),
        })?;

    // ── SignerBuilder ─────────────────────────────────────────────────────
    let mut signer = SignerBuilder::new(&signing_key, signer_cert.clone());

    // ── signingCertificateV2 attribute (OID 1.2.840.113549.1.9.16.2.47) ──
    // Identical to rust_pdf_signing `build_signing_certificate_v2_attribute_value`.
    let signing_certificate_v2_oid = Oid(Bytes::copy_from_slice(&[
        42, 134, 72, 134, 247, 13, 1, 9, 16, 2, 47,
    ]));
    let cert_hash = {
        // Use encode_der() to match rust_pdf_signing reference behaviour.
        // The signingCertificateV2 hash MUST be over the canonical DER form
        // of the certificate (same bytes that appear in the CMS certificates field).
        // Fall back to encode_ber() only for certs that cannot be re-encoded as DER.
        let cert_der = signer_cert.encode_der()
            .or_else(|_| signer_cert.encode_ber())
            .map_err(|e| PdfError::Parse {
                offset: None,
                context: format!("signer cert encode error: {e}"),
            })?;
        Sha256::digest(&cert_der).to_vec()
    };
    let signing_certificate_v2_value = {
        let hash_octet = OctetString::new(Bytes::from(cert_hash));
        let ess_cert_id_v2 = bcder::encode::sequence(hash_octet.encode());
        let signing_cert_v2 = bcder::encode::sequence(ess_cert_id_v2);
        let attr_value    = bcder::encode::sequence(signing_cert_v2);
        attr_value.to_captured(Der)
    };
    signer = signer.signed_attribute(
        signing_certificate_v2_oid,
        vec![AttributeValue::new(signing_certificate_v2_value)],
    );

    // ── optional signature timestamp (unsigned attribute via TSA) ─────────
    if let Some(tsa_url) = &opts.timestamp_url {
        signer = signer
            .time_stamp_url(tsa_url)
            .map_err(|e| PdfError::Parse {
                offset: None,
                context: format!("TSA URL error: {e}"),
            })?;
    }

    // ── SignedDataBuilder ─────────────────────────────────────────────────
    let mut builder = SignedDataBuilder::default()
        .content_external(signed_content.to_vec())
        .content_type(Oid(Bytes::copy_from_slice(
            cryptographic_message_syntax::asn1::rfc5652::OID_ID_DATA.as_ref(),
        )))
        .signer(signer);

    for cert in &certs {
        builder = builder.certificate(cert.clone());
    }

    let cms_der = builder.build_der()
        .map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("CMS SignedData build error: {e}"),
        })?;

    Ok(cms_der)
}

// ---------------------------------------------------------------------------
// Public: verify_cms — full CMS signature verification
// ---------------------------------------------------------------------------

/// Result of verifying one CMS signature against given content bytes.
pub struct CmsVerifyResult {
    pub digest_valid:      bool,
    pub signature_valid:   bool,
    pub has_timestamp:     bool,
    pub certificates:      Vec<CmsCertInfo>,
    pub chain_warnings:    Vec<String>,
    pub chain_valid:       bool,
}

pub struct CmsCertInfo {
    pub subject:       String,
    pub issuer:        String,
    pub serial:        String,
    pub not_before:    Option<String>,
    pub not_after:     Option<String>,
    pub is_expired:    bool,
    pub is_self_signed: bool,
}

/// Verify a CMS SignedData blob against the given signed content bytes.
///
/// Uses `cryptographic_message_syntax::SignedData` — same as rust_pdf_signing
/// `SignatureValidator::validate_one`.
pub fn verify_cms(cms_der: &[u8], signed_content: &[u8]) -> CmsVerifyResult {
    use cryptographic_message_syntax::SignedData;
    use x509_certificate::CapturedX509Certificate;

    let mut result = CmsVerifyResult {
        digest_valid:    false,
        signature_valid: false,
        has_timestamp:   false,
        certificates:    vec![],
        chain_warnings:  vec![],
        chain_valid:     false,
    };

    // ── parse SignedData ──────────────────────────────────────────────────
    let signed_data = match SignedData::parse_ber(cms_der) {
        Ok(sd) => sd,
        Err(_) => return result,
    };

    // ── verify each signer ────────────────────────────────────────────────
    for signer_info in signed_data.signers() {
        // verify_message_digest_with_content takes only content (no &signed_data arg)
        match signer_info.verify_message_digest_with_content(signed_content) {
            Ok(()) => result.digest_valid = true,
            Err(_) => {}
        }
        match signer_info.verify_signature_with_signed_data(&signed_data) {
            Ok(()) => result.signature_valid = true,
            Err(_) => {}
        }
        // Check for timestamp unsigned attribute
        // OID 1.2.840.113549.1.9.16.2.14  id-smime-aa-signatureTimeStampToken
        let ts_oid_bytes: &[u8] = &[0x06, 0x0b, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d,
                                     0x01, 0x09, 0x10, 0x02, 0x0e];
        if cms_der.windows(ts_oid_bytes.len()).any(|w| w == ts_oid_bytes) {
            result.has_timestamp = true;
        }
    }

    // ── extract certificates ──────────────────────────────────────────────
    let certs: Vec<CapturedX509Certificate> = signed_data.certificates().cloned().collect();
    let mut chain_ok = true;

    for cert in &certs {
        let subject = cert.subject_common_name()
            .unwrap_or_else(|| cert.subject_name().user_friendly_str()
                .unwrap_or_else(|_| "Unknown".to_string()));
        let issuer = cert.issuer_name().user_friendly_str()
            .unwrap_or_else(|_| "Unknown".to_string());
        // Extract serial from the raw DER: Certificate/TBSCertificate/serialNumber
        let serial = extract_serial_hex(cert);

        let (not_before, not_after, is_expired) =
            extract_validity(cert);
        let is_self_signed = subject == issuer;

        if is_expired {
            chain_ok = false;
            result.chain_warnings.push(format!("Certificate '{subject}' is expired"));
        }
        if is_self_signed {
            result.chain_warnings.push(
                format!("Root CA '{subject}' is self-signed but not a recognized public CA")
            );
        }

        result.certificates.push(CmsCertInfo {
            subject,
            issuer,
            serial,
            not_before,
            not_after,
            is_expired,
            is_self_signed,
        });
    }
    result.chain_valid = chain_ok;
    result
}

fn extract_serial_hex(cert: &x509_certificate::CapturedX509Certificate) -> String {
    // Use encode_ber() — avoids BER/DER mode conflict from encode_der().
    let der = match cert.encode_ber() {
        Ok(d) => d,
        Err(_) => return "?".to_string(),
    };
    if let Some((_, outer)) = parse_tlv(&der) {
        if let Some((_, tbs)) = parse_tlv(outer) {
            let mut pos = 0;
            if tbs.len() > pos && tbs[pos] == 0xa0 {
                if let Some((used, _)) = parse_tlv_at(tbs, pos) { pos += used; }
            }
            if let Some((_, serial_val)) = parse_tlv_at(tbs, pos) {
                return serial_val.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join("");
            }
        }
    }
    "?".to_string()
}

fn extract_validity(cert: &x509_certificate::CapturedX509Certificate)
    -> (Option<String>, Option<String>, bool)
{
    use std::time::SystemTime;
    let now = SystemTime::now();

    // validity_not_before / validity_not_after return DateTime<Utc> directly
    let not_before = Some(format!("{}", cert.validity_not_before()));
    let not_after  = Some(format!("{}", cert.validity_not_after()));

    // is_expired: not_after < now
    let not_after_systime: SystemTime = cert.validity_not_after().into();
    let is_expired = not_after_systime < now;

    (not_before, not_after, is_expired)
}

// ---------------------------------------------------------------------------
// Public: extract_message_digest (for verify_pdf lightweight path)
// ---------------------------------------------------------------------------

/// Extract the SHA-256 `messageDigest` from a CMS blob (minimal DER walk).
/// Returns `(parseable, Option<digest_bytes>)`.
pub fn extract_message_digest(cms_der: &[u8]) -> (bool, Option<Vec<u8>>) {
    let md_oid = der_oid(OID_MESSAGE_DIGEST);
    if let Some(pos) = cms_der.windows(md_oid.len()).position(|w| w == md_oid.as_slice()) {
        let after_oid = &cms_der[pos + md_oid.len()..];
        if let Some((_, set_body)) = parse_tlv(after_oid) {
            if let Some((_, digest)) = parse_tlv(set_body) {
                return (true, Some(digest.to_vec()));
            }
        }
    }
    let parseable = parse_tlv(cms_der).is_some();
    (parseable, None)
}
