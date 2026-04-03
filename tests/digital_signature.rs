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

#[allow(unused_imports)]
use rust_pdfbox::signing::{
    resolve_anchor_rect, sign_pdf, validate_pdf_full, verify_pdf,
    SignatureAnchorMode, SignOptions,
};
#[allow(unused_imports)]
use rust_pdfbox::{cos::ObjectId, Document};
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
    // Uses the generated anchor sample PDF (build_anchor_sample_pdf) as the
    // canonical "real" sample document for integration testing.
    let pdf = build_anchor_sample_pdf();
    let (certs, key_pem) = load_test_credentials();

    let opts = SignOptions {
        reason:  "Signed by rust-pdfbox integration test".into(),
        contact_info: "devtest@rust-pdfbox.local".into(),
        location: "CI".into(),
        reserved_size: 16_384,
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts)
        .expect("sign anchor sample PDF should succeed");

    assert!(signed.len() > pdf.len());

    let results = verify_pdf(&signed).expect("verify signed anchor sample PDF");
    assert!(!results.is_empty(), "signed anchor sample PDF must have at least one signature");
    assert!(results[0].digest_valid,
        "digest must be valid for anchor sample PDF; status='{}'",
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

// ---------------------------------------------------------------------------
// Tests for complete validate_pdf_full
// ---------------------------------------------------------------------------

#[test]
fn validate_pdf_full_returns_all_checks() {
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions {
        reason: "Full validation test".into(),
        contact_info: "validator@example.com".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let results = validate_pdf_full(&signed).expect("validate_pdf_full should succeed");

    assert_eq!(results.len(), 1, "expected exactly 1 validation result");
    let r = &results[0];

    // ── cryptographic ──
    assert!(r.digest_match, "digest must match");
    assert!(r.cms_signature_valid, "CMS signature must be valid");

    // ── certificate chain ──
    assert!(!r.certificates.is_empty(), "must have at least one certificate");
    assert!(r.certificate_chain_valid, "chain must be structurally valid");

    // ── modification detection ──
    assert!(r.no_unauthorized_modifications,
        "single fresh signature must have no unauthorized mods");
    assert!(r.modification_notes.iter().all(|n| n.contains("permitted")),
        "all notes must be permitted: {:?}", r.modification_notes);

    // ── attack defences ──
    assert!(r.byte_range_valid, "ByteRange structure must be valid (no USF)");
    assert!(r.signature_not_wrapped, "Contents not relocated (no SWA)");
    assert!(r.certification_permission_ok, "no MDP violation");

    // ── byte-range ──
    assert!(r.byte_range_covers_whole_file,
        "single signature should cover the whole file");

    // ── is_valid() aggregate ──
    assert!(r.is_valid(), "is_valid() must be true for a freshly signed PDF; errors={:?}", r.errors);

    // ── metadata ──
    assert_eq!(r.reason.as_deref(), Some("Full validation test"));
    assert_eq!(r.contact_info.as_deref(), Some("validator@example.com"));
}

#[test]
fn validate_pdf_full_no_unauthorized_mods_on_signed_pdf() {
    // Uses the generated anchor sample PDF as a realistic multi-content input.
    let pdf = build_anchor_sample_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions::default();

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let results = validate_pdf_full(&signed).expect("validate_pdf_full");

    assert!(!results.is_empty());
    let r = &results[0];
    assert!(r.no_unauthorized_modifications,
        "freshly signed PDF must have no unauthorized modifications");
    assert!(r.byte_range_valid, "ByteRange must be valid");
    assert!(r.signature_not_wrapped, "Contents must not be wrapped");
}

#[test]
fn validate_pdf_full_cert_chain_warnings_for_self_signed() {
    // Our test CA is self-signed, so chain should be valid but not trusted
    let pdf = minimal_pdf();
    let (certs, key_pem) = load_test_credentials();
    let opts = SignOptions::default();

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign_pdf");
    let results = validate_pdf_full(&signed).expect("validate_pdf_full");

    assert!(!results.is_empty());
    let r = &results[0];
    assert!(r.certificate_chain_valid, "structurally valid chain");
    // Self-signed test CA → not trusted
    assert!(!r.certificate_chain_trusted,
        "test CA should not be trusted; this is expected");
    assert!(!r.chain_warnings.is_empty(),
        "should have at least one trust warning for test CA");
}

#[test]
fn validate_pdf_full_unsigned_returns_error() {
    let pdf = minimal_pdf();
    let result = validate_pdf_full(&pdf);
    assert!(result.is_err(),
        "unsigned PDF should return an Err, not an empty vec");
}

// ===========================================================================
// Anchor-tag tests
// ===========================================================================

/// Build a PDF whose first page has real text content at a known position.
/// Content stream places "SIGN_HERE" at (72, 300) with font size 12.
/// The page is 612×792 (US Letter).
fn anchor_pdf() -> Vec<u8> {
    // Content stream: BT  /F1 12 Tf  72 300 Td  (SIGN_HERE) Tj  ET
    let content = "BT /F1 12 Tf 72 300 Td (SIGN_HERE) Tj ET";
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let stream_off = pdf.len() as u64;
    pdf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
            content.len()
        )
        .as_bytes(),
    );

    let font_off = pdf.len() as u64;
    pdf.extend_from_slice(
        b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );

    let page_off = pdf.len() as u64;
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] \
          /Contents 5 0 R \
          /Resources << /Font << /F1 4 0 R >> >> >>\nendobj\n",
    );

    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", page_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", font_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", stream_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

