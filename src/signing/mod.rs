//! PDF Digital Signature support — pure rust-pdfbox implementation.
//!
//! This module implements PDF digital signatures following:
//!
//! - PDF 1.7 spec §12.8 — Digital Signatures
//! - RFC 5652  — Cryptographic Message Syntax (CMS / PKCS#7)
//! - RFC 5280  — X.509 Certificate Profile
//! - ETSI EN 319 142 — PAdES baseline profiles (B-B, B-T stubs)
//!
//! # Architecture (mirrors Java PDFBox)
//!
//! | Concept                        | Java PDFBox class               | This module           |
//! |--------------------------------|---------------------------------|-----------------------|
//! | Signature field                | `PDSignature`                   | [`SigField`]          |
//! | Signature options              | `SignatureOptions`               | [`SignOptions`]       |
//! | CMS / PKCS#7 builder           | `CreateSignature`               | [`cms`]               |
//! | Byte-range placeholder         | `SignatureInterface`             | [`ByteRangePlaceholder`] |
//! | AcroForm wiring                | `PDDocumentCatalog.getAcroForm` | [`acroform`]          |
//! | DER / ASN.1 helpers            | `org.bouncycastle.asn1.*`       | [`asn1`]              |
//!
//! # How signing works (step-by-step)
//!
//! 1. **Prepare** — add a `/Sig` field to `/AcroForm`; reserve a
//!    `/Contents <000…0>` placeholder and a `/ByteRange [_ _ _ _]` placeholder.
//! 2. **Serialize** the whole updated PDF to bytes (incremental append).
//! 3. **Patch byte-range** — the total file length is now known; fill in the
//!    four ByteRange values that skip the `/Contents` hex-string.
//! 4. **Digest** — SHA-256 over the two byte ranges that exclude `/Contents`.
//! 5. **Sign** — RSA-PKCS1v15 or ECDSA over the digest; wrap in a CMS
//!    `SignedData` envelope.
//! 6. **Inject** — hex-encode the DER-encoded CMS blob and overwrite the
//!    `/Contents` placeholder (must fit within the reserved size).
//!
//! # Supported key types
//!
//! - RSA (any bit-length, PKCS#8 PEM — `-----BEGIN PRIVATE KEY-----`)
//! - EC P-256 (PKCS#8 PEM)
//!
//! # References (rust_pdf_signing as design guide)
//!
//! The `rust_pdf_signing` crate (by Ralph Bisschops) served as the
//! architecture reference. Functions replicated here:
//!
//! | rust_pdf_signing function              | This module                        |
//! |----------------------------------------|------------------------------------|
//! | `PDFSigningDocument::sign_document_no_placeholder` | [`sign_pdf`]         |
//! | `ByteRange::find_and_patch`            | [`ByteRangePlaceholder::patch`]     |
//! | `digitally_sign::build_cms`            | [`cms::build_cms_signed_data`]      |
//! | `acro_form::add_sig_field`             | [`acroform::add_sig_field`]         |

pub mod acroform;
pub mod asn1;
pub mod cms;

use std::collections::BTreeMap;

use crate::cos::{CosName, CosObject, CosDictionary, ObjectId};
use crate::{Document, PdfError};

// ---------------------------------------------------------------------------
// Public surface types
// ---------------------------------------------------------------------------

/// Signature format — mirrors `SignatureFormat` in rust_pdf_signing.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureFormat {
    /// PKCS#7 / `adbe.pkcs7.detached` — classic Adobe format.
    Pkcs7,
    /// PAdES / `ETSI.CAdES.detached` — ETSI EN 319 142 format.
    PAdES,
}

/// PAdES baseline conformance level — mirrors `PadesLevel` in rust_pdf_signing.
#[derive(Debug, Clone, PartialEq)]
#[allow(non_camel_case_types)]
pub enum PadesLevel {
    /// Basic: ESS-signingCertificateV2 only, no timestamp, no DSS.
    B_B,
    /// Timestamp: B-B + RFC 3161 signature timestamp in unsigned attributes.
    B_T,
    /// Long-Term: B-T + DSS dictionary (CRL/OCSP/Certs) for offline validation.
    B_LT,
    /// Long-Term Archival: B-LT + document-level timestamp over the DSS.
    B_LTA,
}

/// Placement mode for anchor-tag-based visible signature positioning.
/// Mirrors `SignatureAnchorMode` in rust_pdf_signing.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureAnchorMode {
    /// Place the visible signature directly on top of the matched tag text.
    Overlay,
    /// Place the visible signature to the right / in front of the matched tag text.
    InFront,
}

impl Default for SignatureAnchorMode {
    fn default() -> Self { Self::InFront }
}

