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
pub mod appearance;
pub mod asn1;
pub mod cms;
pub mod ltv;
pub mod validator;

use std::collections::BTreeMap;
use std::path::PathBuf;

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

    /// Optional path to a PNG or JPEG image to embed in the visible signature
    /// appearance stream. Only used when `visible_signature == true`.
    /// When `None`, a text-only appearance is generated (signer name, reason, date).
    pub image_path: Option<PathBuf>,
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
            image_path:    None,
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
    // Use UTC so that the /M date string agrees with the signingTime attribute
    // inside the CMS blob (which is also UTC). Using Local time with +00'00'
    // would produce a mismatch that Adobe flags as "signed in future".
    let now = chrono::Utc::now();
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
    sig_dict.set(CosName::new(b"M"),              CosObject::String(date_str.clone().into_bytes()));
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

    // Resolve anchor-tag → compute placement rect if requested.
    // anchor_tag overrides opts.rect; if anchor is not found, return Err.
    let resolved_rect: Option<[f64; 4]> = if opts.visible_signature {
        if let (Some(tag), Some(w), Some(h)) = (
            opts.anchor_tag.as_deref(),
            opts.anchor_width,
            opts.anchor_height,
        ) {
            let pid = page_ref.ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page {} not found in document", opts.page),
            })?;
            let rect = resolve_anchor_rect(&doc, pid, tag, w, h, &opts.anchor_mode)?;
            Some(rect)
        } else {
            opts.rect
        }
    } else {
        None
    };

    let mut widget_dict = CosDictionary::new();
    widget_dict.set(CosName::type_name(),          CosObject::Name(CosName::new(b"Annot")));
    widget_dict.set(CosName::new(b"Subtype"),       CosObject::Name(CosName::new(b"Widget")));
    widget_dict.set(CosName::new(b"FT"),            CosObject::Name(CosName::new(b"Sig")));
    widget_dict.set(CosName::new(b"T"),             CosObject::String(opts.field_name.as_bytes().to_vec()));
    widget_dict.set(CosName::new(b"V"),             CosObject::Reference(sig_id));
    widget_dict.set(CosName::new(b"F"),             CosObject::Integer(4)); // Print flag
    // effective_rect: anchor-resolved rect OR opts.rect (when visible), else None
    let effective_rect = resolved_rect;
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

    // ── Build /AP appearance stream for visible signatures ────────────────
    // Two-layer n0/n2 structure — allocate 5 IDs:
    //   ap_id   = next+2  (outer AP/N Form)
    //   n0_id   = next+3  (/n0 empty background sub-Form)
    //   n2_id   = next+4  (/n2 foreground sub-Form)
    //   img_id  = next+5  (Image XObject, image mode only)
    //   font_id = next+6  (Helvetica font, text-only mode only)
    // page_annots_id = next+7,  acroform_id = next+8
    let ap_id   = ObjectId::new(next_id + 2, 0);
    let n0_id   = ObjectId::new(next_id + 3, 0);
    let n2_id   = ObjectId::new(next_id + 4, 0);
    let img_id  = ObjectId::new(next_id + 5, 0);
    let font_id = ObjectId::new(next_id + 6, 0);

    if opts.visible_signature {
        if let Some(r) = effective_rect {
            let ap_result = appearance::build_appearance(
                r,
                opts.image_path.as_deref(),
                &opts.signer_name,
                &opts.reason,
                &date_str,
                ap_id,
                n0_id,
                n2_id,
                img_id,
                font_id,
            ).map_err(|e| PdfError::Parse {
                offset: None,
                context: format!("appearance build failed: {e}"),
            })?;

            // Wire /AP << /N <ap_id> >> into the widget
            let mut ap_dict = CosDictionary::new();
            ap_dict.set(CosName::new(b"N"), CosObject::Reference(ap_id));
            widget_dict.set(CosName::new(b"AP"), CosObject::Dictionary(ap_dict));

            // Insert all five appearance objects into the changed map
            let mut ap_changed: BTreeMap<ObjectId, CosObject> = BTreeMap::new();
            ap_changed.insert(ap_result.ap_id,  ap_result.ap_obj);   // outer AP/N
            ap_changed.insert(ap_result.n0_id,  ap_result.n0_obj);   // /n0 background
            ap_changed.insert(ap_result.n2_id,  ap_result.n2_obj);   // /n2 foreground
            if let (Some(iid), Some(iobj)) = (ap_result.img_id, ap_result.img_obj) {
                ap_changed.insert(iid, iobj);                          // Image XObject
            }
            ap_changed.insert(ap_result.font_id, ap_result.font_obj); // Helvetica font

            let widget_obj = CosObject::Dictionary(widget_dict);

            // ── Step 4: build AcroForm + Annots update objects ────────────────
            // ap=+2, n0=+3, n2=+4, img=+5, font=+6  →  annots=+7, acroform=+8
            let page_annots_id = ObjectId::new(next_id + 7, 0);
            let mut changed: BTreeMap<ObjectId, CosObject> = BTreeMap::new();
            changed.insert(sig_id,    sig_obj);
            changed.insert(widget_id, widget_obj);
            changed.extend(ap_changed);

            if let Some(pr) = page_ref {
                let updated_page = build_page_with_annot(&doc, pr, widget_id, page_annots_id, &mut changed);
                if let Some((id, obj)) = updated_page {
                    changed.insert(id, obj);
                }
            }

            let acroform_id  = ObjectId::new(next_id + 8, 0);
            let catalog_id   = doc.catalog_ref().unwrap_or(ObjectId::new(1, 0));
            let acroform_obj = acroform::build_acroform(&doc, widget_id, acroform_id, &mut changed);
            changed.insert(acroform_id, acroform_obj);
            let updated_catalog = build_updated_catalog(&doc, catalog_id, acroform_id);
            changed.insert(catalog_id, updated_catalog);

            return sign_pdf_with_changes(pdf_bytes, cert_chain_pem, private_key_pem, opts,
                changed, &date_str, sub_filter_bytes);
        }
    }

    let widget_obj = CosObject::Dictionary(widget_dict);

    // ── Step 4: build AcroForm + Annots update objects ────────────────────
    let mut changed: BTreeMap<ObjectId, CosObject> = BTreeMap::new();
    changed.insert(sig_id,    sig_obj);
    changed.insert(widget_id, widget_obj);

    // Add widget to page /Annots  (IDs next+2, next+3 for annots/acroform — no AP objects)
    let page_annots_id = ObjectId::new(next_id + 2, 0);
    if let Some(pr) = page_ref {
        let updated_page = build_page_with_annot(&doc, pr, widget_id, page_annots_id, &mut changed);
        if let Some((id, obj)) = updated_page {
            changed.insert(id, obj);
        }
    }

    let acroform_id  = ObjectId::new(next_id + 3, 0);
    let catalog_id   = doc.catalog_ref().unwrap_or(ObjectId::new(1, 0));
    let acroform_obj = acroform::build_acroform(&doc, widget_id, acroform_id, &mut changed);
    changed.insert(acroform_id, acroform_obj);
    let updated_catalog = build_updated_catalog(&doc, catalog_id, acroform_id);
    changed.insert(catalog_id, updated_catalog);

    sign_pdf_with_changes(pdf_bytes, cert_chain_pem, private_key_pem, opts,
        changed, &date_str, sub_filter_bytes)
}