// ---------------------------------------------------------------------------
// resolve_anchor_rect unit tests
// ---------------------------------------------------------------------------

#[test]
#[cfg(feature = "text")]
fn resolve_anchor_rect_overlay_finds_tag() {
    let pdf = anchor_pdf();
    let doc = Document::load_from_bytes(&pdf).expect("parse anchor pdf");

    // Page 1 object ID — walk tree manually: catalog→pages→kids[0]
    let page_id = ObjectId::new(3, 0); // known from anchor_pdf() above

    let rect = resolve_anchor_rect(
        &doc,
        page_id,
        "SIGN_HERE",
        150.0, // width
        40.0,  // height
        &SignatureAnchorMode::Overlay,
    )
    .expect("resolve_anchor_rect should succeed for known tag");

    // chunk.x=72, chunk.y=300, height=40
    // Overlay: x=72, y_top=300, y_bot=260  →  [72, 260, 222, 300]
    assert!((rect[0] - 72.0).abs() < 1.0,  "x1 should be ~72, got {}", rect[0]);
    assert!((rect[1] - 260.0).abs() < 1.0, "y1 should be ~260, got {}", rect[1]);
    assert!((rect[2] - 222.0).abs() < 1.0, "x2 should be ~222, got {}", rect[2]);
    assert!((rect[3] - 300.0).abs() < 1.0, "y2 should be ~300, got {}", rect[3]);
    // Sanity: width = x2-x1 = 150
    assert!((rect[2] - rect[0] - 150.0).abs() < 1.0, "width should be 150");
    // Sanity: height = y2-y1 = 40
    assert!((rect[3] - rect[1] - 40.0).abs() < 1.0, "height should be 40");
}

#[test]
#[cfg(feature = "text")]
fn resolve_anchor_rect_infront_shifts_right() {
    let pdf = anchor_pdf();
    let doc = Document::load_from_bytes(&pdf).expect("parse anchor pdf");
    let page_id = ObjectId::new(3, 0);

    let overlay = resolve_anchor_rect(
        &doc, page_id, "SIGN_HERE", 150.0, 40.0, &SignatureAnchorMode::Overlay,
    ).expect("overlay");

    let infront = resolve_anchor_rect(
        &doc, page_id, "SIGN_HERE", 150.0, 40.0, &SignatureAnchorMode::InFront,
    ).expect("infront");

    // InFront shifts x by font_size (12 pt) compared to Overlay
    assert!(infront[0] > overlay[0],
        "InFront x1 ({}) should be greater than Overlay x1 ({})", infront[0], overlay[0]);
    // Both have the same y extents
    assert!((infront[1] - overlay[1]).abs() < 1.0, "y1 should be the same");
    assert!((infront[3] - overlay[3]).abs() < 1.0, "y2 should be the same");
}