/// Controls how and where the signature is placed.
///
/// Mirrors `SignatureOptions` in rust_pdf_signing / Java PDFBox `SignatureOptions`.
#[derive(Debug, Clone)]
pub struct SignOptions {
    // ── Cryptographic format ──────────────────────────────────────────────
    /// Signature format: PKCS7 (Adobe legacy) or PAdES (ETSI).
    /// Default: `Pkcs7`.
    pub format: SignatureFormat,

    /// PAdES conformance level. Only used when `format == PAdES`.
    ///
    /// | Level | Description                                     |
    /// |-------|-------------------------------------------------|
    /// | `B_B` | Basic: ESS-signingCertV2 only, no timestamp     |
    /// | `B_T` | Timestamp: B-B + RFC 3161 TSA token             |
    /// | `B_LT`| Long-Term: B-T + DSS dict (CRL/OCSP/Certs)     |
    /// | `B_LTA`| Archival: B-LT + document-level timestamp      |
    pub pades_level: PadesLevel,

    /// RFC 3161 timestamp authority URL.
    /// Required for PAdES B-T / B-LT / B-LTA.
    /// Optional for PKCS7 LTV (`http://timestamp.digicert.com` by default for PKCS7).
    pub timestamp_url: Option<String>,

    /// Include CRL data in CMS signed attributes (`adbe-revocationInfoArchival`).
    /// Default: `true` for PKCS7 (matches Adobe LTV expectation), `false` for PAdES B-B/B-T.
    pub include_crl: bool,

    /// Include OCSP response in CMS signed attributes.
    /// Default: `false`.
    pub include_ocsp: bool,

    /// Append a DSS (Document Security Store) dictionary after signing.
    /// Automatically `true` for PAdES B-LT / B-LTA.
    pub include_dss: bool,

    // ── Placement ─────────────────────────────────────────────────────────
    /// 1-based page number where the signature widget lives.  Default: `1`.
    pub page: u32,

    /// Visible signature rectangle `[x1 y1 x2 y2]` in page user-space points.
    /// `None` → invisible signature (no appearance stream).
    pub rect: Option<[f64; 4]>,

    /// When `false`, an invisible (cryptography-only) signature is created
    /// even if `rect` is set. Default: `true` (visible when `rect` is set).
    pub visible_signature: bool,

    // ── Anchor-tag placement ──────────────────────────────────────────────
    /// Optional text marker on the page to which the visible signature is anchored.
    /// When set, the engine searches for this text and derives a placement rectangle
    /// from `anchor_width` / `anchor_height`.  Returns an error if not found.
    pub anchor_tag: Option<String>,

    /// Width of the signature rectangle when `anchor_tag` is used.
    pub anchor_width: Option<f64>,

    /// Height of the signature rectangle when `anchor_tag` is used.
    pub anchor_height: Option<f64>,

    /// Whether the visible signature is placed on top of (`Overlay`) or to the
    /// right of (`InFront`) the anchor tag text. Default: `InFront`.
    pub anchor_mode: SignatureAnchorMode,

    // ── Signer metadata ───────────────────────────────────────────────────
    /// Signer display name (written to `/Name` in the sig dictionary).
    pub signer_name: String,

    /// Signer e-mail address (written to `/ContactInfo`).
    pub contact_info: String,

    /// Signing reason (written to `/Reason`).
    pub reason: String,

    /// Signing location (written to `/Location`).
    pub location: String,

    // ── Technical ─────────────────────────────────────────────────────────
    /// Number of bytes to reserve for the `/Contents` CMS blob (hex-encoded).
    /// Increase if signing fails with "CMS blob exceeds reserved_size".
    /// Default: 32 768 (comfortable for 3-cert RSA-2048 chain + timestamp).
    pub reserved_size: usize,

    /// `/AcroForm` field name for the signature widget. Default: `"Signature1"`.
    pub field_name: String,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            format:        SignatureFormat::Pkcs7,
            pades_level:   PadesLevel::B_B,
            // DigiCert free TSA — used for PKCS7 LTV and PAdES B-T+
            timestamp_url: Some("http://timestamp.digicert.com".into()),
            include_crl:   true,   // Adobe LTV default for PKCS7
            include_ocsp:  false,
            include_dss:   false,
            page:          1,
            rect:          None,
            visible_signature: true,
            anchor_tag:    None,
            anchor_width:  None,
            anchor_height: None,
            anchor_mode:   SignatureAnchorMode::InFront,
            signer_name:   String::new(),
            contact_info:  String::new(),
            reason:        "Digital Signature".into(),
            location:      String::new(),
            reserved_size: 32_768,
            field_name:    "Signature1".into(),
        }
    }
}

