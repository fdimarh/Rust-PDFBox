//! Complete PDF signature validator — based on `rust_pdf_signing::signature_validator`.
//!
//! Verifies every digital signature in a PDF including:
//! * CMS digest + cryptographic signature integrity
//! * RFC 3161 DocTimestamp handling
//! * Certificate chain ordering, expiry, and trust
//! * LTV: DSS dictionary, CRL/OCSP in CMS, embedded timestamp
//! * Modification detection (incremental revision analysis)
//! * pdf-insecurity.org attack defences (USF, SWA, ISA, EAA, Shadow, Certification)

use crate::cos::{CosName, CosObject, CosDictionary, ObjectId};
use crate::{Document, PdfError};
use chrono::{DateTime, Utc, TimeZone};
use cryptographic_message_syntax::SignedData;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Detailed validation result for one digital signature field.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    // ── identity ─────────────────────────────────────────────────────────────
    pub field_name:            Option<String>,
    pub is_document_timestamp: bool,

    // ── signer metadata ───────────────────────────────────────────────────────
    pub signer_name:  Option<String>,
    pub contact_info: Option<String>,
    pub reason:       Option<String>,
    pub signing_time: Option<String>,
    pub filter:       Option<String>,
    pub sub_filter:   Option<String>,

    // ── byte-range ────────────────────────────────────────────────────────────
    pub byte_range:                   Vec<i64>,
    pub byte_range_covers_whole_file: bool,
    pub is_encrypted:                 bool,

    // ── cryptographic checks ──────────────────────────────────────────────────
    pub computed_digest:     Vec<u8>,
    pub digest_match:        bool,
    pub cms_signature_valid: bool,

    // ── certificate info ──────────────────────────────────────────────────────
    pub certificates:              Vec<CertInfo>,
    pub certificate_chain_valid:   bool,
    pub certificate_chain_trusted: bool,
    pub chain_warnings:            Vec<String>,

    // ── LTV ───────────────────────────────────────────────────────────────────
    pub has_dss:                 bool,
    pub dss_crl_count:           usize,
    pub dss_ocsp_count:          usize,
    pub dss_cert_count:          usize,
    pub has_vri:                 bool,
    pub has_cms_revocation_data: bool,
    pub has_timestamp:           bool,
    pub is_ltv_enabled:          bool,

    // ── modification detection ────────────────────────────────────────────────
    pub signature_revision_end:        usize,
    pub no_unauthorized_modifications: bool,
    pub modification_notes:            Vec<String>,

    // ── security attack defences ──────────────────────────────────────────────
    pub byte_range_valid:            bool,
    pub signature_not_wrapped:       bool,
    pub certification_level:         Option<u8>,
    pub certification_permission_ok: bool,
    pub security_warnings:           Vec<String>,

    // ── aggregate ─────────────────────────────────────────────────────────────
    pub errors: Vec<String>,
}

impl ValidationResult {
    /// `true` only when every individual check passed.
    pub fn is_valid(&self) -> bool {
        self.digest_match
            && self.cms_signature_valid
            && self.certificate_chain_valid
            && self.no_unauthorized_modifications
            && self.byte_range_valid
            && self.signature_not_wrapped
            && self.certification_permission_ok
            && self.errors.is_empty()
    }
}

/// Basic certificate metadata extracted from the CMS `SignedData`.
#[derive(Debug, Clone)]
pub struct CertInfo {
    pub subject:        String,
    pub issuer:         String,
    pub serial_number:  String,
    pub not_before:     Option<DateTime<Utc>>,
    pub not_after:      Option<DateTime<Utc>>,
    pub is_expired:     bool,
    pub is_self_signed: bool,
}

// ---------------------------------------------------------------------------
// SignatureValidator — public entry point
// ---------------------------------------------------------------------------

pub struct SignatureValidator;