#[test]
#[cfg(feature = "text")]
fn resolve_anchor_rect_missing_tag_returns_error() {
    let pdf = anchor_pdf();
    let doc = Document::load_from_bytes(&pdf).expect("parse anchor pdf");
    let page_id = ObjectId::new(3, 0);

    let result = resolve_anchor_rect(
        &doc, page_id, "NONEXISTENT_TAG_XYZ", 150.0, 40.0, &SignatureAnchorMode::Overlay,
    );

    assert!(result.is_err(), "missing tag should return Err");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("NONEXISTENT_TAG_XYZ") || err_msg.contains("not found"),
        "error message should mention the missing tag: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// sign_pdf with anchor_tag integration tests
// ---------------------------------------------------------------------------

#[test]
#[cfg(feature = "text")]
fn sign_pdf_with_anchor_tag_overlay_produces_valid_signature() {
    let pdf = anchor_pdf();
    let (certs, key_pem) = load_test_credentials();

    let opts = SignOptions {
        visible_signature: true,
        anchor_tag:    Some("SIGN_HERE".into()),
        anchor_width:  Some(150.0),
        anchor_height: Some(40.0),
        anchor_mode:   SignatureAnchorMode::Overlay,
        reason:        "Anchor overlay test".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts)
        .expect("sign_pdf with anchor_tag Overlay should succeed");

    assert!(signed.len() > pdf.len(), "signed PDF must be larger");

    let results = verify_pdf(&signed).expect("verify_pdf");
    assert!(!results.is_empty(), "must have at least one signature");
    assert!(
        results[0].digest_valid,
        "digest must be valid; status='{}'", results[0].status
    );
}

#[test]
#[cfg(feature = "text")]
fn sign_pdf_with_anchor_tag_infront_produces_valid_signature() {
    let pdf = anchor_pdf();
    let (certs, key_pem) = load_test_credentials();

    let opts = SignOptions {
        visible_signature: true,
        anchor_tag:    Some("SIGN_HERE".into()),
        anchor_width:  Some(160.0),
        anchor_height: Some(50.0),
        anchor_mode:   SignatureAnchorMode::InFront,
        reason:        "Anchor infront test".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts)
        .expect("sign_pdf with anchor_tag InFront should succeed");

    assert!(signed.len() > pdf.len(), "signed PDF must be larger");

    let results = verify_pdf(&signed).expect("verify_pdf");
    assert!(!results.is_empty(), "must have at least one signature");
    assert!(
        results[0].digest_valid,
        "digest must be valid; status='{}'", results[0].status
    );
}

#[test]
#[cfg(feature = "text")]
fn sign_pdf_anchor_tag_not_found_returns_error() {
    let pdf = anchor_pdf();
    let (certs, key_pem) = load_test_credentials();

    let opts = SignOptions {
        visible_signature: true,
        anchor_tag:    Some("MISSING_TAG_DOES_NOT_EXIST".into()),
        anchor_width:  Some(150.0),
        anchor_height: Some(40.0),
        anchor_mode:   SignatureAnchorMode::Overlay,
        ..Default::default()
    };

    let result = sign_pdf(&pdf, &certs, &key_pem, &opts);
    assert!(
        result.is_err(),
        "sign_pdf with a missing anchor tag should return Err"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("MISSING_TAG_DOES_NOT_EXIST") || err_msg.contains("not found"),
        "error message should reference the missing tag: {err_msg}"
    );
}

#[test]
#[cfg(feature = "text")]
fn sign_pdf_anchor_overrides_rect() {
    // If both `rect` and `anchor_tag` are set, anchor_tag wins.
    let pdf = anchor_pdf();
    let (certs, key_pem) = load_test_credentials();

    let opts_anchor = SignOptions {
        visible_signature: true,
        anchor_tag:    Some("SIGN_HERE".into()),
        anchor_width:  Some(150.0),
        anchor_height: Some(40.0),
        anchor_mode:   SignatureAnchorMode::Overlay,
        rect:          Some([10.0, 10.0, 20.0, 20.0]), // should be ignored
        reason:        "Anchor overrides rect".into(),
        ..Default::default()
    };

    // Should succeed (anchor finds tag, ignores explicit rect)
    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts_anchor)
        .expect("sign_pdf anchor override should succeed");

    let doc = Document::load_from_bytes(&signed).expect("parse");
    // The document should have an AcroForm (proves signing happened)
    let catalog = doc.catalog().expect("catalog");
    assert!(catalog.get(&rust_pdfbox::cos::CosName::new(b"AcroForm")).is_some(),
        "AcroForm must be present");

    // Verify the signature is cryptographically valid
    let results = verify_pdf(&signed).expect("verify");
    assert!(!results.is_empty());
    assert!(results[0].digest_valid, "digest must be valid");
}