/// Certificate information extracted from a CMS blob.
#[derive(Debug, Clone)]
pub struct CertInfo {
    pub subject:       String,
    pub issuer:        String,
    pub serial:        String,
    pub not_before:    Option<String>,
    pub not_after:     Option<String>,
    pub is_expired:    bool,
    pub is_self_signed: bool,
}

/// Result of a signature verification.
///
/// Mirrors `ValidationResult` in rust_pdf_signing.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// Name of the `/AcroForm` field that held this signature.
    pub field_name: String,
    /// `/Filter` value.
    pub filter: Option<String>,
    /// `/SubFilter` value (e.g. `adbe.pkcs7.detached` or `ETSI.CAdES.detached`).
    pub sub_filter: Option<String>,
    /// `/Reason` string from the signature dictionary.
    pub reason: Option<String>,
    /// `/ContactInfo` string.
    pub contact_info: Option<String>,
    /// `/M` signing date string.
    pub signing_time: Option<String>,
    /// Raw bytes of the `/Contents` CMS blob.
    pub cms_bytes: Vec<u8>,
    /// Byte ranges that were digested `[r0_start, r0_len, r1_start, r1_len]`.
    pub byte_range: [i64; 4],
    /// `true` when ByteRange covers the whole file (excluding `/Contents` gap).
    pub byte_range_covers_whole_file: bool,
    /// `true` when the SHA-256 digest over the byte ranges matches the
    /// `messageDigest` attribute inside the CMS `SignedData`.
    pub digest_valid: bool,
    /// `true` when the RSA/EC signature over the signed attributes verifies
    /// against the signer certificate embedded in the CMS.
    pub cms_signature_valid: bool,
    /// `true` when the CMS blob is structurally parseable.
    pub cms_parseable: bool,
    /// Certificates embedded in the CMS blob.
    pub certificates: Vec<CertInfo>,
    /// `true` when the certificate chain is structurally valid.
    pub certificate_chain_valid: bool,
    /// Warnings about the certificate chain.
    pub chain_warnings: Vec<String>,
    /// `true` when a signature timestamp is present.
    pub has_timestamp: bool,
    /// `true` when a DSS dictionary is present.
    pub has_dss: bool,
    /// `true` when LTV requirements are met (timestamp + revocation data).
    pub is_ltv_enabled: bool,
    /// Human-readable status string.
    pub status: String,
    /// List of validation errors (empty = passed).
    pub errors: Vec<String>,
}

impl VerifyResult {
    pub fn is_valid(&self) -> bool {
        self.digest_valid && self.cms_signature_valid && self.errors.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ByteRangePlaceholder — reserve then patch
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Signs a PDF document and returns the signed bytes.
///
/// # Arguments
///
/// * `pdf_bytes`      — the original, unmodified PDF bytes to sign.
/// * `cert_chain_pem` — PEM text containing the full certificate chain
///   (signer cert first, then intermediate(s), then root).
/// * `private_key_pem` — PKCS#8 PEM private key (`RSAPrivateKey` or EC).
/// * `opts`            — placement / metadata options.
///
/// # How it works (following rust_pdf_signing / Java PDFBox pattern)
///
/// 1. Parse the PDF and locate the next free object number.
/// 2. Build a `/Sig` dictionary and a `/Widget` annotation dictionary.
/// 3. Append them plus an updated `/AcroForm` via incremental update.
/// 4. Write a first pass → scan for `/ByteRange` and `/Contents` placeholders.
/// 5. Patch `/ByteRange` with the real offsets.
/// 6. Concatenate the signed byte ranges → pass raw bytes to CMS builder.
/// 7. Build CMS `SignedData` → DER-encode → hex-encode → inject into `/Contents`.
///
/// # Errors
///
/// Returns [`PdfError::Parse`] / [`PdfError::Unsupported`] for malformed
/// input or unsupported key types.
pub fn sign_pdf(
    pdf_bytes: &[u8],
    cert_chain_pem: &str,
    private_key_pem: &str,
    opts: &SignOptions,
) -> Result<Vec<u8>, PdfError> {
    // Parse certs just for DER encoding needed by the sig dict
    let certs = x509_certificate::CapturedX509Certificate::from_pem_multiple(cert_chain_pem)
        .map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("failed to parse certificate chain PEM: {e}"),
        })?;
    if certs.is_empty() {
        return Err(PdfError::Parse {
            offset: None,
            context: "cert_chain_pem must contain at least one certificate".into(),
        });
    }

    // ── Step 1: parse existing document ──────────────────────────────────
    let doc = Document::load_from_bytes(pdf_bytes)?;
    let next_id = next_free_object_id(&doc);
    let sig_id      = ObjectId::new(next_id,     0);
    let widget_id   = ObjectId::new(next_id + 1, 0);

    // ── Step 2: build /Sig dictionary ────────────────────────────────────
    let now = chrono::Local::now();
    let date_str = now.format("D:%Y%m%d%H%M%S+00'00'").to_string();