// ---------------------------------------------------------------------------
// Shared sign-with-changes helper (write → patch ByteRange → CMS → inject)
// ---------------------------------------------------------------------------

fn sign_pdf_with_changes(
    pdf_bytes:        &[u8],
    cert_chain_pem:   &str,
    private_key_pem:  &str,
    opts:             &SignOptions,
    changed:          BTreeMap<ObjectId, CosObject>,
    date_str:         &str,
    _sub_filter_bytes: &[u8],
) -> Result<Vec<u8>, PdfError> {
    let doc = Document::load_from_bytes(pdf_bytes)?;

    // ── Step 5: first pass — write incremental update ────────────────────
    let mut first_pass: Vec<u8> = Vec::with_capacity(pdf_bytes.len() + 8192);
    crate::writer::IncrementalWriter::write_update(pdf_bytes, &doc, &changed, &mut first_pass)
        .map_err(|e| PdfError::Parse { offset: None, context: format!("write pass 1: {e}") })?;

    // ── Step 6: locate ByteRange and Contents placeholders ────────────────
    let (br_offset, contents_offset, contents_hex_len) =
        find_sig_placeholders(&first_pass, &opts.field_name)?;

    let range0_end:   i64 = contents_offset as i64;
    let range1_start: i64 = (contents_offset + contents_hex_len) as i64;
    let range1_end:   i64 = first_pass.len() as i64 - range1_start;

    // ── Step 7: patch /ByteRange in-place ────────────────────────────────
    patch_byte_range(&mut first_pass,
        br_offset, 0, range0_end, range1_start, range1_end)?;

    // ── Step 8: concatenate signed byte ranges ───────────────────────────
    let signed_content = {
        let mut v = Vec::with_capacity(range0_end as usize + range1_end as usize);
        v.extend_from_slice(&first_pass[0..range0_end as usize]);
        v.extend_from_slice(&first_pass[range1_start as usize..(range1_start + range1_end) as usize]);
        v
    };

    // ── Step 9: build CMS SignedData ─────────────────────────────────────
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

    // Decide whether to embed revocation data in the CMS signed attributes.
    // Mirrors rust_pdf_signing `digitally_sign_document` logic:
    //  - PKCS7: include_cms_revocation = user's include_crl || include_ocsp flags
    //  - PAdES B-B: no revocation, no timestamp
    //  - PAdES B-T: optional (user flags)
    //  - PAdES B-LT/LTA: always include both CRL + OCSP
    let is_pades = opts.format == SignatureFormat::PAdES;
    let (include_cms_crl, include_cms_ocsp, include_dss) = if is_pades {
        match opts.pades_level {
            PadesLevel::B_B  => (false, false, false),
            PadesLevel::B_T  => (opts.include_crl, opts.include_ocsp, false),
            PadesLevel::B_LT => (true, true, true),
            PadesLevel::B_LTA => (true, true, true),
        }
    } else {
        // PKCS7: use user flags; DSS is opt-in
        (opts.include_crl, opts.include_ocsp, opts.include_dss)
    };

    let cms_opts = cms::CmsOptions {
        sub_filter: sub_filter_str,
        timestamp_url: tsa_url.clone(),
        include_crl: include_cms_crl,
        include_ocsp: include_cms_ocsp,
        cert_chain_pem: cert_chain_pem.to_string(),
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

    // ── Step 10: inject /Contents ─────────────────────────────────────────
    let mut signed = first_pass;
    inject_contents(&mut signed, contents_offset, contents_hex_len, &cms_der)?;
    let _ = date_str;

    // ── Step 11: DSS dictionary (incremental append) ──────────────────────
    if include_dss {
        let certs_for_dss = x509_certificate::CapturedX509Certificate::from_pem_multiple(cert_chain_pem)
            .unwrap_or_default();
        signed = ltv::append_dss_dictionary(signed, &certs_for_dss)?;
    }

    // ── Step 12: PAdES B-LTA — document-level timestamp ───────────────────
    if is_pades && opts.pades_level == PadesLevel::B_LTA {
        if let Some(tsa_url_str) = &tsa_url {
            signed = append_document_timestamp(signed, tsa_url_str, opts.reserved_size)?;
        }
    }

    Ok(signed)
}

// ---------------------------------------------------------------------------
// PAdES B-LTA: append a document-level RFC 3161 timestamp as a new sig field
// ---------------------------------------------------------------------------

/// Append a `/Type /DocTimeStamp` signature field to `pdf_bytes` as an
/// incremental update. Mirrors `rust_pdf_signing::append_document_timestamp`.
fn append_document_timestamp(
    pdf_bytes:     Vec<u8>,
    tsa_url:       &str,
    reserved_size: usize,
) -> Result<Vec<u8>, PdfError> {
    use crate::cos::{CosName, CosObject, CosDictionary, ObjectId};
    use crate::writer::IncrementalWriter;
    use sha2::{Digest, Sha256};

    let doc = Document::load_from_bytes(&pdf_bytes)?;
    let mut obj_counter = doc.objects.max_object_number() + 1;
    let mut alloc = || { let id = ObjectId::new(obj_counter, 0); obj_counter += 1; id };
    let mut changed: BTreeMap<ObjectId, CosObject> = BTreeMap::new();

    // V (signature value) dictionary
    let v_id = alloc();
    {
        let mut v = CosDictionary::new();
        v.set(CosName::type_name(), CosObject::Name(CosName::new(b"Sig")));
        v.set(CosName::new(b"Filter"),    CosObject::Name(CosName::new(b"Adobe.PPKLite")));
        v.set(CosName::new(b"SubFilter"), CosObject::Name(CosName::new(b"ETSI.RFC3161")));
        const PAD: i64 = 1_000_000_000;
        v.set(CosName::new(b"ByteRange"), CosObject::Array(vec![
            CosObject::Integer(PAD), CosObject::Integer(PAD),
            CosObject::Integer(PAD), CosObject::Integer(PAD),
        ]));
        v.set(CosName::new(b"Contents"), CosObject::HexString(vec![0u8; reserved_size]));
        changed.insert(v_id, CosObject::Dictionary(v));
    }

    // Merged field + widget annotation
    let field_id  = alloc();
    let page_ref  = page_object_id(&doc, 1).unwrap_or(ObjectId::new(1, 0));
    {
        let ts_name = format!("DocTimestamp{}", rand::random::<u32>());
        let mut fw = CosDictionary::new();
        fw.set(CosName::new(b"FT"),      CosObject::Name(CosName::new(b"Sig")));
        fw.set(CosName::new(b"T"),       CosObject::String(ts_name.into_bytes()));
        fw.set(CosName::new(b"V"),       CosObject::Reference(v_id));
        fw.set(CosName::type_name(),     CosObject::Name(CosName::new(b"Annot")));
        fw.set(CosName::new(b"Subtype"), CosObject::Name(CosName::new(b"Widget")));
        fw.set(CosName::new(b"Rect"), CosObject::Array(vec![
            CosObject::Integer(0), CosObject::Integer(0),
            CosObject::Integer(0), CosObject::Integer(0),
        ]));
        fw.set(CosName::new(b"P"), CosObject::Reference(page_ref));
        fw.set(CosName::new(b"F"), CosObject::Integer(6));
        changed.insert(field_id, CosObject::Dictionary(fw));
    }

    // Add field to page /Annots
    {
        let page_dict = doc.objects.get(&page_ref)
            .and_then(|o| o.as_dictionary()).cloned()
            .unwrap_or_else(CosDictionary::new);
        let mut new_page = page_dict;
        let mut annots = new_page.get_array(&CosName::new(b"Annots"))
            .map(|a| a.to_vec()).unwrap_or_default();
        annots.push(CosObject::Reference(field_id));
        new_page.set(CosName::new(b"Annots"), CosObject::Array(annots));
        changed.insert(page_ref, CosObject::Dictionary(new_page));
    }

    // Update AcroForm
    let acroform_id = alloc();
    let acroform_obj = acroform::build_acroform(&doc, field_id, acroform_id, &mut changed);
    changed.insert(acroform_id, acroform_obj);
    let catalog_id = doc.catalog_ref().unwrap_or(ObjectId::new(1, 0));
    let mut cat = doc.objects.get(&catalog_id)
        .and_then(|o| o.as_dictionary()).cloned()
        .unwrap_or_else(CosDictionary::new);
    cat.set(CosName::new(b"AcroForm"), CosObject::Reference(acroform_id));
    changed.insert(catalog_id, CosObject::Dictionary(cat));

    // First pass: write placeholder
    let mut first_pass: Vec<u8> = Vec::with_capacity(pdf_bytes.len() + 8192);
    IncrementalWriter::write_update(&pdf_bytes, &doc, &changed, &mut first_pass)
        .map_err(|e| PdfError::Parse { offset: None, context: format!("DTS write: {e}") })?;

    // Patch ByteRange — use LAST occurrence (DocTimestamp is at end; existing sigs are earlier)
    let (br_off, ct_off, ct_len) = find_last_sig_placeholders(&first_pass)?;
    let r0e: i64 = ct_off as i64;
    let r1s: i64 = (ct_off + ct_len) as i64;
    let r1e: i64 = first_pass.len() as i64 - r1s;
    patch_byte_range(&mut first_pass, br_off, 0, r0e, r1s, r1e)?;

    // Hash the signed ranges
    let mut hasher = Sha256::new();
    hasher.update(&first_pass[..r0e as usize]);
    hasher.update(&first_pass[r1s as usize..(r1s + r1e) as usize]);
    let file_hash = hasher.finalize().to_vec();

    // Fetch timestamp token
    let ts_token = ltv::fetch_timestamp_token(tsa_url, &file_hash)?;
    if ts_token.len() > reserved_size {
        return Err(PdfError::Parse {
            offset: None,
            context: format!(
                "DocTimestamp token ({} bytes) > reserved_size ({}).",
                ts_token.len(), reserved_size
            ),
        });
    }

    inject_contents(&mut first_pass, ct_off, ct_len, &ts_token)?;
    Ok(first_pass)
}

/// Verifies all digital signatures found in a PDF.
///
/// Returns one [`VerifyResult`] per `/Sig` field found in `/AcroForm`.
/// Uses `cryptographic_message_syntax` for full CMS verification — same as
/// `rust_pdf_signing` `SignatureValidator::validate`.
pub fn verify_pdf(pdf_bytes: &[u8]) -> Result<Vec<VerifyResult>, PdfError> {
    use validator::SignatureValidator;

    let val_results = match SignatureValidator::validate(pdf_bytes) {
        Ok(v) => v,
        // If no signatures found, return empty vec (backward-compatible behaviour)
        Err(PdfError::Parse { ref context, .. }) if context.contains("No digital signature") => {
            return Ok(vec![]);
        }
        Err(e) => return Err(e),
    };
    let mut results = Vec::with_capacity(val_results.len());

    for v in val_results {
        let br = if v.byte_range.len() == 4 {
            [v.byte_range[0], v.byte_range[1], v.byte_range[2], v.byte_range[3]]
        } else {
            [0i64; 4]
        };

        // Convert validator::CertInfo → signing::CertInfo
        let certificates: Vec<CertInfo> = v.certificates.iter().map(|c| CertInfo {
            subject:       c.subject.clone(),
            issuer:        c.issuer.clone(),
            serial:        c.serial_number.clone(),
            not_before:    c.not_before.map(|t| t.to_rfc3339()),
            not_after:     c.not_after.map(|t| t.to_rfc3339()),
            is_expired:    c.is_expired,
            is_self_signed: c.is_self_signed,
        }).collect();

        // Build status string
        let all_ok = v.digest_match && v.cms_signature_valid;
        let status = if all_ok {
            "VALID: digest and signature verified".into()
        } else if v.digest_match {
            "PARTIAL: digest matches but signature failed".into()
        } else if v.cms_signature_valid {
            "PARTIAL: signature valid but digest mismatch".into()
        } else {
            format!("INVALID: {}", v.errors.join("; "))
        };

        results.push(VerifyResult {
            field_name:                v.field_name.unwrap_or_else(|| "unnamed".into()),
            filter:                    v.filter,
            sub_filter:                v.sub_filter,
            reason:                    v.reason,
            contact_info:              v.contact_info,
            signing_time:              v.signing_time,
            cms_bytes:                 vec![],    // not needed in VerifyResult
            byte_range:                br,
            byte_range_covers_whole_file: v.byte_range_covers_whole_file,
            digest_valid:              v.digest_match,
            cms_signature_valid:       v.cms_signature_valid,
            cms_parseable:             true,
            certificates,
            certificate_chain_valid:   v.certificate_chain_valid,
            chain_warnings:            v.chain_warnings,
            has_timestamp:             v.has_timestamp,
            has_dss:                   v.has_dss,
            is_ltv_enabled:            v.is_ltv_enabled,
            status,
            errors:                    v.errors,
        });
    }

    Ok(results)
}

/// Full-featured validation using `SignatureValidator`.
/// Returns the complete `ValidationResult` with modification detection,
/// attack defences, LTV details, and certificate trust info.
pub fn validate_pdf_full(pdf_bytes: &[u8])
    -> Result<Vec<validator::ValidationResult>, PdfError>
{
    validator::SignatureValidator::validate(pdf_bytes)
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

    let hex_start = ct_off + b"/Contents ".len();
    let closing = buf[hex_start..]
        .iter()
        .position(|&b| b == b'>')
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "could not find closing '>' for /Contents placeholder".into(),
        })?;
    let hex_field_len = closing + 1;
    Ok((br_off, hex_start, hex_field_len))
}