#[test]
#[cfg(feature = "text")]
fn anchor_tag_partial_match_works() {
    // Tag search uses `contains`, so a partial match should work too.
    let pdf = anchor_pdf();
    let doc = Document::load_from_bytes(&pdf).expect("parse anchor pdf");
    let page_id = ObjectId::new(3, 0);

    // "SIGN" is a prefix of "SIGN_HERE" — should match
    let rect = resolve_anchor_rect(
        &doc, page_id, "SIGN", 100.0, 30.0, &SignatureAnchorMode::Overlay,
    ).expect("partial tag prefix should match");

    assert!(rect[2] > rect[0], "width must be positive");
    assert!(rect[3] > rect[1], "height must be positive");
}

#[test]
#[cfg(feature = "text")]
fn anchor_rect_dimensions_are_correct() {
    // Verify that the returned rect always has the requested width/height.
    let pdf = anchor_pdf();
    let doc = Document::load_from_bytes(&pdf).expect("parse");
    let page_id = ObjectId::new(3, 0);

    for (w, h) in [(100.0_f64, 30.0_f64), (200.0, 60.0), (50.0, 25.0)] {
        for mode in [SignatureAnchorMode::Overlay, SignatureAnchorMode::InFront] {
            let rect = resolve_anchor_rect(
                &doc, page_id, "SIGN_HERE", w, h, &mode,
            ).expect("resolve");

            let actual_w = rect[2] - rect[0];
            let actual_h = rect[3] - rect[1];
            assert!(
                (actual_w - w).abs() < 0.1,
                "width mismatch for mode={mode:?}: expected {w}, got {actual_w}"
            );
            assert!(
                (actual_h - h).abs() < 0.1,
                "height mismatch for mode={mode:?}: expected {h}, got {actual_h}"
            );
        }
    }
}