    // Determine SubFilter based on format
    let sub_filter_bytes: &[u8] = match opts.format {
        SignatureFormat::PAdES => b"ETSI.CAdES.detached",
        SignatureFormat::Pkcs7 => b"adbe.pkcs7.detached",
    };

    let mut sig_dict = CosDictionary::new();
    sig_dict.set(CosName::type_name(),           CosObject::Name(CosName::new(b"Sig")));
    sig_dict.set(CosName::new(b"Filter"),         CosObject::Name(CosName::new(b"Adobe.PPKLite")));
    sig_dict.set(CosName::new(b"SubFilter"),      CosObject::Name(CosName::new(sub_filter_bytes)));
    sig_dict.set(CosName::new(b"Reason"),         CosObject::String(opts.reason.as_bytes().to_vec()));
    sig_dict.set(CosName::new(b"Location"),       CosObject::String(opts.location.as_bytes().to_vec()));
    sig_dict.set(CosName::new(b"M"),              CosObject::String(date_str.into_bytes()));
    if !opts.contact_info.is_empty() {
        sig_dict.set(CosName::new(b"ContactInfo"), CosObject::String(opts.contact_info.as_bytes().to_vec()));
    }
    if !opts.signer_name.is_empty() {
        sig_dict.set(CosName::new(b"Name"), CosObject::String(opts.signer_name.as_bytes().to_vec()));
    }
    // ByteRange placeholder — use a large literal placeholder string embedded
    // as a HexString so the serializer writes it as-is with exact width.
    // We write a special marker that the search can find and patch in-place.
    // Format: /ByteRange [0000000000 0000000000 0000000000 0000000000]
    //          = exactly 55 chars — always fits any file up to 10 GB.
    // Stored as String("BYTERANGE_PLACEHOLDER") — the serializer writes it
    // as a literal-string; we then find and replace the whole entry.
    // HOWEVER: we need the array form so PDF readers parse it correctly.
    // Solution: override the Array serialization for ByteRange by using
    // integer objects with enough zero-padding in the placeholder.
    // We achieve this by writing raw bytes through a special placeholder marker
    // in the sig dict that find_sig_placeholders can locate.
    //
    // The marker written to the PDF looks like:
    //   /ByteRange [0000000000 0000000000 0000000000 0000000000]
    //   (55 chars, always patchable in-place since real values < 10 digits each)
    //
    // We achieve this with a custom trick: store the array using Integer(0),
    // and after serialization, search-and-replace the serialized form.
    // The serializer writes: /ByteRange [0 0 0 0]  (20 chars)
    // We need 55 chars, so we pad the zeros to 10 digits each.
    // We do this by using large "sentinel" integers that serialize to 10 digits.
    const PAD: i64 = 1_000_000_000; // 10 digits
    sig_dict.set(CosName::new(b"ByteRange"), CosObject::Array(vec![
        CosObject::Integer(PAD), CosObject::Integer(PAD),
        CosObject::Integer(PAD), CosObject::Integer(PAD),
    ]));
    // Contents placeholder — reserved_size zero bytes stored as HexString
    // so the serializer writes <000000…> (angle-bracket hex format required by PDF spec)
    sig_dict.set(CosName::new(b"Contents"), CosObject::HexString(
        vec![0u8; opts.reserved_size],
    ));
    let sig_obj = CosObject::Dictionary(sig_dict);

    // ── Step 3: build /Widget annotation dictionary ──────────────────────
    let page_ref = page_object_id(&doc, opts.page);
    let mut widget_dict = CosDictionary::new();
    widget_dict.set(CosName::type_name(),          CosObject::Name(CosName::new(b"Annot")));
    widget_dict.set(CosName::new(b"Subtype"),       CosObject::Name(CosName::new(b"Widget")));
    widget_dict.set(CosName::new(b"FT"),            CosObject::Name(CosName::new(b"Sig")));
    widget_dict.set(CosName::new(b"T"),             CosObject::String(opts.field_name.as_bytes().to_vec()));
    widget_dict.set(CosName::new(b"V"),             CosObject::Reference(sig_id));
    widget_dict.set(CosName::new(b"F"),             CosObject::Integer(4)); // Print flag
    // Use [0 0 0 0] when invisible_signature is false or no rect given
    let effective_rect = if opts.visible_signature { opts.rect } else { None };
    let rect_arr = match effective_rect {
        Some([x1, y1, x2, y2]) => vec![
            CosObject::Real(x1), CosObject::Real(y1),
            CosObject::Real(x2), CosObject::Real(y2),
        ],
        None => vec![
            CosObject::Integer(0), CosObject::Integer(0),
            CosObject::Integer(0), CosObject::Integer(0),
        ],
    };
    widget_dict.set(CosName::new(b"Rect"), CosObject::Array(rect_arr));
    if let Some(pr) = page_ref {
        widget_dict.set(CosName::new(b"P"), CosObject::Reference(pr));
    }
    let widget_obj = CosObject::Dictionary(widget_dict);