/// Like `find_sig_placeholders` but returns the **last** occurrence of each
/// placeholder.  Used by `append_document_timestamp` so it patches the new
/// DocTimestamp field rather than any earlier sig-dict ByteRange.
fn find_last_sig_placeholders(buf: &[u8]) -> Result<(usize, usize, usize), PdfError> {
    // Last /ByteRange [1000000000 placeholder
    let br_needle = b"/ByteRange [1000000000";
    let br_off = buf.windows(br_needle.len())
        .enumerate()
        .filter(|(_, w)| *w == br_needle)
        .map(|(i, _)| i)
        .last()
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "could not find /ByteRange placeholder for DocTimestamp".into(),
        })?;

    // Last /Contents < placeholder (must come after br_off)
    let ct_needle = b"/Contents <";
    let ct_off = buf.windows(ct_needle.len())
        .enumerate()
        .filter(|(i, w)| *i > br_off && *w == ct_needle)
        .map(|(i, _)| i)
        .last()
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "could not find /Contents placeholder for DocTimestamp".into(),
        })?;

    let hex_start = ct_off + b"/Contents ".len();
    let closing = buf[hex_start..]
        .iter()
        .position(|&b| b == b'>')
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "could not find closing '>' for DocTimestamp /Contents".into(),
        })?;
    Ok((br_off, hex_start, closing + 1))
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
#[allow(dead_code)]
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