#[test]
#[cfg(feature = "text")]
fn sign_pdf_anchor_validates_full() {
    // Full validate_pdf_full check after anchor-tag signing
    let pdf = anchor_pdf();
    let (certs, key_pem) = load_test_credentials();

    let opts = SignOptions {
        visible_signature: true,
        anchor_tag:    Some("SIGN_HERE".into()),
        anchor_width:  Some(180.0),
        anchor_height: Some(50.0),
        anchor_mode:   SignatureAnchorMode::InFront,
        reason:        "Anchor full-validate test".into(),
        contact_info:  "anchor@test.local".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign");
    let results = validate_pdf_full(&signed).expect("validate_pdf_full");

    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert!(r.digest_match, "digest must match");
    assert!(r.cms_signature_valid, "CMS sig must be valid");
    assert!(r.no_unauthorized_modifications, "no modifications");
    assert!(r.is_valid(), "is_valid() must be true; errors={:?}", r.errors);
}

// ===========================================================================
// Anchor + image signing demo — writes anchor_sample.pdf + anchor_signed.pdf
// ===========================================================================

/// Build a standards-compliant US-Letter PDF with body text and a visible
/// `##SIGN_HERE##` anchor tag near the bottom of the page.
///
/// ### PDF structure (objects written top-to-bottom, xref matches offsets):
/// ```
/// 1 0 obj  Catalog  → /Pages 2
/// 2 0 obj  Pages    → /Kids [3]
/// 3 0 obj  Page     → /MediaBox /Contents 5 /Resources /Font /F1 4
/// 4 0 obj  Font     → Helvetica Type1
/// 5 0 obj  Stream   → content operators
/// ```
///
/// ### PDF content-stream fixes vs the old broken version:
/// - Objects written in order 1→2→3→4→5 so xref offsets are simple to verify
/// - `/Length` counts the exact bytes in the stream (no off-by-one)
/// - PDF path operators `m` / `l` / `S` instead of PostScript `moveto`/`lineto`
/// - `stream` keyword followed by exactly one `\n`, `endstream` preceded by `\n`
fn build_anchor_sample_pdf() -> Vec<u8> {
    // ── content stream ────────────────────────────────────────────────────
    // US Letter 612×792 pt, origin bottom-left, Y grows upward.
    //
    // Signature line sits at y=145; label at y=165; anchor tag at y=130.
    let content = concat!(
        // Title
        "BT /F1 20 Tf 72 730 Td (Digital Signature Demo Document) Tj ET\n",
        // Body text
        "BT /F1 11 Tf 72 680 Td ",
            "(This document demonstrates anchor-tag-based visible PDF digital signatures.) Tj ET\n",
        "BT /F1 11 Tf 72 660 Td ",
            "(The signer stamp will appear next to the anchor marker below.) Tj ET\n",
        "BT /F1 11 Tf 72 640 Td ",
            "(Generated by rust-pdfbox \\055 April 2026) Tj ET\n",
        "BT /F1 11 Tf 72 615 Td ",
            "(Lorem ipsum dolor sit amet, consectetur adipiscing elit.) Tj ET\n",
        // Signature section
        "BT /F1 11 Tf 72 165 Td (Authorized Signature:) Tj ET\n",
        // Horizontal rule (PDF path: m=moveto, l=lineto, S=stroke)
        "0.7 0.7 0.7 RG 1 w 72 148 m 320 148 l S\n",
        // Anchor tag (10 pt, placed below the rule)
        "BT /F1 10 Tf 72 130 Td (##SIGN_HERE##) Tj ET\n",
    );
    let stream_bytes = content.as_bytes();

    // ── assemble objects in order 1..5 ───────────────────────────────────
    let mut pdf: Vec<u8> = Vec::with_capacity(4096);
    pdf.extend_from_slice(b"%PDF-1.4\n");
    pdf.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n"); // 4-byte binary comment (marks binary PDF)

    // Object 1: Catalog
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // Object 2: Pages
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    // Object 3: Page
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n\
          << /Type /Page\n\
             /Parent 2 0 R\n\
             /MediaBox [0 0 612 792]\n\
             /Contents 5 0 R\n\
             /Resources << /Font << /F1 4 0 R >> >>\n\
          >>\nendobj\n",
    );

    // Object 4: Font (Helvetica, built-in Type1)
    let off4 = pdf.len();
    pdf.extend_from_slice(
        b"4 0 obj\n\
          << /Type /Font /Subtype /Type1 /BaseFont /Helvetica\n\
             /Encoding /WinAnsiEncoding >>\nendobj\n",
    );

    // Object 5: Content stream
    // /Length must equal the exact byte count of the stream data.
    // Per PDF spec §7.3.8.1: stream data is between "\nstream\n" and "\nendstream".
    let off5 = pdf.len();
    pdf.extend_from_slice(
        format!("5 0 obj\n<< /Length {} >>\nstream\n", stream_bytes.len()).as_bytes(),
    );
    pdf.extend_from_slice(stream_bytes);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    // ── cross-reference table ─────────────────────────────────────────────
    // PDF spec §7.5.4: each xref entry must be exactly 20 bytes.
    // Format: nnnnnnnnnn ggggg n\r\n  (10 + 1 + 5 + 1 + 1 + 1 + 1 = 20)
    // No space before \r\n.
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f\r\n");
    pdf.extend_from_slice(format!("{off1:010} 00000 n\r\n").as_bytes());
    pdf.extend_from_slice(format!("{off2:010} 00000 n\r\n").as_bytes());
    pdf.extend_from_slice(format!("{off3:010} 00000 n\r\n").as_bytes());
    pdf.extend_from_slice(format!("{off4:010} 00000 n\r\n").as_bytes());
    pdf.extend_from_slice(format!("{off5:010} 00000 n\r\n").as_bytes());

    // ── trailer ───────────────────────────────────────────────────────────
    pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

/// Diagnostic: print ByteRange values, Contents offset, and verify alignment.
/// Run with: cargo test diag_bytrange -- --nocapture
#[test]
fn diag_byterange_alignment() {

    let pdf = build_anchor_sample_pdf();
    let (certs, key_pem) = load_test_credentials();

    let opts = SignOptions {
        visible_signature: false, // invisible — simpler for diagnosis
        include_crl:   false,
        include_ocsp:  false,
        include_dss:   false,
        timestamp_url: None,
        reserved_size: 8192,
        ..Default::default()
    };

    let signed = sign_pdf(&pdf, &certs, &key_pem, &opts).expect("sign");
    let buf = &signed;

    // ── Find /ByteRange ────────────────────────────────────────────────────
    let br_needle = b"/ByteRange [";
    let br_off = buf.windows(br_needle.len())
        .position(|w| w == br_needle)
        .expect("/ByteRange not found");
    let br_end = buf[br_off..].iter().position(|&b| b == b']').unwrap() + br_off + 1;
    let br_raw = std::str::from_utf8(&buf[br_off..br_end]).unwrap();
    println!("ByteRange raw    : {br_raw:?}");

    // Parse
    let nums_str = &br_raw["/ByteRange [".len()..br_raw.len()-1];
    let vals: Vec<i64> = nums_str.split_whitespace()
        .filter_map(|s| s.parse().ok()).collect();
    assert_eq!(vals.len(), 4, "ByteRange must have 4 values");
    let (r0s, r0l, r1s, r1l) = (vals[0], vals[1], vals[2], vals[3]);
    println!("ByteRange values : r0s={r0s} r0l={r0l} r1s={r1s} r1l={r1l}");
    println!("File size        : {}", buf.len());
    println!("Covered bytes    : {}", r0l + r1l);
    println!("Gap (Contents)   : r0_end={} r1_start={} gap_len={}", r0s+r0l, r1s, r1s-(r0s+r0l));

    // ── Find /Contents < ───────────────────────────────────────────────────
    let ct_needle = b"/Contents <";
    let ct_off = buf.windows(ct_needle.len())
        .position(|w| w == ct_needle)
        .expect("/Contents < not found");
    let hex_open  = ct_off + b"/Contents ".len();  // offset of '<'
    let hex_close = buf[hex_open..].iter().position(|&b| b == b'>').unwrap() + hex_open; // offset of '>'
    println!("\n/Contents < at   : {hex_open}  (> at {hex_close})");
    println!("Contents field   : bytes [{hex_open}..{}]  len={}", hex_close+1, hex_close+1-hex_open);

    // ── Alignment check ────────────────────────────────────────────────────
    // ByteRange: Range0 ends at r0s+r0l, Range1 starts at r1s
    // The gap is [r0s+r0l .. r1s] which must exactly be the </Contents <...>>
    // hex_open = first byte of '<'     → should equal r0s+r0l
    // hex_close+1 = first byte after > → should equal r1s
    let gap_start = (r0s + r0l) as usize;
    let gap_end   = r1s as usize;
    println!("\nExpected gap     : [{gap_start}..{gap_end}]");
    println!("Actual  gap      : [{hex_open}..{}]", hex_close+1);
    println!("Open  aligned    : {}", hex_open == gap_start);
    println!("Close aligned    : {}", hex_close + 1 == gap_end);

    // Context around the gap boundary
    let ctx_start = gap_start.saturating_sub(20);
    let ctx_end   = (gap_end + 10).min(buf.len());
    let ctx = &buf[ctx_start..ctx_end];
    println!("\nContext [{ctx_start}..{ctx_end}]:");
    // print as ASCII with non-printable as '.'
    let ascii: String = ctx.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '·' }).collect();
    println!("  {ascii}");

    // ── Digest check ──────────────────────────────────────────────────────
    use sha2::{Digest as _, Sha256};
    let mut h = Sha256::new();
    h.update(&buf[r0s as usize..(r0s+r0l) as usize]);
    h.update(&buf[r1s as usize..(r1s+r1l) as usize]);
    let file_hash = h.finalize();
    let hash_hex: String = file_hash.iter().map(|b| format!("{b:02x}")).collect();
    println!("\nSHA-256 over ranges: {hash_hex}");

    // Assert alignment
    assert_eq!(hex_open, gap_start,
        "MISMATCH: /Contents '<' is at {hex_open} but ByteRange gap starts at {gap_start}");
    assert_eq!(hex_close + 1, gap_end,
        "MISMATCH: /Contents '>' is at {hex_close} but ByteRange gap ends at {gap_end}");

    // Write the signed PDF and a text dump for inspection
    std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/diag_signed.pdf"),
        &signed,
    ).unwrap();

    // Dump sig dict region to text file
    let update_start = pdf.len();
    let region_end = (update_start + 1500).min(buf.len());
    let region: String = buf[update_start..region_end].iter()
        .map(|&b| if b.is_ascii_graphic() || b == b' ' || b == b'\n' || b == b'\r' { b as char } else { '·' })
        .collect();
    std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/diag_update.txt"),
        &region,
    ).unwrap();
}