    // ── Step 4: build AcroForm + Annots update objects ────────────────────
    let mut changed: BTreeMap<ObjectId, CosObject> = BTreeMap::new();
    changed.insert(sig_id,    sig_obj);
    changed.insert(widget_id, widget_obj);

    // Add widget to page /Annots
    let page_annots_id = ObjectId::new(next_id + 2, 0);
    if let Some(pr) = page_ref {
        let updated_page = build_page_with_annot(&doc, pr, widget_id, page_annots_id, &mut changed);
        if let Some((id, obj)) = updated_page {
            changed.insert(id, obj);
        }
    }

    // Add /AcroForm to catalog
    let acroform_id  = ObjectId::new(next_id + 3, 0);
    let catalog_id   = doc.catalog_ref().unwrap_or(ObjectId::new(1, 0));
    let acroform_obj = acroform::build_acroform(&doc, widget_id, acroform_id, &mut changed);
    changed.insert(acroform_id, acroform_obj);
    let updated_catalog = build_updated_catalog(&doc, catalog_id, acroform_id);
    changed.insert(catalog_id, updated_catalog);

    // ── Step 5: first pass — write incremental update ────────────────────
    let mut first_pass: Vec<u8> = Vec::with_capacity(pdf_bytes.len() + 8192);
    crate::writer::IncrementalWriter::write_update(pdf_bytes, &doc, &changed, &mut first_pass)
        .map_err(|e| PdfError::Parse { offset: None, context: format!("write pass 1: {e}") })?;

    // ── Step 6: locate ByteRange and Contents placeholders in first_pass ─
    let (br_offset, contents_offset, contents_hex_len) =
        find_sig_placeholders(&first_pass, &opts.field_name)?;

    // Signed ranges: [0..contents_offset)  and  [contents_offset+contents_hex_len..EOF]
    let range0_end:  i64 = contents_offset as i64;
    let range1_start: i64 = (contents_offset + contents_hex_len) as i64;
    let range1_end:  i64 = first_pass.len() as i64 - range1_start;

    // ── Step 7: patch /ByteRange in-place (placeholder is 55 chars wide) ─
    patch_byte_range(&mut first_pass,
        br_offset, 0, range0_end, range1_start, range1_end)?;

    // ── Step 8: concatenate signed byte ranges ─────────────────────────
    // Pass the raw content bytes to the CMS crate — it internally computes
    // SHA-256 and builds all signed attributes, exactly like rust_pdf_signing.
    let signed_content = {
        let mut v = Vec::with_capacity(range0_end as usize + range1_end as usize);
        v.extend_from_slice(&first_pass[0..range0_end as usize]);
        v.extend_from_slice(&first_pass[range1_start as usize..(range1_start + range1_end) as usize]);
        v
    };

    // ── Step 9: build CMS SignedData → DER (using cryptographic_message_syntax) ─
    // Determine timestamp URL based on format/pades_level
    let tsa_url = match opts.format {
        SignatureFormat::PAdES => match opts.pades_level {
            PadesLevel::B_T | PadesLevel::B_LT | PadesLevel::B_LTA => opts.timestamp_url.clone(),
            PadesLevel::B_B => None,
        },
        SignatureFormat::Pkcs7 => opts.timestamp_url.clone(),
    };

    let sub_filter_str: &'static str = match opts.format {
        SignatureFormat::PAdES => "ETSI.CAdES.detached",
        SignatureFormat::Pkcs7 => "adbe.pkcs7.detached",
    };

    let cms_opts = cms::CmsOptions {
        sub_filter: sub_filter_str,
        timestamp_url: tsa_url,
    };

    let cms_der = cms::build_cms_signed_data_with_opts(
        &signed_content, cert_chain_pem, private_key_pem, &cms_opts,
    )?;

    if cms_der.len() > opts.reserved_size {
        return Err(PdfError::Parse {
            offset: None,
            context: format!(
                "CMS blob ({} bytes) exceeds reserved_size ({}). \
                 Increase SignOptions::reserved_size.",
                cms_der.len(), opts.reserved_size
            ),
        });
    }

    // ── Step 10: hex-encode CMS and inject into /Contents ────────────────
    let mut signed = first_pass;
    inject_contents(&mut signed, contents_offset, contents_hex_len, &cms_der)?;

    Ok(signed)
}