#[allow(dead_code)]
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

// ---------------------------------------------------------------------------
// Anchor-tag resolution
// ---------------------------------------------------------------------------

/// Search the text content of `page_id` for `tag` and return the computed
/// signature placement rectangle.
///
/// ## Algorithm
/// 1. Collect all `TextChunk`s from the page content stream(s).
/// 2. Find the first chunk whose `.text` **contains** `tag` (case-sensitive).
/// 3. Derive a rectangle based on `anchor_mode`:
///    - `Overlay`  — rect starts at `(chunk.x, chunk.y - height)` covering the
///      tag position exactly (signature overlays the tag).
///    - `InFront`  — rect starts at `(chunk.x + font_size, chunk.y - height)`
///      so it sits to the right of the tag.
///
/// ## PDF coordinate system reminder
/// PDF Y=0 is at the **bottom** of the page.  `TextChunk.y` is the **baseline**
/// Y of the text.  We place the signature **above** the baseline by `height`
/// so the full rectangle is `[x, y-height, x+width, y]`.
///
/// Returns `Err` if the tag is not found.
pub fn resolve_anchor_rect(
    doc:         &Document,
    page_id:     ObjectId,
    tag:         &str,
    width:       f64,
    height:      f64,
    mode:        &SignatureAnchorMode,
) -> Result<[f64; 4], PdfError> {
    // Collect content stream bytes for this page.
    let page_dict = doc.objects
        .get(&page_id)
        .and_then(|o| o.as_dictionary())
        .ok_or_else(|| PdfError::Parse {
            offset: None,
            context: format!("page object {page_id:?} not found or not a dictionary"),
        })?
        .clone();

    let content_bytes = collect_page_content_bytes(doc, &page_dict);

    // Extract text chunks (requires `text` feature).
    #[cfg(feature = "text")]
    {
        use crate::text::extract_chunks;

        let chunks = extract_chunks(&content_bytes, None);

        // Find first chunk containing the tag text (case-sensitive).
        let hit = chunks.iter().find(|c| c.text.contains(tag));
        match hit {
            Some(chunk) => {
                let x = match mode {
                    SignatureAnchorMode::Overlay  => chunk.x,
                    SignatureAnchorMode::InFront  => chunk.x + chunk.font_size.max(8.0),
                };
                let y_top = chunk.y;          // baseline = top of sig rect
                let y_bot = y_top - height;   // bottom of sig rect
                Ok([x, y_bot, x + width, y_top])
            }
            None => Err(PdfError::Parse {
                offset: None,
                context: format!(
                    "anchor tag {:?} not found on page; available text chunks: [{}]",
                    tag,
                    chunks.iter().map(|c| format!("{:?}", c.text)).collect::<Vec<_>>().join(", ")
                ),
            }),
        }
    }

    #[cfg(not(feature = "text"))]
    Err(PdfError::Unsupported {
        feature: "anchor_tag requires the 'text' feature to be enabled",
    })
}