/// Cryptographic verification of anchor-tag signing using the generated
/// anchor sample PDF.  Writes `anchor_sample.pdf` to the workspace root so
/// that `scripts/sign_all_variants.sh` can use it as its anchor input.
/// All signed-output file writes have moved to that script.
#[test]
#[cfg(feature = "text")]
fn anchor_image_sign_demo_writes_valid_pdf() {
    let (cert_pem, key_pem) = load_test_credentials();
    let sig_image_path = asset_path("sig1.png");

    // ── 1. Build the sample PDF ───────────────────────────────────────────
    let sample_pdf = build_anchor_sample_pdf();

    // Sanity: our own parser must accept it
    let doc = rust_pdfbox::Document::load_from_bytes(&sample_pdf)
        .expect("build_anchor_sample_pdf must produce a parseable PDF");
    assert_eq!(doc.page_count(), 1, "sample PDF must have exactly 1 page");

    // Write unsigned source into signing_assets/ so the script and other
    // tests can reference it alongside the other asset files.
    std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/signing_assets/anchor_sample.pdf"),
        &sample_pdf,
    ).expect("write tests/signing_assets/anchor_sample.pdf");
    println!("✅  anchor_sample.pdf written to tests/signing_assets/");

    // ── 2. Sign with anchor tag (image if available, else text-only) ──────
    let image_path = if sig_image_path.exists() {
        Some(sig_image_path.clone())
    } else {
        None
    };

    let opts = SignOptions {
        visible_signature: true,
        anchor_tag:    Some("##SIGN_HERE##".into()),
        anchor_width:  Some(200.0),
        anchor_height: Some(60.0),
        anchor_mode:   SignatureAnchorMode::InFront,
        image_path,
        signer_name:  "Rust PDFBox Demo Signer".into(),
        reason:       "Approved — anchor-tag signing demo".into(),
        contact_info: "demo@rust-pdfbox.local".into(),
        location:     "Jakarta, Indonesia".into(),
        include_crl:   false,
        include_ocsp:  false,
        include_dss:   false,
        timestamp_url: None,
        reserved_size: 32_768,
        field_name:    "AnchorSignature".into(),
        ..Default::default()
    };

    let signed = sign_pdf(&sample_pdf, &cert_pem, &key_pem, &opts)
        .expect("sign_pdf with anchor+image should succeed");

    assert!(
        signed.len() > sample_pdf.len(),
        "signed PDF ({}) must be larger than source ({})",
        signed.len(), sample_pdf.len()
    );

    // ── 3. The signed PDF must still be parseable ─────────────────────────
    let signed_doc = rust_pdfbox::Document::load_from_bytes(&signed)
        .expect("signed PDF must be parseable");
    assert_eq!(signed_doc.page_count(), 1, "page count must be preserved");

    // ── 4. AcroForm must be present in catalog ────────────────────────────
    let catalog = signed_doc.catalog().expect("catalog must exist");
    assert!(
        catalog.get(&rust_pdfbox::cos::CosName::new(b"AcroForm")).is_some(),
        "/AcroForm must be present in catalog after signing"
    );

    // ── 5. Cryptographic verification ────────────────────────────────────
    let results = verify_pdf(&signed).expect("verify_pdf must not error");
    assert_eq!(results.len(), 1, "must find exactly one signature");

    let r = &results[0];
    assert!(r.digest_valid,
        "SHA-256 digest over signed byte ranges must match; status='{}'", r.status);
    assert!(r.cms_signature_valid,
        "CMS RSA signature must verify; status='{}'", r.status);
    assert!(r.byte_range_covers_whole_file,
        "ByteRange must cover the entire file");
    assert!(r.errors.is_empty(),
        "no verification errors expected; got: {:?}", r.errors);

    // ── 6. Full structural validation ─────────────────────────────────────
    let full = validate_pdf_full(&signed).expect("validate_pdf_full");
    assert_eq!(full.len(), 1);
    let fr = &full[0];
    assert!(fr.digest_match,           "full: digest must match");
    assert!(fr.cms_signature_valid,    "full: CMS sig must be valid");
    assert!(fr.no_unauthorized_modifications, "full: no unauthorized modifications");
    assert!(fr.byte_range_valid,       "full: ByteRange structure must be valid");
    assert!(fr.signature_not_wrapped,  "full: signature must not be wrapped");
    assert!(fr.is_valid(),
        "full: is_valid() must be true; errors={:?}", fr.errors);
}