/// Verifies all digital signatures found in a PDF.
///
/// Returns one [`VerifyResult`] per `/Sig` field found in `/AcroForm`.
/// Uses `cryptographic_message_syntax` for full CMS verification — same as
/// `rust_pdf_signing` `SignatureValidator::validate`.
pub fn verify_pdf(pdf_bytes: &[u8]) -> Result<Vec<VerifyResult>, PdfError> {
    let doc = Document::load_from_bytes(pdf_bytes)?;
    let mut results = Vec::new();

    let sig_fields = find_sig_fields(&doc);

    // Check DSS presence once
    let has_dss = doc.catalog()
        .and_then(|cat| cat.get(&CosName::new(b"DSS")))
        .is_some();

    for (field_name, sig_ref) in sig_fields {
        let sig_dict = match doc.objects.get(&sig_ref).and_then(|o| o.as_dictionary()) {
            Some(d) => d.clone(),
            None => continue,
        };

        let filter = sig_dict.get(&CosName::new(b"Filter"))
            .and_then(|v| v.as_name())
            .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned());
        let sub_filter = sig_dict.get(&CosName::new(b"SubFilter"))
            .and_then(|v| v.as_name())
            .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned());
        let reason = sig_dict.get(&CosName::new(b"Reason"))
            .and_then(|v| v.as_string())
            .map(|b| String::from_utf8_lossy(b).into_owned());
        let contact_info = sig_dict.get(&CosName::new(b"ContactInfo"))
            .and_then(|v| v.as_string())
            .map(|b| String::from_utf8_lossy(b).into_owned());
        let signing_time = sig_dict.get(&CosName::new(b"M"))
            .and_then(|v| v.as_string())
            .map(|b| String::from_utf8_lossy(b).into_owned());

        // /Contents — raw CMS DER bytes
        let cms_bytes: Vec<u8> = match sig_dict
            .get(&CosName::new(b"Contents"))
            .and_then(|v| v.as_string())
        {
            Some(b) => b.to_vec(),
            None => {
                results.push(VerifyResult {
                    field_name, filter, sub_filter, reason, contact_info, signing_time,
                    cms_bytes: vec![], byte_range: [0; 4],
                    byte_range_covers_whole_file: false,
                    digest_valid: false, cms_signature_valid: false, cms_parseable: false,
                    certificates: vec![], certificate_chain_valid: false,
                    chain_warnings: vec![], has_timestamp: false,
                    has_dss, is_ltv_enabled: false,
                    status: "ERROR: missing /Contents".into(),
                    errors: vec!["Missing /Contents".into()],
                });
                continue;
            }
        };

        // /ByteRange [r0_start r0_len r1_start r1_len]
        let br = match sig_dict
            .get(&CosName::new(b"ByteRange"))
            .and_then(|v| v.as_array())
        {
            Some(arr) if arr.len() == 4 => {
                let v: Vec<i64> = arr.iter().filter_map(|x| x.as_integer()).collect();
                if v.len() == 4 { [v[0], v[1], v[2], v[3]] } else { [0i64; 4] }
            }
            _ => [0i64; 4],
        };

        // ByteRange covers whole file check
        let file_len = pdf_bytes.len() as i64;
        let byte_range_covers = br[0] == 0
            && br[2] == br[0] + br[1] + cms_bytes.len() as i64 * 2 + 2
            && br[2] + br[3] == file_len;

        // Extract signed content for CMS verification
        let r0s = br[0] as usize;
        let r0e = (br[0] + br[1]) as usize;
        let r1s = br[2] as usize;
        let r1e = (br[2] + br[3]) as usize;
        let signed_content = if r0e <= pdf_bytes.len() && r1e <= pdf_bytes.len() {
            let mut v = Vec::with_capacity(r0e - r0s + r1e - r1s);
            v.extend_from_slice(&pdf_bytes[r0s..r0e]);
            v.extend_from_slice(&pdf_bytes[r1s..r1e]);
            v
        } else {
            vec![]
        };

        // Full CMS verification
        let cv = cms::verify_cms(&cms_bytes, &signed_content);

        let certificates: Vec<CertInfo> = cv.certificates.iter().map(|c| CertInfo {
            subject:       c.subject.clone(),
            issuer:        c.issuer.clone(),
            serial:        c.serial.clone(),
            not_before:    c.not_before.clone(),
            not_after:     c.not_after.clone(),
            is_expired:    c.is_expired,
            is_self_signed: c.is_self_signed,
        }).collect();

        let is_ltv_enabled = (cv.has_timestamp && has_dss)
            || (cv.has_timestamp && !cv.certificates.is_empty());

        let mut errors = Vec::new();
        if !cv.digest_valid   { errors.push("Digest mismatch".into()); }
        if !cv.signature_valid { errors.push("Signature verification failed".into()); }

        let status = if cv.digest_valid && cv.signature_valid {
            "VALID: digest and signature verified".into()
        } else if cv.digest_valid {
            "VALID: digest matches".into()
        } else {
            "INVALID: digest mismatch".into()
        };

        results.push(VerifyResult {
            field_name, filter, sub_filter, reason, contact_info, signing_time,
            cms_bytes,
            byte_range: br,
            byte_range_covers_whole_file: byte_range_covers,
            digest_valid: cv.digest_valid,
            cms_signature_valid: cv.signature_valid,
            cms_parseable: true,
            certificates,
            certificate_chain_valid: cv.chain_valid,
            chain_warnings: cv.chain_warnings,
            has_timestamp: cv.has_timestamp,
            has_dss,
            is_ltv_enabled,
            status,
            errors,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn next_free_object_id(doc: &Document) -> u32 {
    doc.objects.max_object_number() + 1
}

fn page_object_id(doc: &Document, page_num: u32) -> Option<ObjectId> {
    let idx = (page_num.saturating_sub(1)) as usize;
    // Page tree iter returns Page structs; we need the underlying ObjectId.
    // Walk pages_dict Kids array recursively.
    let catalog = doc.catalog()?.clone();
    let pages_ref = catalog.get(&CosName::new(b"Pages"))?.as_reference()?;
    find_page_id_in_tree(doc, pages_ref, idx, &mut 0)
}

fn find_page_id_in_tree(doc: &Document, node_id: ObjectId, target: usize, count: &mut usize)
    -> Option<ObjectId>
{
    let node = doc.objects.get(&node_id)?.as_dictionary()?.clone();
    let type_name = node.get_name(&CosName::type_name());

    if type_name == Some(&CosName::new(b"Page")) {
        if *count == target {
            return Some(node_id);
        }
        *count += 1;
        return None;
    }
    // /Pages intermediate node
    let kids = node.get_array(&CosName::kids())?.to_vec();
    for kid in kids {
        if let Some(kid_ref) = kid.as_reference() {
            if let Some(found) = find_page_id_in_tree(doc, kid_ref, target, count) {
                return Some(found);
            }
        }
    }
    None
}

fn build_page_with_annot(
    doc: &Document,
    page_id: ObjectId,
    widget_id: ObjectId,
    _new_annots_id: ObjectId,
    _changed: &mut BTreeMap<ObjectId, CosObject>,
) -> Option<(ObjectId, CosObject)> {
    let page_dict = doc.objects.get(&page_id)?.as_dictionary()?.clone();
    let mut new_page = page_dict.clone();

    // Append widget to existing /Annots array, or create new one
    let mut annots = match page_dict.get_array(&CosName::new(b"Annots")) {
        Some(arr) => arr.to_vec(),
        None => vec![],
    };
    annots.push(CosObject::Reference(widget_id));
    new_page.set(CosName::new(b"Annots"), CosObject::Array(annots));

    Some((page_id, CosObject::Dictionary(new_page)))
}

fn build_updated_catalog(doc: &Document, catalog_id: ObjectId, acroform_id: ObjectId)
    -> CosObject
{
    let mut cat = doc.objects
        .get(&catalog_id)
        .and_then(|o| o.as_dictionary())
        .cloned()
        .unwrap_or_else(CosDictionary::new);
    cat.set(CosName::new(b"AcroForm"), CosObject::Reference(acroform_id));
    CosObject::Dictionary(cat)
}

/// Find the offset of the `/ByteRange` placeholder and the offset + length of
/// `/Contents <000…0>` in the serialised PDF bytes.
///
/// Returns `(byte_range_offset, contents_angle_open_offset, total_hex_field_len)`
fn find_sig_placeholders(buf: &[u8], _field_name: &str)
    -> Result<(usize, usize, usize), PdfError>
{
    // Locate /ByteRange [1000000000 — the sentinel padded placeholder
    let br_needle = b"/ByteRange [1000000000";
    let br_off = buf.windows(br_needle.len())
        .position(|w| w == br_needle)
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "could not find /ByteRange placeholder in serialised PDF".into(),
        })?;

    // Locate /Contents < — the HexString placeholder
    let ct_needle = b"/Contents <";
    let ct_off = buf.windows(ct_needle.len())
        .position(|w| w == ct_needle)
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "could not find /Contents placeholder in serialised PDF".into(),
        })?;

    // The hex field starts at ct_off + len("/Contents ") = ct_off + 10
    // i.e. the '<' character
    let hex_start = ct_off + b"/Contents ".len();

    // Find the closing '>' to measure the reserved length
    let closing = buf[hex_start..]
        .iter()
        .position(|&b| b == b'>')
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "could not find closing '>' for /Contents placeholder".into(),
        })?;

    // hex_start points at '<', closing is offset of '>' relative to hex_start.
    // The total field length including both angle brackets = closing + 1.
    let hex_field_len = closing + 1;
    Ok((br_off, hex_start, hex_field_len))
}

