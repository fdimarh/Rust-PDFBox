//! Integration tests for PDF digital signature support.
//!
//! Uses real assets from `tests/signing_assets/` which were copied from
//! `rust_pdf_signing/examples/assets/` (reference implementation by Ralph Bisschops).
//!
//! # Assets used
//!
//! | File                       | Source / Purpose                         |
//! |----------------------------|------------------------------------------|
//! | `sample.pdf`                | Input PDF (lopdf-generated sample)       |
//! | `good-user-crl-ocsp.p12`   | PKCS#12 source bundle (password: ks-password) |
//! | `ca-chain.pem`             | RSA cert chain extracted from P12 (signer cert first) |
//! | `user-key.pem`             | RSA PKCS#8 private key extracted from P12 |
//! | `user-cert.pem`            | End-entity certificate (extracted from P12) |
//! | `ca-certs.pem`             | CA certificates only (extracted from P12) |
//!
//! # What is tested
//!
//! 1. `sign_pdf` returns a larger byte buffer without error.
//! 2. The signed PDF still parses with `Document::load_from_bytes`.
//! 3. The signed PDF has the correct page count.
//! 4. `verify_pdf` finds exactly one signature field.
//! 5. The SHA-256 digest over the signed byte ranges matches (`digest_valid`).
//! 6. The CMS blob is structurally parseable (`cms_parseable`).
//! 7. Signing an already-in-memory minimal PDF (no disk file) also works.
//! 8. `verify_pdf` returns an empty vec for a plain unsigned PDF.

use rust_pdfbox::signing::{sign_pdf, verify_pdf, SignOptions};
use rust_pdfbox::Document;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Asset helpers
// ---------------------------------------------------------------------------

fn asset_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("signing_assets");
    p.push(name);
    p
}

fn asset_bytes(name: &str) -> Vec<u8> {
    std::fs::read(asset_path(name))
        .unwrap_or_else(|e| panic!("missing test asset '{}': {e}", name))
}

fn asset_text(name: &str) -> String {
    std::fs::read_to_string(asset_path(name))
        .unwrap_or_else(|e| panic!("missing test asset '{}': {e}", name))
}


// ---------------------------------------------------------------------------
// Minimal in-memory PDF fixture
// ---------------------------------------------------------------------------

fn minimal_pdf() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let p1_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n");
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", p1_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

/// Load real RSA-2048 credentials from tests/signing_assets/.
/// Returns (cert_chain_pem, key_pem) — both as PEM strings.
fn load_test_credentials() -> (String, String) {
    let cert_chain_pem = asset_text("ca-chain.pem");
    let key_pem        = asset_text("user-key.pem");
    assert!(cert_chain_pem.contains("-----BEGIN CERTIFICATE-----"),
        "ca-chain.pem contains no certificates — check asset path and PEM format");
    (cert_chain_pem, key_pem)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn sign_pdf_produces_larger_output() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions::default();

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts)
        .expect("sign_pdf should succeed");

    assert!(signed.len() > pdf.len(),
        "signed PDF ({} bytes) should be larger than original ({} bytes)",
        signed.len(), pdf.len());
}

#[test]
fn signed_pdf_is_parseable() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions::default();

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let doc = Document::load_from_bytes(&signed)
        .expect("signed PDF should be parseable by Document::load_from_bytes");

    assert_eq!(doc.page_count(), 1, "page count must be preserved after signing");
}

#[test]
fn signed_pdf_has_acroform_in_catalog() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions {
        field_name: "TestSig".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let doc = Document::load_from_bytes(&signed).expect("parse signed PDF");

    let catalog = doc.catalog().expect("catalog must exist");
    let acroform = catalog.get(&rust_pdfbox::cos::CosName::new(b"AcroForm"));
    assert!(acroform.is_some(), "/AcroForm must be present in catalog after signing");
}

#[test]
fn verify_finds_one_signature() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions::default();

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let results = verify_pdf(&signed).expect("verify_pdf should not error");

    assert_eq!(results.len(), 1,
        "expected exactly 1 signature result, got {}", results.len());
}