/// Collect raw (decoded) content stream bytes for a page dictionary.
///
/// Handles both single-stream (`/Contents ref`) and multi-stream
/// (`/Contents [ref ref …]`) pages.
fn collect_page_content_bytes(doc: &Document, page_dict: &CosDictionary) -> Vec<u8> {
    use crate::io::decode_stream;

    let contents_obj = match page_dict.get(&CosName::new(b"Contents")) {
        Some(o) => o.clone(),
        None    => return vec![],
    };

    let decode = |obj: &CosObject| -> Vec<u8> {
        if let Some(stream) = obj.as_stream() {
            let filter = stream.dictionary.get(&CosName::new(b"Filter"));
            decode_stream(&stream.data, filter).unwrap_or_else(|_| stream.data.clone())
        } else if let Some(ref_id) = obj.as_reference() {
            let inner = match doc.objects.get(&ref_id) {
                Some(o) => o,
                None    => return vec![],
            };
            if let Some(stream) = inner.as_stream() {
                let filter = stream.dictionary.get(&CosName::new(b"Filter"));
                decode_stream(&stream.data, filter).unwrap_or_else(|_| stream.data.clone())
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    };

    match &contents_obj {
        CosObject::Array(arr) => {
            let mut out = Vec::new();
            for item in arr {
                out.extend_from_slice(&decode(item));
                out.push(b' '); // PDF spec: streams in array are concatenated
            }
            out
        }
        other => decode(other),
    }
}