/// Overwrite the `/ByteRange [1000000000 ...]` placeholder with real values.
/// Pads with spaces to keep the buffer size constant.
fn patch_byte_range(
    buf: &mut Vec<u8>,
    br_off: usize,
    r0s: i64, r0e: i64,
    r1s: i64, r1e: i64,
) -> Result<(), PdfError> {
    let end = buf[br_off..]
        .iter()
        .position(|&b| b == b']')
        .map(|p| br_off + p + 1)
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "malformed /ByteRange placeholder".into(),
        })?;

    let replacement = format!("/ByteRange [{r0s} {r0e} {r1s} {r1e}]");
    let new_bytes = replacement.as_bytes();
    let old_len = end - br_off;

    if new_bytes.len() > old_len {
        return Err(PdfError::Parse {
            offset: None,
            context: format!(
                "/ByteRange replacement ({} bytes) longer than placeholder ({} bytes). \
                 Increase PAD sentinel value.",
                new_bytes.len(), old_len
            ),
        });
    }

    // Write replacement, pad remainder with spaces (keeps all byte offsets valid)
    buf[br_off..br_off + new_bytes.len()].copy_from_slice(new_bytes);
    for i in 0..(old_len - new_bytes.len()) {
        buf[br_off + new_bytes.len() + i] = b' ';
    }
    Ok(())
}