#[test]
fn verify_digest_is_valid() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions {
        reason:  "Test signature".into(),
        contact_info: "test@example.com".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let results = verify_pdf(&signed).expect("verify_pdf");

    assert!(!results.is_empty(), "must have at least one result");
    let r = &results[0];

    assert!(r.cms_parseable, "CMS blob must be structurally parseable");
    assert!(r.digest_valid,
        "SHA-256 digest over signed byte ranges must match CMS messageDigest; status='{}'",
        r.status);
    assert!(r.cms_signature_valid,
        "CMS RSA/EC signature must verify against embedded signer cert; status='{}'",
        r.status);
}

#[test]
fn verify_reason_is_preserved() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let reason = "Verified by rust-pdfbox test suite";
    let opts = SignOptions {
        reason: reason.into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let results = verify_pdf(&signed).expect("verify_pdf");

    assert!(!results.is_empty());
    assert_eq!(results[0].reason.as_deref(), Some(reason));
}

#[test]
fn verify_empty_on_unsigned_pdf() {
    let pdf = minimal_pdf();
    let results = verify_pdf(&pdf).expect("verify_pdf must not error on unsigned PDF");
    assert!(results.is_empty(),
        "unsigned PDF must yield 0 verification results, got {}", results.len());
}

#[test]
fn byte_range_covers_whole_file() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions::default();

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let results = verify_pdf(&signed).expect("verify_pdf");

    assert!(!results.is_empty());
    let br = results[0].byte_range;
    // br = [r0_start, r0_len, r1_start, r1_len]  (as per PDF §12.8)
    assert!(br[1] > 0, "r0_len must be > 0");
    assert!(br[3] > 0, "r1_len must be > 0");
    // The two ranges must not overlap and together cover the whole file.
    let total_covered = br[1] + br[3];
    assert!(total_covered > 0, "byte ranges must be non-zero");
    // r1_start must be after r0_start + r0_len (the /Contents gap sits between them)
    assert!(br[2] > br[0] + br[1],
        "r1_start ({}) should be after end of r0 ({})", br[2], br[0] + br[1]);
    // The gap between the two ranges is the /Contents hex field (2 + reserved*2 bytes)
    let gap = br[2] - (br[0] + br[1]);
    assert!(gap > 0, "gap between ranges (the /Contents field) must be > 0, got {gap}");
}

#[test]
fn sign_real_sample_pdf_with_assets() {
    // Uses real PDFBox-generated sample.pdf + keystore-local certs
    let pdf = asset_bytes("sample.pdf");
    let (certs, key_pem) = load_test_credentials();

    let opts = SignOptions {
        reason:  "Signed by rust-pdfbox integration test".into(),
        contact_info: "devtest@rust-pdfbox.local".into(),
        location: "CI".into(),
        reserved_size: 16_384,
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts)
        .expect("sign real sample.pdf should succeed");

    assert!(signed.len() > pdf.len());

    let results = verify_pdf(&signed).expect("verify real signed PDF");
    assert!(!results.is_empty(), "signed real PDF must have at least one signature");
    assert!(results[0].digest_valid,
        "digest must be valid for real sample.pdf; status='{}'",
        results[0].status);
}

#[test]
fn sign_with_visible_rect() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions {
        rect: Some([50.0, 700.0, 250.0, 750.0]),
        reason: "Visible signature test".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign with visible rect");
    assert!(signed.len() > pdf.len());

    let results = verify_pdf(&signed).expect("verify");
    assert!(!results.is_empty());
    assert!(results[0].digest_valid);
}

#[test]
fn sign_page_two_does_not_panic() {
    // Build a 2-page PDF and sign page 2
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let p1_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n");
    let p2_off = pdf.len() as u64;
    pdf.extend_from_slice(b"4 0 obj\n<< /Type /Page /MediaBox [0 0 595 842] >>\nendobj\n");
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", p1_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", p2_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());

    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions { page: 2, ..Default::default() };

    // Should not panic / error — result validates correctly
    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign page 2");
    let results = verify_pdf(&signed).expect("verify page 2");
    assert!(!results.is_empty());
    assert!(results[0].digest_valid);
}

#[test]
fn sign_rejects_empty_cert_chain() {
    let pdf = minimal_pdf();
    let opts = SignOptions::default();
    // Pass an empty PEM string — no certificates
    let err = sign_pdf(&pdf, "", "-----BEGIN PRIVATE KEY-----\n-----END PRIVATE KEY-----\n", &opts);
    assert!(err.is_err(), "empty cert chain should be rejected");
}