impl SignatureValidator {
    /// Validate every digital signature found in `pdf_bytes`.
    pub fn validate(pdf_bytes: &[u8]) -> Result<Vec<ValidationResult>, PdfError> {
        let doc = Document::load_from_bytes(pdf_bytes)?;
        let fields = collect_sig_fields(&doc);
        if fields.is_empty() {
            return Err(PdfError::Parse {
                offset: None,
                context: "No digital signature fields found in the PDF".into(),
            });
        }

        let mut results: Vec<ValidationResult> = fields
            .into_iter()
            .map(|f| validate_one(pdf_bytes, &doc, f))
            .collect::<Result<_, _>>()?;

        // Revision boundary for each sig (used by modification detection)
        let eof_offsets = find_eof_offsets(pdf_bytes);
        for r in &mut results {
            if r.byte_range.len() == 4 {
                let sig_end = (r.byte_range[2] + r.byte_range[3]) as usize;
                r.signature_revision_end = eof_offsets
                    .iter()
                    .find(|&&e| e >= sig_end)
                    .copied()
                    .unwrap_or(pdf_bytes.len());
            }
        }

        // Modification detection
        detect_modifications(pdf_bytes, &mut results)?;

        // Security attack defences
        for r in &mut results {
            let (bv, bw) = usf_check(pdf_bytes, &r.byte_range);
            r.byte_range_valid = bv;
            r.security_warnings.extend(bw);
            if !bv { r.errors.push("ByteRange structure invalid (possible USF attack)".into()); }

            let (nw, sw) = swa_check(pdf_bytes, &r.byte_range);
            r.signature_not_wrapped = nw;
            r.security_warnings.extend(sw);
            if !nw { r.errors.push("Signature Contents may be relocated (possible SWA attack)".into()); }

            let (cl, co, cw) = certification_check(&doc, r);
            r.certification_level = cl;
            r.certification_permission_ok = co;
            r.security_warnings.extend(cw);
            if !co { r.errors.push("Document certification permissions violated".into()); }
        }

        // ByteRange-covers-whole-file: only flag last sig if it doesn't AND has unauthorized mods
        let last = results.len().saturating_sub(1);
        if !results.is_empty()
            && !results[last].byte_range_covers_whole_file
            && !results[last].no_unauthorized_modifications
        {
            results[last].errors.push("ByteRange does not cover the entire file".into());
        }

        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Field discovery
// ---------------------------------------------------------------------------

struct SigField {
    field_name:  Option<String>,
    sig_dict_id: ObjectId,
    is_doc_ts:   bool,
}

fn collect_sig_fields(doc: &Document) -> Vec<SigField> {
    let mut out = Vec::new();
    let catalog = match doc.catalog() { Some(c) => c.clone(), None => return out };

    let acroform = match catalog.get(&CosName::new(b"AcroForm")) {
        Some(CosObject::Reference(r)) => {
            match doc.objects.get(r).and_then(|o| o.as_dictionary()) {
                Some(d) => d.clone(), None => return out,
            }
        }
        Some(CosObject::Dictionary(d)) => d.clone(),
        _ => return out,
    };

    let fields = match acroform.get_array(&CosName::new(b"Fields")) {
        Some(a) => a.to_vec(), None => return out,
    };

    let mut queue = fields;
    while let Some(item) = queue.pop() {
        let fid = match item.as_reference() { Some(r) => r, None => continue };
        let fd = match doc.objects.get(&fid).and_then(|o| o.as_dictionary()) {
            Some(d) => d.clone(), None => continue,
        };
        if let Some(kids) = fd.get_array(&CosName::new(b"Kids")) {
            for k in kids.to_vec() { queue.push(k); }
        }
        let is_sig = fd.get_name(&CosName::new(b"FT"))
            .map(|n| n.as_bytes() == b"Sig").unwrap_or(false);
        if !is_sig { continue; }

        let (sig_id, is_ts) = match fd.get(&CosName::new(b"V")).and_then(|v| v.as_reference()) {
            Some(vr) => {
                let is_ts = doc.objects.get(&vr).and_then(|o| o.as_dictionary())
                    .map(|vd| {
                        vd.get_name(&CosName::new(b"Type")).map(|n| n.as_bytes() == b"DocTimeStamp").unwrap_or(false)
                        || vd.get_name(&CosName::new(b"SubFilter")).map(|n| n.as_bytes() == b"ETSI.RFC3161").unwrap_or(false)
                    }).unwrap_or(false);
                (vr, is_ts)
            }
            None => {
                let has = fd.get(&CosName::new(b"ByteRange")).is_some()
                    && fd.get(&CosName::new(b"Contents")).is_some();
                if !has { continue; }
                let sf_ts = fd.get_name(&CosName::new(b"SubFilter"))
                    .map(|n| n.as_bytes() == b"ETSI.RFC3161").unwrap_or(false);
                (fid, sf_ts)
            }
        };

        let name = fd.get(&CosName::new(b"T"))
            .and_then(|v| v.as_string())
            .map(|b| String::from_utf8_lossy(b).into_owned());

        out.push(SigField { field_name: name, sig_dict_id: sig_id, is_doc_ts: is_ts });
    }
    out
}

// ---------------------------------------------------------------------------
// Per-signature validation
// ---------------------------------------------------------------------------

fn validate_one(pdf_bytes: &[u8], doc: &Document, field: SigField) -> Result<ValidationResult, PdfError> {
    let mut errors: Vec<String> = Vec::new();

    let sd = match doc.objects.get(&field.sig_dict_id).and_then(|o| o.as_dictionary()) {
        Some(d) => d.clone(),
        None => return Err(PdfError::Parse { offset: None, context: "sig dict missing".into() }),
    };

    let filter       = dict_name_str(&sd, b"Filter");
    let sub_filter   = dict_name_str(&sd, b"SubFilter");
    let signer_name  = dict_string(&sd, b"Name");
    let contact_info = dict_string(&sd, b"ContactInfo");
    let reason       = dict_string(&sd, b"Reason");
    let signing_time = dict_string(&sd, b"M");

    let is_ts = field.is_doc_ts || sub_filter.as_deref() == Some("ETSI.RFC3161");

    let byte_range: Vec<i64> = sd.get_array(&CosName::new(b"ByteRange"))
        .map(|a| a.iter().filter_map(|x| x.as_integer()).collect())
        .unwrap_or_default();
    if byte_range.len() != 4 {
        errors.push(format!("ByteRange has {} elements (expected 4)", byte_range.len()));
    }
    let covers_file = byte_range.len() == 4
        && (byte_range[2] + byte_range[3]) as usize == pdf_bytes.len();

    let cms_bytes: Vec<u8> = sd.get(&CosName::new(b"Contents"))
        .and_then(|v| v.as_string()).map(|b| b.to_vec()).unwrap_or_default();
    let all_zeros = cms_bytes.iter().all(|&b| b == 0);
    if cms_bytes.is_empty() { errors.push("Contents entry missing".into()); }
    else if all_zeros { errors.push("Contents is all zeros".into()); }

    let computed_digest: Vec<u8> = if byte_range.len() == 4 && !cms_bytes.is_empty() && !all_zeros {
        let mut h = Sha256::new();
        let s0 = byte_range[0] as usize;
        let e0 = (byte_range[0] + byte_range[1]) as usize;
        let s1 = byte_range[2] as usize;
        let e1 = (byte_range[2] + byte_range[3]) as usize;
        if e0 <= pdf_bytes.len() { h.update(&pdf_bytes[s0..e0]); }
        if e1 <= pdf_bytes.len() { h.update(&pdf_bytes[s1..e1]); }
        h.finalize().to_vec()
    } else { vec![] };

    let mut digest_match = false;
    let mut cms_sig_valid = false;
    let mut certs: Vec<CertInfo> = Vec::new();
    let mut chain_valid   = false;
    let mut chain_trusted = false;
    let mut chain_warnings = Vec::new();

    if !cms_bytes.is_empty() && !all_zeros {
        let trimmed = trim_zeros(&cms_bytes);
        let parse_res = if is_ts {
            SignedData::parse_ber(trimmed)
                .or_else(|_| match extract_inner_signed_data(trimmed) {
                    Some(inner) => SignedData::parse_ber(&inner),
                    None        => SignedData::parse_ber(trimmed),
                })
        } else {
            SignedData::parse_ber(&cms_bytes)
        };

        match parse_res {
            Ok(sd_cms) => {
                certs = extract_certs(&sd_cms);
                let now = Utc::now();
                let any_expired = certs.iter().any(|c| c.is_expired);
                if any_expired { errors.push("One or more certificates have expired".into()); }

                let signers: Vec<_> = sd_cms.signers().collect();
                if signers.is_empty() {
                    errors.push("CMS SignedData contains no signers".into());
                } else {
                    for signer in &signers {
                        match signer.verify_signature_with_signed_data(&sd_cms) {
                            Ok(())  => cms_sig_valid = true,
                            Err(e)  => errors.push(format!("CMS signer verification failed: {e}")),
                        }
                        if !computed_digest.is_empty() {
                            if is_ts {
                                match extract_ts_imprint(trimmed) {
                                    Some(h) if h == computed_digest => digest_match = true,
                                    Some(_) => errors.push("Timestamp messageImprint mismatch".into()),
                                    None => {
                                        if cms_sig_valid { digest_match = true; }
                                        else { errors.push("Could not extract messageImprint".into()); }
                                    }
                                }
                            } else {
                                match extract_message_digest(&cms_bytes) {
                                    Some(md) if md == computed_digest => digest_match = true,
                                    Some(_) => errors.push("CMS messageDigest mismatch".into()),
                                    None => {
                                        if cms_sig_valid { digest_match = true; }
                                        else { errors.push("Could not extract messageDigest".into()); }
                                    }
                                }
                            }
                        }
                    }
                }
                let (cv, ct, cw) = check_chain(&certs, &now);
                chain_valid = cv; chain_trusted = ct; chain_warnings = cw;
                if !cv && !any_expired { errors.push("Certificate chain validation failed".into()); }
            }
            Err(e) => errors.push(format!("Failed to parse CMS SignedData: {e}")),
        }
    }

    let (has_dss, dss_crls, dss_ocsps, dss_certs, has_vri) = check_dss(doc, &cms_bytes);
    // OID 1.2.840.113583.1.1.8 — adbe-revocationInfoArchival
    let has_revoc_in_cms = oid_present(&cms_bytes,
        &[0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x2f, 0x01, 0x01, 0x08]);
    // OID 1.2.840.113549.1.9.16.2.14 — id-smime-aa-signatureTimeStampToken
    let has_ts_in_cms = oid_present(&cms_bytes,
        &[0x06, 0x0b, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x10, 0x02, 0x0e]);
    let has_timestamp = has_ts_in_cms || (is_ts && cms_sig_valid);
    let has_revoc = (has_dss && (dss_crls > 0 || dss_ocsps > 0)) || has_revoc_in_cms;
    let is_ltv = has_revoc && has_timestamp;

    Ok(ValidationResult {
        field_name: field.field_name,
        is_document_timestamp: is_ts,
        signer_name, contact_info, reason, signing_time, filter, sub_filter,
        byte_range, byte_range_covers_whole_file: covers_file, is_encrypted: false,
        computed_digest, digest_match, cms_signature_valid: cms_sig_valid,
        certificates: certs,
        certificate_chain_valid: chain_valid,
        certificate_chain_trusted: chain_trusted,
        chain_warnings,
        has_dss, dss_crl_count: dss_crls, dss_ocsp_count: dss_ocsps, dss_cert_count: dss_certs,
        has_vri,
        has_cms_revocation_data: has_revoc_in_cms,
        has_timestamp, is_ltv_enabled: is_ltv,
        signature_revision_end: 0,
        no_unauthorized_modifications: true,
        modification_notes: Vec::new(),
        byte_range_valid: true,
        signature_not_wrapped: true,
        certification_level: None,
        certification_permission_ok: true,
        security_warnings: Vec::new(),
        errors,
    })
}

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

fn dict_string(d: &CosDictionary, key: &[u8]) -> Option<String> {
    d.get(&CosName::new(key))?.as_string()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

fn dict_name_str(d: &CosDictionary, key: &[u8]) -> Option<String> {
    d.get_name(&CosName::new(key))
        .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned())
}

// ---------------------------------------------------------------------------
// DER helpers
// ---------------------------------------------------------------------------

fn read_der_len(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    if offset >= data.len() { return None; }
    let f = data[offset] as usize;
    if f < 0x80 { return Some((offset + 1, f)); }
    if f == 0x81 {
        if offset + 1 >= data.len() { return None; }
        return Some((offset + 2, data[offset + 1] as usize));
    }
    if f == 0x82 {
        if offset + 2 >= data.len() { return None; }
        let l = ((data[offset + 1] as usize) << 8) | data[offset + 2] as usize;
        return Some((offset + 3, l));
    }
    if f == 0x83 {
        if offset + 3 >= data.len() { return None; }
        let l = ((data[offset + 1] as usize) << 16)
            | ((data[offset + 2] as usize) << 8)
            | data[offset + 3] as usize;
        return Some((offset + 4, l));
    }
    None
}

fn trim_zeros(data: &[u8]) -> &[u8] {
    if data.len() < 2 || data[0] != 0x30 { return data; }
    if let Some((s, l)) = read_der_len(data, 1) {
        let t = s + l;
        if t <= data.len() { return &data[..t]; }
    }
    data
}

fn extract_inner_signed_data(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data[0] != 0x30 { return None; }
    let (outer, _) = read_der_len(data, 1)?;
    let pos = outer;
    if pos >= data.len() || data[pos] != 0x06 { return None; }
    let (oid_s, oid_l) = read_der_len(data, pos + 1)?;
    let after = oid_s + oid_l;
    if after >= data.len() || data[after] != 0xA0 { return None; }
    let (explicit, _) = read_der_len(data, after + 1)?;
    if explicit < data.len() && data[explicit] == 0x30 {
        Some(data[explicit..].to_vec())
    } else {
        None
    }
}

fn extract_message_digest(cms: &[u8]) -> Option<Vec<u8>> {
    // OID 1.2.840.113549.1.9.4
    let oid: &[u8] = &[0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x04];
    for i in 0..cms.len().saturating_sub(oid.len()) {
        if &cms[i..i + oid.len()] != oid { continue; }
        let after = i + oid.len();
        if after >= cms.len() || cms[after] != 0x31 { continue; }
        let (ss, _) = read_der_len(cms, after + 1)?;
        if ss >= cms.len() || cms[ss] != 0x04 { continue; }
        let (os, ol) = read_der_len(cms, ss + 1)?;
        if ol == 32 && os + 32 <= cms.len() {
            return Some(cms[os..os + 32].to_vec());
        }
    }
    None
}

fn extract_ts_imprint(data: &[u8]) -> Option<Vec<u8>> {
    // SHA-256 OID: 2.16.840.1.101.3.4.2.1
    let sha256: &[u8] = &[0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
    for i in 0..data.len().saturating_sub(sha256.len()) {
        if &data[i..i + sha256.len()] != sha256 { continue; }
        let mut pos = i + sha256.len();
        // skip NULL AlgorithmIdentifier parameter if present
        if pos + 1 < data.len() && data[pos] == 0x05 && data[pos + 1] == 0x00 { pos += 2; }
        for off in 0..10 {
            let c = pos + off;
            if c >= data.len() { break; }
            if data[c] == 0x04 {
                if let Some((cs, cl)) = read_der_len(data, c + 1) {
                    if cl == 32 && cs + 32 <= data.len() {
                        return Some(data[cs..cs + 32].to_vec());
                    }
                }
            }
        }
    }
    None
}

fn oid_present(cms: &[u8], oid: &[u8]) -> bool {
    cms.windows(oid.len()).any(|w| w == oid)
}

// ---------------------------------------------------------------------------
// Certificate chain
// ---------------------------------------------------------------------------

fn extract_certs(sd: &SignedData) -> Vec<CertInfo> {
    let now = Utc::now();
    sd.certificates().filter_map(|cr| {
        let der = cr.encode_ber().ok()?;
        let (_, p) = x509_parser::parse_x509_certificate(&der).ok()?;
        let subject = p.subject().to_string();
        let issuer  = p.issuer().to_string();
        let serial  = p.serial.to_str_radix(16);
        let v = p.validity();
        let nb = asn1_time(&v.not_before);
        let na = asn1_time(&v.not_after);
        let exp = na.map_or(false, |t| now > t);
        Some(CertInfo {
            subject: subject.clone(), issuer: issuer.clone(),
            serial_number: serial, not_before: nb, not_after: na,
            is_expired: exp, is_self_signed: subject == issuer,
        })
    }).collect()
}

fn asn1_time(t: &x509_parser::time::ASN1Time) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(t.timestamp(), 0).single()
}

fn order_chain(certs: &[CertInfo]) -> Vec<CertInfo> {
    if certs.len() <= 1 { return certs.to_vec(); }
    let issuer_set: HashSet<&str> = certs.iter()
        .filter(|c| !c.is_self_signed).map(|c| c.issuer.as_str()).collect();
    let ee = certs.iter()
        .find(|c| !c.is_self_signed && !issuer_set.contains(c.subject.as_str()))
        .or_else(|| certs.iter().find(|c| !c.is_self_signed));
    let ee = match ee { Some(e) => e, None => return certs.to_vec() };
    let mut chain = vec![ee.clone()];
    let mut cur = &ee.issuer;
    let mut used: HashSet<usize> = [certs.iter().position(|c| std::ptr::eq(c, ee)).unwrap_or(0)].into();
    for _ in 0..certs.len() {
        match certs.iter().enumerate().find(|(i, c)| !used.contains(i) && c.subject == *cur) {
            Some((idx, next)) => {
                chain.push(next.clone()); used.insert(idx);
                if next.is_self_signed { break; }
                cur = &next.issuer;
            }
            None => break,
        }
    }
    chain
}

fn check_chain(certs: &[CertInfo], _now: &DateTime<Utc>) -> (bool, bool, Vec<String>) {
    let mut w = Vec::new();
    if certs.is_empty() {
        return (false, false, vec!["No certificates in signature".into()]);
    }
    let chain = { let o = order_chain(certs); if o.len() == certs.len() { o } else { certs.to_vec() } };
    let mut valid = true;
    if chain.len() > 1 {
        for i in 0..chain.len() - 1 {
            if chain[i].issuer != chain[i + 1].subject {
                valid = false;
                w.push(format!("Chain break: cert[{i}] issuer '{}' ≠ cert[{}] subject '{}'",
                    chain[i].issuer, i + 1, chain[i + 1].subject));
            }
        }
    }
    for (i, c) in chain.iter().enumerate() {
        if c.is_expired { valid = false; w.push(format!("Cert [{i}] '{}' expired", c.subject)); }
    }
    let root = &chain[chain.len() - 1];
    let signer = &chain[0];
    let trusted = if root.is_self_signed {
        if chain.len() == 1 {
            w.push(format!("Self-signed: '{}' — not from trusted CA", signer.subject));
            false
        } else {
            let t = known_root(&root.subject);
            if !t { w.push(format!("Root '{}' self-signed, not recognized as public CA", root.subject)); }
            t
        }
    } else {
        let t = known_root(&root.issuer);
        if !t { w.push(format!("Root issuer '{}' not recognized as trusted CA", root.issuer)); }
        t
    };
    (valid, trusted, w)
}

fn known_root(dn: &str) -> bool {
    let d = dn.to_lowercase();
    ["digicert","globalsign","isrg root","let's encrypt","comodo","sectigo","usertrust",
     "entrust","geotrust","thawte","verisign","symantec","baltimore","cybertrust","quovadis",
     "buypass","swisssign","certum","identrust","dst root","amazon root","starfield",
     "microsoft root","apple root","google trust","gts root","actalis","harica",
     "t-telesec","deutsche telekom","certigna","camerfirma"]
    .iter().any(|r| d.contains(r))
}

// ---------------------------------------------------------------------------
// LTV: DSS dictionary
// ---------------------------------------------------------------------------

fn check_dss(doc: &Document, cms: &[u8]) -> (bool, usize, usize, usize, bool) {
    let cat = match doc.catalog() { Some(c) => c.clone(), None => return (false,0,0,0,false) };
    let dss = match cat.get(&CosName::new(b"DSS")) {
        Some(CosObject::Dictionary(d)) => d.clone(),
        Some(CosObject::Reference(r)) => match doc.objects.get(r).and_then(|o| o.as_dictionary()) {
            Some(d) => d.clone(), None => return (false,0,0,0,false),
        },
        _ => return (false,0,0,0,false),
    };
    let crls  = dss.get_array(&CosName::new(b"CRLs")).map(|a| a.len()).unwrap_or(0);
    let ocsps = dss.get_array(&CosName::new(b"OCSPs")).map(|a| a.len()).unwrap_or(0);
    let certs = dss.get_array(&CosName::new(b"Certs")).map(|a| a.len()).unwrap_or(0);
    let vri = if !cms.is_empty() && dss.get(&CosName::new(b"VRI")).is_some() {
        use sha1::{Sha1, Digest as _};
        let key: String = Sha1::digest(cms).iter().map(|b| format!("{b:02X}")).collect();
        match dss.get(&CosName::new(b"VRI")) {
            Some(CosObject::Dictionary(vd)) => vd.get(&CosName::new(key.as_bytes())).is_some(),
            Some(CosObject::Reference(r)) => doc.objects.get(r)
                .and_then(|o| o.as_dictionary())
                .map(|d| d.get(&CosName::new(key.as_bytes())).is_some())
                .unwrap_or(false),
            _ => false,
        }
    } else { false };
    (true, crls, ocsps, certs, vri)
}

// ---------------------------------------------------------------------------
// Modification detection
// ---------------------------------------------------------------------------

fn find_eof_offsets(pdf: &[u8]) -> Vec<usize> {
    let m = b"%%EOF";
    let mut offs = Vec::new();
    let mut pos = 0usize;
    while pos + m.len() <= pdf.len() {
        if let Some(f) = pdf[pos..].windows(m.len()).position(|w| w == m) {
            let abs = pos + f;
            let mut end = abs + m.len();
            if end < pdf.len() && pdf[end] == b'\r' { end += 1; }
            if end < pdf.len() && pdf[end] == b'\n' { end += 1; }
            offs.push(end);
            pos = end;
        } else { break; }
    }
    offs
}

fn detect_modifications(pdf: &[u8], results: &mut Vec<ValidationResult>) -> Result<(), PdfError> {
    let n = results.len();
    if n == 0 { return Ok(()); }
    let last = n - 1;
    if results[last].byte_range_covers_whole_file {
        results[last].no_unauthorized_modifications = true;
    }
    for i in 0..n {
        if i == last && results[last].byte_range_covers_whole_file { continue; }
        let rev_end = results[i].signature_revision_end;
        if rev_end == 0 || rev_end >= pdf.len() {
            results[i].no_unauthorized_modifications = true;
            continue;
        }
        let rev_doc = match Document::load_from_bytes(&pdf[..rev_end]) {
            Ok(d) => d,
            Err(_) => {
                results[i].no_unauthorized_modifications = true;
                results[i].modification_notes.push("Could not parse revision for modification check".into());
                continue;
            }
        };
        let full_doc = match Document::load_from_bytes(pdf) {
            Ok(d) => d,
            Err(_) => { results[i].no_unauthorized_modifications = true; continue; }
        };
        let (unauth, notes) = compare_revisions(&rev_doc, &full_doc);
        results[i].no_unauthorized_modifications = !unauth;
        results[i].modification_notes = notes;
        if unauth {
            results[i].errors.push("Document modified after this signature was applied".into());
        }
    }
    Ok(())
}

enum Change { Permitted(String), Unauthorized(String) }

fn compare_revisions(rev: &Document, full: &Document) -> (bool, Vec<String>) {
    let mut notes = Vec::new();
    let mut unauth = false;
    let rev_ids:  HashSet<ObjectId> = rev.objects.keys().copied().collect();
    let full_ids: HashSet<ObjectId> = full.objects.keys().copied().collect();

    for &id in full_ids.difference(&rev_ids) {
        match classify_new(full, id) {
            Change::Permitted(d)    => notes.push(format!("Added obj {}: {} (permitted)", id.object_number, d)),
            Change::Unauthorized(d) => {
                notes.push(format!("Added obj {}: {} (UNAUTHORIZED)", id.object_number, d));
                unauth = true;
            }
        }
    }
    for &id in rev_ids.difference(&full_ids) {
        notes.push(format!("Deleted obj {} (UNAUTHORIZED)", id.object_number));
        unauth = true;
    }
    for &id in rev_ids.intersection(&full_ids) {
        let ro = match rev.objects.get(&id)  { Some(o) => o, None => continue };
        let fo = match full.objects.get(&id) { Some(o) => o, None => continue };
        if format!("{ro:?}") == format!("{fo:?}") { continue; }
        match classify_modified(full, id, ro, fo) {
            Change::Permitted(d)    => notes.push(format!("Modified obj {}: {} (permitted)", id.object_number, d)),
            Change::Unauthorized(d) => {
                notes.push(format!("Modified obj {}: {} (UNAUTHORIZED)", id.object_number, d));
                unauth = true;
            }
        }
    }
    (unauth, notes)
}

fn classify_new(doc: &Document, id: ObjectId) -> Change {
    let obj = match doc.objects.get(&id) { Some(o) => o, None => return Change::Unauthorized("unreadable".into()) };
    match obj {
        CosObject::Dictionary(d) => {
            if let Some(tn) = d.get_name(&CosName::new(b"Type")) {
                match tn.as_bytes() {
                    b"Sig" | b"DocTimeStamp" => return Change::Permitted("signature value dict".into()),
                    b"Annot" => {
                        if let Some(sub) = d.get_name(&CosName::new(b"Subtype")) {
                            let s = sub.as_bytes();
                            // EAA: dangerous annotations that can overlay signed content
                            if matches!(s, b"FreeText"|b"Stamp"|b"Redact"|b"Watermark"|b"Square"
                                |b"Circle"|b"Line"|b"Ink"|b"FileAttachment"|b"RichMedia"|b"Screen"
                                |b"3D"|b"Sound"|b"Movie"|b"Polygon"|b"PolyLine"|b"Caret"
                                |b"Highlight"|b"Underline"|b"Squiggly"|b"StrikeOut"|b"Text"|b"Popup")
                            {
                                return Change::Unauthorized(format!("[EAA] dangerous annotation /{}",
                                    String::from_utf8_lossy(s)));
                            }
                            if s == b"Widget" || s == b"Link" {
                                return Change::Permitted("widget/link annotation".into());
                            }
                        }
                    }
                    b"Catalog" => return Change::Permitted("catalog update".into()),
                    _ => {}
                }
            }
            if d.get_name(&CosName::new(b"FT")).map(|n| n.as_bytes() == b"Sig").unwrap_or(false) {
                return Change::Permitted("signature field".into());
            }
            if d.get(&CosName::new(b"VRI")).is_some() || d.get(&CosName::new(b"CRLs")).is_some()
                || d.get(&CosName::new(b"OCSPs")).is_some() || d.get(&CosName::new(b"Certs")).is_some()
            {
                return Change::Permitted("DSS dictionary".into());
            }
            if d.get(&CosName::new(b"Cert")).is_some() || d.get(&CosName::new(b"CRL")).is_some()
                || d.get(&CosName::new(b"OCSP")).is_some()
            {
                return Change::Permitted("VRI entry".into());
            }
            if d.get(&CosName::new(b"ByteRange")).is_some() && d.get(&CosName::new(b"Contents")).is_some() {
                return Change::Permitted("signature value dict".into());
            }
            if d.get_array(&CosName::new(b"Fields")).is_some() {
                return Change::Permitted("AcroForm update".into());
            }
            let keys: Vec<_> = d.iter()
                .map(|(k, _)| String::from_utf8_lossy(k.as_bytes()).to_string())
                .collect();
            Change::Unauthorized(format!("dictionary: {:?}", keys))
        }
        CosObject::Stream(s) => {
            let d = &s.dictionary;
            if let Some(tn) = d.get_name(&CosName::new(b"Type")) {
                match tn.as_bytes() {
                    b"XRef"    => return Change::Permitted("XRef stream (incremental update)".into()),
                    b"ObjStm"  => return Change::Unauthorized("[Shadow] object stream added after signing".into()),
                    b"XObject" => {
                        if let Some(sub) = d.get_name(&CosName::new(b"Subtype")) {
                            if sub.as_bytes() == b"Form"  { return Change::Permitted("form XObject (sig appearance)".into()); }
                            if sub.as_bytes() == b"Image" { return Change::Unauthorized("[Shadow] image XObject added after signing".into()); }
                        }
                    }
                    _ => {}
                }
            }
            if d.get(&CosName::new(b"Resources")).is_some() || d.get(&CosName::new(b"BBox")).is_some() {
                return Change::Unauthorized("[Shadow] content stream with Resources/BBox added after signing".into());
            }
            let keys: Vec<_> = d.iter()
                .map(|(k, _)| String::from_utf8_lossy(k.as_bytes()).to_string())
                .collect();
            if keys.iter().all(|k| matches!(k.as_str(), "Length" | "Filter" | "DecodeParms" | "DL")) {
                return Change::Permitted("data stream (DSS/CRL/OCSP)".into());
            }
            Change::Unauthorized(format!("stream: {:?}", keys))
        }
        _ => Change::Unauthorized(format!("object: {obj:?}")),
    }
}

fn classify_modified(full: &Document, id: ObjectId, ro: &CosObject, fo: &CosObject) -> Change {
    if let Some(cat_id) = full.catalog_ref() {
        if cat_id == id { return classify_catalog_change(ro, fo); }
    }
    if let (Some(rd), Some(fd)) = (ro.as_dictionary(), fo.as_dictionary()) {
        if fd.get_name(&CosName::new(b"Type")).map(|n| n.as_bytes() == b"Page").unwrap_or(false) {
            return classify_page_change(rd, fd);
        }
        if fd.get_name(&CosName::new(b"Type")).map(|n| n.as_bytes() == b"Pages").unwrap_or(false) {
            return Change::Unauthorized("[Shadow] /Type /Pages tree modified".into());
        }
        if fd.get(&CosName::new(b"Fields")).is_some() && fd.get(&CosName::new(b"SigFlags")).is_some() {
            return classify_acroform_change(rd, fd);
        }
        // ISA: non-signature field value changed
        if let Some(ft) = fd.get_name(&CosName::new(b"FT")) {
            if ft.as_bytes() != b"Sig" {
                let rv = format!("{:?}", rd.get(&CosName::new(b"V")));
                let fv_s = format!("{:?}", fd.get(&CosName::new(b"V")));
                if rv != fv_s {
                    return Change::Unauthorized(format!("[ISA] field /FT /{} value /V changed",
                        String::from_utf8_lossy(ft.as_bytes())));
                }
            }
        }
    }
    if matches!((ro, fo), (CosObject::Stream(_), CosObject::Stream(_))) {
        return Change::Unauthorized(format!("[ISA] stream obj {} modified", id.object_number));
    }
    Change::Unauthorized(format!("obj {} modified", id.object_number))
}

fn classify_catalog_change(ro: &CosObject, fo: &CosObject) -> Change {
    let (rd, fd) = match (ro.as_dictionary(), fo.as_dictionary()) {
        (Some(r), Some(f)) => (r, f),
        _ => return Change::Unauthorized("catalog not a dictionary".into()),
    };
    let mut changes = Vec::new();
    let mut unauth = false;
    for (k, fv) in fd.iter() {
        let ks = String::from_utf8_lossy(k.as_bytes()).to_string();
        match rd.get(k) {
            Some(rv) if format!("{rv:?}") == format!("{fv:?}") => {}
            Some(_) => match ks.as_str() {
                "AcroForm" | "DSS" | "Perms" => changes.push(format!("/{ks} updated")),
                "OCProperties" | "Pages"     => { changes.push(format!("[Shadow] /{ks} modified")); unauth = true; }
                _                            => { changes.push(format!("/{ks} modified (unauthorized)")); unauth = true; }
            },
            None => match ks.as_str() {
                "DSS" | "Perms" | "AcroForm" => changes.push(format!("/{ks} added")),
                "OCProperties"               => { changes.push("[Shadow] /OCProperties added".into()); unauth = true; }
                _                            => { changes.push(format!("/{ks} added (unauthorized)")); unauth = true; }
            }
        }
    }
    for (k, _) in rd.iter() {
        if fd.get(k).is_none() {
            changes.push(format!("/{} removed (unauthorized)", String::from_utf8_lossy(k.as_bytes())));
            unauth = true;
        }
    }
    let desc = if changes.is_empty() { "catalog (no changes)".into() }
               else { format!("catalog: {}", changes.join(", ")) };
    if unauth { Change::Unauthorized(desc) } else { Change::Permitted(desc) }
}

fn classify_page_change(rd: &CosDictionary, fd: &CosDictionary) -> Change {
    let mut changes = Vec::new();
    let mut unauth = false;
    for (k, fv) in fd.iter() {
        let ks = String::from_utf8_lossy(k.as_bytes()).to_string();
        match rd.get(k) {
            Some(rv) if format!("{rv:?}") == format!("{fv:?}") => {}
            Some(rv) => match ks.as_str() {
                "Annots" => {
                    if array_append_only(rv, fv) { changes.push("/Annots extended".into()); }
                    else { changes.push("[EAA] /Annots modified (not append-only)".into()); unauth = true; }
                }
                "Resources" => changes.push("/Resources updated".into()),
                "Contents"  => { changes.push("[Shadow] /Contents reference changed".into()); unauth = true; }
                "MediaBox" | "CropBox" | "TrimBox" | "BleedBox" | "ArtBox" =>
                    { changes.push(format!("[Shadow] /{ks} modified")); unauth = true; }
                _ => { changes.push(format!("/{ks} modified (unauthorized)")); unauth = true; }
            },
            None => if ks == "Annots" { changes.push("/Annots added".into()); }
                    else { changes.push(format!("/{ks} added (unauthorized)")); unauth = true; }
        }
    }
    for (k, _) in rd.iter() {
        if fd.get(k).is_none() {
            changes.push(format!("/{} removed (unauthorized)", String::from_utf8_lossy(k.as_bytes())));
            unauth = true;
        }
    }
    let desc = if changes.is_empty() { "page (no changes)".into() }
               else { format!("page: {}", changes.join(", ")) };
    if unauth { Change::Unauthorized(desc) } else { Change::Permitted(desc) }
}

fn classify_acroform_change(rd: &CosDictionary, fd: &CosDictionary) -> Change {
    let mut changes = Vec::new();
    let mut unauth = false;
    for (k, fv) in fd.iter() {
        let ks = String::from_utf8_lossy(k.as_bytes()).to_string();
        match rd.get(k) {
            Some(rv) if format!("{rv:?}") == format!("{fv:?}") => {}
            Some(rv) => match ks.as_str() {
                "Fields"   => {
                    if array_append_only(rv, fv) { changes.push("/Fields extended".into()); }
                    else { changes.push("/Fields modified (not append-only)".into()); unauth = true; }
                }
                "SigFlags" => changes.push("/SigFlags updated".into()),
                "DR"       => changes.push("/DR updated".into()),
                _ => { changes.push(format!("/{ks} modified (unauthorized)")); unauth = true; }
            },
            None => match ks.as_str() {
                "Fields" | "SigFlags" | "DR" => changes.push(format!("/{ks} added")),
                _ => { changes.push(format!("/{ks} added (unauthorized)")); unauth = true; }
            }
        }
    }
    let desc = if changes.is_empty() { "AcroForm (no changes)".into() }
               else { format!("AcroForm: {}", changes.join(", ")) };
    if unauth { Change::Unauthorized(desc) } else { Change::Permitted(desc) }
}

fn array_append_only(rv: &CosObject, fv: &CosObject) -> bool {
    let ra = match rv.as_array() { Some(a) => a, None => return false };
    let fa = match fv.as_array() { Some(a) => a, None => return false };
    fa.len() >= ra.len()
        && ra.iter().enumerate().all(|(i, r)| format!("{r:?}") == format!("{:?}", fa[i]))
}

// ---------------------------------------------------------------------------
// Security attack defences
// ---------------------------------------------------------------------------

/// USF (Universal Signature Forgery): validate ByteRange structure.
fn usf_check(pdf: &[u8], br: &[i64]) -> (bool, Vec<String>) {
    let mut w = Vec::new();
    let mut ok = true;
    if br.len() != 4 {
        w.push(format!("[USF] ByteRange has {} elements, expected 4", br.len()));
        return (false, w);
    }
    let (o1, l1, o2, l2) = (br[0], br[1], br[2], br[3]);
    let flen = pdf.len() as i64;
    if o1 != 0 { w.push(format!("[USF] ByteRange starts at {o1} not 0")); ok = false; }
    if o1 < 0 || l1 < 0 || o2 < 0 || l2 < 0 { w.push("[USF] Negative ByteRange values".into()); ok = false; }
    if o1 + l1 > flen { w.push("[USF] First range exceeds file".into()); ok = false; }
    if o2 + l2 > flen { w.push("[USF] Second range exceeds file".into()); ok = false; }
    let end0 = o1 + l1;
    if end0 > o2 { w.push(format!("[USF] Ranges overlap (end0={end0} > o2={o2})")); ok = false; }
    if end0 < o2 && end0 >= 0 && o2 <= flen {
        let (gs, ge) = (end0 as usize, o2 as usize);
        let gap = &pdf[gs..ge];
        if gap.is_empty() { w.push("[USF] Zero-length gap".into()); ok = false; }
        else {
            if gap[0] != b'<' { w.push("[USF] Gap doesn't start with '<'".into()); ok = false; }
            if gap[gap.len() - 1] != b'>' { w.push("[USF] Gap doesn't end with '>'".into()); ok = false; }
            if gap.len() >= 2 && gap[1..gap.len()-1].iter().any(|b| !b.is_ascii_hexdigit()) {
                w.push("[USF] Gap contains non-hex chars".into()); ok = false;
            }
        }
    }
    if l1 < 50 { w.push(format!("[USF] First segment suspiciously short ({l1} bytes)")); ok = false; }
    (ok, w)
}

/// SWA (Signature Wrapping Attack): verify /Contents location.
fn swa_check(pdf: &[u8], br: &[i64]) -> (bool, Vec<String>) {
    let mut w = Vec::new();
    if br.len() != 4 { return (true, w); }
    let gs = br[0] as usize + br[1] as usize;
    let ge = br[2] as usize;
    if gs >= pdf.len() || ge > pdf.len() || gs >= ge { return (true, w); }
    let search = gs.saturating_sub(20);
    if !pdf[search..gs].windows(9).any(|ww| ww == b"/Contents") {
        w.push("[SWA] /Contents not found immediately before ByteRange gap".into());
        return (false, w);
    }
    // Count large /Contents< hex strings (more than 10 is suspicious)
    let pat = b"/Contents<";
    let mut count = 0usize;
    let mut pos = 0usize;
    while pos + pat.len() < pdf.len() {
        if let Some(f) = pdf[pos..].windows(pat.len()).position(|ww| ww == pat) {
            let abs = pos + f;
            let hs = abs + pat.len();
            if let Some(cl) = pdf[hs..].iter().position(|&b| b == b'>') {
                if cl + 2 >= 200 { count += 1; }
                pos = hs + cl + 1;
            } else { pos = hs + 1; }
        } else { break; }
    }
    if count > 10 {
        w.push(format!("[SWA] Found {count} large /Contents hex-strings (possible wrapping)"));
        return (false, w);
    }
    (true, w)
}

/// Certification Attack: MDP permission enforcement.
fn certification_check(doc: &Document, result: &ValidationResult) -> (Option<u8>, bool, Vec<String>) {
    let mut w = Vec::new();
    let cat = match doc.catalog() { Some(c) => c.clone(), None => return (None, true, w) };
    let perms_obj = match cat.get(&CosName::new(b"Perms")) { Some(v) => v.clone(), None => return (None, true, w) };
    let perms = match perms_obj {
        CosObject::Dictionary(d) => d,
        CosObject::Reference(r) => match doc.objects.get(&r).and_then(|o| o.as_dictionary()) {
            Some(d) => d.clone(), None => return (None, true, w),
        },
        _ => return (None, true, w),
    };
    let docmdp_ref = match perms.get(&CosName::new(b"DocMDP")).and_then(|v| v.as_reference()) {
        Some(r) => r, None => return (None, true, w),
    };
    let sig_dict = match doc.objects.get(&docmdp_ref).and_then(|o| o.as_dictionary()) {
        Some(d) => d.clone(), None => return (None, true, w),
    };
    if let Some(level) = extract_mdp(doc, &sig_dict) {
        w.push(format!("[Certification] MDP level {level} — {}", match level {
            1 => "no changes", 2 => "form fill-in + signing", 3 => "form + signing + annots", _ => "unknown",
        }));
        let ok = mdp_compliant(level, result);
        if !ok { w.push(format!("[Certification] Changes VIOLATE MDP level {level}")); }
        return (Some(level), ok, w);
    }
    (None, true, w)
}

fn extract_mdp(doc: &Document, sig: &CosDictionary) -> Option<u8> {
    let refs = sig.get_array(&CosName::new(b"Reference"))?.to_vec();
    for item in refs {
        let rd = match item {
            CosObject::Dictionary(d) => d,
            CosObject::Reference(r) => match doc.objects.get(&r).and_then(|o| o.as_dictionary()) {
                Some(d) => d.clone(), None => continue,
            },
            _ => continue,
        };
        if !rd.get_name(&CosName::new(b"TransformMethod")).map(|n| n.as_bytes() == b"DocMDP").unwrap_or(false) {
            continue;
        }
        let tp_obj = match rd.get(&CosName::new(b"TransformParams")) { Some(v) => v.clone(), None => continue };
        let tp = match tp_obj {
            CosObject::Dictionary(d) => d,
            CosObject::Reference(r) => match doc.objects.get(&r).and_then(|o| o.as_dictionary()) {
                Some(d) => d.clone(), None => continue,
            },
            _ => continue,
        };
        if let Some(p) = tp.get_int(&CosName::new(b"P")) { return Some(p as u8); }
    }
    None
}

fn mdp_compliant(level: u8, r: &ValidationResult) -> bool {
    if r.modification_notes.is_empty() { return true; }
    let allow = |n: &str| {
        let l = n.to_lowercase();
        l.contains("signature") || l.contains("acroform") || l.contains("dss")
            || l.contains("catalog") || l.contains("data stream") || l.contains("vri")
            || l.contains("permitted")
    };
    match level {
        1 => r.modification_notes.is_empty(),
        2 => r.modification_notes.iter().all(|n| allow(n) || n.to_lowercase().contains("annots extended")),
        3 => r.modification_notes.iter().all(|n| allow(n)
            || n.to_lowercase().contains("annot")
            || n.to_lowercase().contains("widget")),
        _ => true,
    }
}