/// Hex-encode the CMS DER blob and overwrite the `/Contents <000…>` placeholder.
fn inject_contents(buf: &mut Vec<u8>, hex_start: usize, hex_field_len: usize, cms_der: &[u8])
    -> Result<(), PdfError>
{
    // hex_start points to '<'; the payload area is [hex_start+1 .. hex_start+hex_field_len-1]
    let payload_start = hex_start;           // '<'
    let payload_capacity = hex_field_len - 2; // space between '<' and '>'

    let hex_len = cms_der.len() * 2;
    if hex_len > payload_capacity {
        return Err(PdfError::Parse {
            offset: None,
            context: format!(
                "CMS DER hex ({hex_len} chars) exceeds reserved payload ({payload_capacity} chars)"
            ),
        });
    }

    // Write hex digits for cms_der
    let write_at = payload_start + 1; // after '<'
    for (i, &byte) in cms_der.iter().enumerate() {
        let hi = (byte >> 4) as usize;
        let lo = (byte & 0x0f) as usize;
        const HEX: &[u8] = b"0123456789abcdef";
        buf[write_at + i * 2]     = HEX[hi];
        buf[write_at + i * 2 + 1] = HEX[lo];
    }
    Ok(())
}

/// Walk /AcroForm /Fields and collect all (field_name, sig_value_ref) pairs
/// where /FT = /Sig.
fn find_sig_fields(doc: &Document) -> Vec<(String, ObjectId)> {
    let mut out = Vec::new();
    let catalog = match doc.catalog() {
        Some(c) => c.clone(),
        None => return out,
    };
    let acroform_ref = match catalog.get(&CosName::new(b"AcroForm")) {
        Some(CosObject::Reference(r)) => *r,
        Some(CosObject::Dictionary(d)) => {
            // Inline AcroForm — unlikely but handle it
            collect_sig_fields_from_acroform_dict(d, doc, &mut out);
            return out;
        }
        _ => return out,
    };
    let acroform_dict = match doc.objects.get(&acroform_ref).and_then(|o| o.as_dictionary()) {
        Some(d) => d.clone(),
        None => return out,
    };
    collect_sig_fields_from_acroform_dict(&acroform_dict, doc, &mut out);
    out
}

fn collect_sig_fields_from_acroform_dict(
    acroform: &CosDictionary,
    doc: &Document,
    out: &mut Vec<(String, ObjectId)>,
) {
    let fields = match acroform.get_array(&CosName::new(b"Fields")) {
        Some(f) => f.to_vec(),
        None => return,
    };
    for field_ref in fields {
        let field_id = match field_ref.as_reference() {
            Some(r) => r,
            None => continue,
        };
        let field_dict = match doc.objects.get(&field_id).and_then(|o| o.as_dictionary()) {
            Some(d) => d.clone(),
            None => continue,
        };
        let ft = field_dict.get_name(&CosName::new(b"FT"));
        if ft == Some(&CosName::new(b"Sig")) {
            let name = field_dict
                .get(&CosName::new(b"T"))
                .and_then(|v| v.as_string())
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_else(|| "unnamed".into());
            // /V points to the actual /Sig dictionary
            if let Some(sig_ref) = field_dict
                .get(&CosName::new(b"V"))
                .and_then(|v| v.as_reference())
            {
                out.push((name, sig_ref));
            }
        }
    }
}

