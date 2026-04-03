//! LTV (Long-Term Validation) helpers — CRL/OCSP fetching, DSS dictionary,
//! `adbe-revocationInfoArchival` CMS attribute, RFC 3161 timestamp token.
//!
//! Ported directly from `rust_pdf_signing/src/ltv.rs` (Ralph Bisschops).
//! All logic mirrors the reference implementation so that DSS dictionaries
//! and revocation data are formatted exactly as Adobe/Foxit expect.

use std::borrow::Cow;
use std::io::Write;

use bcder::encode::Values;
use bcder::{encode::PrimitiveContent, Captured, Integer, Mode, OctetString, Oid, Tag};
use bcder::Mode::Der;
use cryptographic_message_syntax::Bytes;
use x509_certificate::rfc5652::AttributeValue;
use x509_certificate::CapturedX509Certificate;

use crate::PdfError;

// ---------------------------------------------------------------------------
// Internal helpers — DER length encoding/reading
// ---------------------------------------------------------------------------

fn der_push_length(buf: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        buf.push(len as u8);
    } else if len <= 0xff {
        buf.push(0x81);
        buf.push(len as u8);
    } else if len <= 0xffff {
        buf.push(0x82);
        buf.push((len >> 8) as u8);
        buf.push(len as u8);
    } else {
        buf.push(0x83);
        buf.push((len >> 16) as u8);
        buf.push((len >> 8) as u8);
        buf.push(len as u8);
    }
}

fn der_read_length(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    if offset >= data.len() { return None; }
    let first = data[offset] as usize;
    if first < 0x80 {
        Some((offset + 1, first))
    } else if first == 0x81 {
        if offset + 1 >= data.len() { return None; }
        Some((offset + 2, data[offset + 1] as usize))
    } else if first == 0x82 {
        if offset + 2 >= data.len() { return None; }
        let len = ((data[offset + 1] as usize) << 8) | (data[offset + 2] as usize);
        Some((offset + 3, len))
    } else if first == 0x83 {
        if offset + 3 >= data.len() { return None; }
        let len = ((data[offset + 1] as usize) << 16)
            | ((data[offset + 2] as usize) << 8)
            | (data[offset + 3] as usize);
        Some((offset + 4, len))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Raw DER bytes helper (no wrapping)
// ---------------------------------------------------------------------------

struct RawDerBytes(Vec<u8>);

impl Values for RawDerBytes {
    fn encoded_len(&self, _: Mode) -> usize { self.0.len() }
    fn write_encoded<W: Write>(&self, _: Mode, target: &mut W) -> std::io::Result<()> {
        target.write_all(&self.0)
    }
}

// ---------------------------------------------------------------------------
// CRL bytes helper
// ---------------------------------------------------------------------------

struct CrlBytes {
    bytes: Bytes,
}

impl Values for CrlBytes {
    fn encoded_len(&self, _: Mode) -> usize { self.bytes.len() }
    fn write_encoded<W: Write>(&self, _: Mode, target: &mut W) -> std::io::Result<()> {
        target.write_all(&self.bytes)
    }
}

// ---------------------------------------------------------------------------
// URL extraction from cert extensions
// ---------------------------------------------------------------------------

/// Extract (ocsp_url, crl_url) from a certificate's AIA / CRL-DP extensions.
pub fn get_ocsp_crl_url(cert: &CapturedX509Certificate) -> (Option<String>, Option<String>) {
    use x509_parser::prelude::*;
    use x509_parser::extensions::DistributionPointName::FullName;
    use x509_parser::extensions::ParsedExtension::{AuthorityInfoAccess, CRLDistributionPoints};

    let der = match cert.encode_der() {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    let parsed = match X509Certificate::from_der(&der) {
        Ok((_, c)) => c,
        Err(_) => return (None, None),
    };

    let mut ocsp_url = None;
    let mut crl_url  = None;

    for ext in parsed.extensions() {
        match ext.parsed_extension() {
            AuthorityInfoAccess(aia) => {
                for access in &aia.accessdescs {
                    if access.access_method.to_string() == "1.3.6.1.5.5.7.48.1" {
                        if let GeneralName::URI(u) = &access.access_location {
                            ocsp_url = Some(u.to_string());
                        }
                    }
                }
            }
            CRLDistributionPoints(cdp) => {
                for dp in &cdp.points {
                    if let Some(point) = &dp.distribution_point {
                        if let FullName(names) = point {
                            if let Some(GeneralName::URI(u)) = names.first() {
                                crl_url = Some(u.to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (ocsp_url, crl_url)
}

// ---------------------------------------------------------------------------
// OCSP request builder
// ---------------------------------------------------------------------------

fn create_ocsp_request(cert: &x509_parser::certificate::X509Certificate) -> Result<Vec<u8>, PdfError> {
    use rasn::ber::encode;
    use rasn::types::ObjectIdentifier;
    use rasn_ocsp::{CertId, Request, TbsRequest};
    use x509_parser::num_bigint::{BigInt, Sign};

    let sha1_oid = ObjectIdentifier::new_unchecked(Cow::from(vec![1u32, 3, 14, 3, 2, 26]));
    let sha1 = rasn_pkix::AlgorithmIdentifier {
        algorithm: sha1_oid,
        parameters: None,
    };

    let request = Request {
        req_cert: CertId {
            hash_algorithm: sha1,
            issuer_name_hash: Default::default(),
            issuer_key_hash:  Default::default(),
            serial_number: BigInt::from_bytes_le(Sign::Plus, cert.raw_serial()).into(),
        },
        single_request_extensions: None,
    };

    let tbs = TbsRequest {
        version: Default::default(),
        requestor_name: None,
        request_list: vec![request],
        request_extensions: None,
    };

    let ocsp_req = rasn_ocsp::OcspRequest { tbs_request: tbs, optional_signature: None };
    encode(&ocsp_req).map_err(|e| PdfError::Parse {
        offset: None,
        context: format!("OCSP request encode error: {e:?}"),
    })
}

// ---------------------------------------------------------------------------
// Network fetch helpers
// ---------------------------------------------------------------------------

/// Fetch a live OCSP response for `cert`.  Returns `None` on network/parse errors
/// (non-fatal — signing continues without OCSP data).
pub fn fetch_ocsp_response(cert: &CapturedX509Certificate, ocsp_url: &str)
    -> Option<Vec<u8>>
{
    use x509_parser::prelude::*;

    let der = cert.encode_der().ok()?;
    let parsed_cert = X509Certificate::from_der(&der).ok()?.1;
    let ocsp_req = create_ocsp_request(&parsed_cert).ok()?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build().ok()?;
    let resp = client
        .post(ocsp_url)
        .header("Content-Type", "application/ocsp-request")
        .body(ocsp_req)
        .send().ok()?;

    if resp.status().is_success() {
        resp.bytes().ok().map(|b| b.to_vec())
    } else {
        eprintln!("[ltv] OCSP request to {ocsp_url} failed: HTTP {}", resp.status());
        None
    }
}

/// Fetch a CRL from `crl_url`.  Returns `None` on network errors.
pub fn fetch_crl_response(crl_url: &str) -> Option<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build().ok()?;
    let resp = client.get(crl_url).send().ok()?;
    if resp.status().is_success() {
        resp.bytes().ok().map(|b| b.to_vec())
    } else {
        eprintln!("[ltv] CRL request to {crl_url} failed: HTTP {}", resp.status());
        None
    }
}

// ---------------------------------------------------------------------------
// Revocation data collection
// ---------------------------------------------------------------------------

/// Fetch CRL and/or OCSP data for all certificates in the chain.
/// Returns `(crl_data, ocsp_data)` — each a Vec of raw DER bytes.
pub fn fetch_revocation_data(
    chain:        &[CapturedX509Certificate],
    include_crl:  bool,
    include_ocsp: bool,
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut crl_data  = Vec::new();
    let mut ocsp_data = Vec::new();

    for cert in chain {
        let (ocsp_url, crl_url) = get_ocsp_crl_url(cert);

        if include_ocsp {
            if let Some(url) = ocsp_url {
                if let Some(data) = fetch_ocsp_response(cert, &url) {
                    ocsp_data.push(data);
                }
            }
        }
        if include_crl {
            if let Some(url) = crl_url {
                if let Some(data) = fetch_crl_response(&url) {
                    crl_data.push(data);
                }
            }
        }
    }

    (crl_data, ocsp_data)
}

// ---------------------------------------------------------------------------
// adbe-revocationInfoArchival attribute encoding
// ---------------------------------------------------------------------------

/// Encode CRL + OCSP bytes into the `RevocationInfoArchival` ASN.1 structure
/// used by the `adbe-revocationInfoArchival` CMS signed attribute.
pub fn encode_revocation_info_archival(
    crls:  Vec<Vec<u8>>,
    ocsps: Vec<Vec<u8>>,
) -> Option<Captured> {
    let mut parts: Vec<Captured> = Vec::new();

    if !crls.is_empty() {
        let crl_items: Vec<CrlBytes> = crls.into_iter()
            .map(|b| CrlBytes { bytes: Bytes::copy_from_slice(&b) })
            .collect();
        let crl_seq  = bcder::encode::sequence(crl_items);
        let crl_tag  = bcder::encode::sequence_as(Tag::CTX_0, crl_seq);
        parts.push(crl_tag.to_captured(Der));
    }

    if !ocsps.is_empty() {
        let mut ocsp_items = Vec::new();
        for ocsp_bytes in ocsps {
            let ocsp_os  = OctetString::new(Bytes::from(ocsp_bytes));
            let pkix_oid = Oid(Bytes::copy_from_slice(&[43, 6, 1, 5, 5, 7, 48, 1, 1]));
            let basic_resp = bcder::encode::sequence((pkix_oid.encode(), ocsp_os.encode()));
            let tagged_resp = bcder::encode::sequence_as(Tag::CTX_0, basic_resp);
            let status_enum = Integer::from(0u8).encode_as(Tag::ENUMERATED).to_captured(Der);
            let ocsp_resp   = bcder::encode::sequence((status_enum, tagged_resp));
            ocsp_items.push(ocsp_resp);
        }
        let ocsp_seq = bcder::encode::sequence(ocsp_items);
        let ocsp_tag = bcder::encode::sequence_as(Tag::CTX_1, ocsp_seq);
        parts.push(ocsp_tag.to_captured(Der));
    }

    if parts.is_empty() {
        None
    } else {
        Some(bcder::encode::sequence(parts).to_captured(Der))
    }
}

/// Build the `adbe-revocationInfoArchival` **signed attribute** value and OID.
/// Used inside `SignerBuilder::signed_attribute()`.
pub fn build_adbe_revocation_attribute(
    chain:        &[CapturedX509Certificate],
    include_crl:  bool,
    include_ocsp: bool,
) -> Option<(Oid, Vec<AttributeValue>)> {
    let (crl_data, ocsp_data) = fetch_revocation_data(chain, include_crl, include_ocsp);
    let encoded = encode_revocation_info_archival(crl_data, ocsp_data)?;

    // OID 1.2.840.113583.1.1.8  =  adbe-revocationInfoArchival
    let oid = Oid(Bytes::copy_from_slice(&[42, 134, 72, 134, 247, 47, 1, 1, 8]));
    Some((oid, vec![AttributeValue::new(encoded)]))
}

// ---------------------------------------------------------------------------
// inject_unsigned_attribute_into_cms
// ---------------------------------------------------------------------------

/// Inject an unsigned attribute DER blob into an already-built CMS SignedData.
///
/// Adobe/Foxit require `adbe-revocationInfoArchival` to be in the CMS
/// **unsigned** attributes (not signed) for PKCS7 LTV recognition.
/// This function splices the attribute into the SignerInfo's `[1] unsignedAttrs`.
pub fn inject_unsigned_attribute_into_cms(
    cms_der:  &[u8],
    attr_der: &[u8],
) -> Result<Vec<u8>, PdfError> {
    let err = |msg: &str| PdfError::Parse { offset: None, context: msg.to_string() };

    // DER TLV helpers
    let read_tl = |data: &[u8], pos: usize| -> Option<(u8, usize, usize)> {
        if pos >= data.len() { return None; }
        let tag = data[pos];
        if pos + 1 >= data.len() { return None; }
        let lb = data[pos + 1];
        if lb < 0x80 {
            Some((tag, 2, lb as usize))
        } else {
            let nb = (lb & 0x7f) as usize;
            if nb == 0 || nb > 4 || pos + 2 + nb > data.len() { return None; }
            let mut length = 0usize;
            for i in 0..nb { length = (length << 8) | data[pos + 2 + i] as usize; }
            Some((tag, 2 + nb, length))
        }
    };
    let skip_tlv = |data: &[u8], pos: usize| -> Option<usize> {
        let (_, hdr, len) = read_tl(data, pos)?;
        Some(pos + hdr + len)
    };
    let encode_length = |len: usize| -> Vec<u8> {
        if len < 0x80 { vec![len as u8] }
        else if len <= 0xff { vec![0x81, len as u8] }
        else if len <= 0xffff { vec![0x82, (len >> 8) as u8, (len & 0xff) as u8] }
        else if len <= 0xff_ffff {
            vec![0x83, (len >> 16) as u8, ((len >> 8) & 0xff) as u8, (len & 0xff) as u8]
        } else {
            vec![0x84, (len >> 24) as u8, ((len >> 16) & 0xff) as u8,
                       ((len >>  8) & 0xff) as u8, (len & 0xff) as u8]
        }
    };

    // ── navigate to SignerInfo ────────────────────────────────────────────
    let (_, ci_hdr, _) = read_tl(cms_der, 0).ok_or_else(|| err("bad ContentInfo"))?;
    let oid_end = skip_tlv(cms_der, ci_hdr).ok_or_else(|| err("cannot skip OID"))?;
    let (_, ctx0_hdr, _) = read_tl(cms_der, oid_end).ok_or_else(|| err("bad [0] EXPLICIT"))?;
    let sd_start = oid_end + ctx0_hdr;
    let (_, sd_hdr, _) = read_tl(cms_der, sd_start).ok_or_else(|| err("bad SignedData"))?;
    let mut pos = sd_start + sd_hdr;

    pos = skip_tlv(cms_der, pos).ok_or_else(|| err("skip version"))?;        // version
    pos = skip_tlv(cms_der, pos).ok_or_else(|| err("skip digestAlgs"))?;     // digestAlgorithms
    pos = skip_tlv(cms_der, pos).ok_or_else(|| err("skip encapContentInfo"))?; // encapContentInfo
    if pos < cms_der.len() && cms_der[pos] & 0xe0 == 0xa0 && cms_der[pos] & 0x1f == 0 {
        pos = skip_tlv(cms_der, pos).ok_or_else(|| err("skip certificates"))?;
    }
    if pos < cms_der.len() && cms_der[pos] == 0xa1 {
        pos = skip_tlv(cms_der, pos).ok_or_else(|| err("skip crls"))?;
    }

    // signerInfos SET
    let (_, si_set_hdr, _) = read_tl(cms_der, pos).ok_or_else(|| err("bad signerInfos"))?;
    let si_start = pos + si_set_hdr;
    let (_, si_hdr, si_len) = read_tl(cms_der, si_start).ok_or_else(|| err("bad SignerInfo"))?;
    let si_content_start = si_start + si_hdr;
    let si_content_end   = si_content_start + si_len;

    // walk SignerInfo fields
    let mut sp = si_content_start;
    sp = skip_tlv(cms_der, sp).ok_or_else(|| err("skip SI.version"))?;       // version
    sp = skip_tlv(cms_der, sp).ok_or_else(|| err("skip SI.sid"))?;            // sid
    sp = skip_tlv(cms_der, sp).ok_or_else(|| err("skip SI.digestAlg"))?;      // digestAlgorithm
    if sp < si_content_end && cms_der[sp] == 0xa0 {
        sp = skip_tlv(cms_der, sp).ok_or_else(|| err("skip SI.signedAttrs"))?;
    }
    sp = skip_tlv(cms_der, sp).ok_or_else(|| err("skip SI.sigAlg"))?;         // signatureAlgorithm
    sp = skip_tlv(cms_der, sp).ok_or_else(|| err("skip SI.signature"))?;      // signature

    // sp is now at unsignedAttrs [1] or si_content_end
    let has_ua = sp < si_content_end && cms_der[sp] == 0xa1;

    let new_ua_content: Vec<u8> = if has_ua {
        let (_, ua_hdr, ua_len) = read_tl(cms_der, sp).ok_or_else(|| err("bad unsignedAttrs"))?;
        let ua_cs = sp + ua_hdr;
        let ua_ce = ua_cs + ua_len;
        let mut c = cms_der[ua_cs..ua_ce].to_vec();
        c.extend_from_slice(attr_der);
        c
    } else {
        attr_der.to_vec()
    };

    // ── rebuild from inside-out ───────────────────────────────────────────
    let ua_len_bytes = encode_length(new_ua_content.len());
    let si_before = &cms_der[si_content_start..sp];

    let mut new_si_content = Vec::with_capacity(si_before.len() + 1 + ua_len_bytes.len() + new_ua_content.len());
    new_si_content.extend_from_slice(si_before);
    new_si_content.push(0xa1);
    new_si_content.extend_from_slice(&ua_len_bytes);
    new_si_content.extend_from_slice(&new_ua_content);

    let si_seq_len = encode_length(new_si_content.len());
    let mut new_si = vec![0x30u8];
    new_si.extend_from_slice(&si_seq_len);
    new_si.extend_from_slice(&new_si_content);

    let si_set_len = encode_length(new_si.len());
    let mut new_si_set = vec![0x31u8];
    new_si_set.extend_from_slice(&si_set_len);
    new_si_set.extend_from_slice(&new_si);

    let sd_before = &cms_der[sd_start + sd_hdr .. pos];
    let mut new_sd_content = Vec::with_capacity(sd_before.len() + new_si_set.len());
    new_sd_content.extend_from_slice(sd_before);
    new_sd_content.extend_from_slice(&new_si_set);

    let sd_seq_len = encode_length(new_sd_content.len());
    let mut new_sd = vec![0x30u8];
    new_sd.extend_from_slice(&sd_seq_len);
    new_sd.extend_from_slice(&new_sd_content);

    let ctx0_len = encode_length(new_sd.len());
    let mut new_ctx0 = vec![0xa0u8];
    new_ctx0.extend_from_slice(&ctx0_len);
    new_ctx0.extend_from_slice(&new_sd);

    let oid_bytes = &cms_der[ci_hdr..oid_end];
    let mut new_ci_content = Vec::with_capacity(oid_bytes.len() + new_ctx0.len());
    new_ci_content.extend_from_slice(oid_bytes);
    new_ci_content.extend_from_slice(&new_ctx0);

    let ci_seq_len = encode_length(new_ci_content.len());
    let mut result = vec![0x30u8];
    result.extend_from_slice(&ci_seq_len);
    result.extend_from_slice(&new_ci_content);

    Ok(result)
}

/// Build a complete DER `Attribute` wrapping `adbe-revocationInfoArchival`
/// suitable for injection via `inject_unsigned_attribute_into_cms`.
pub fn build_adbe_revocation_unsigned_der(
    chain:        &[CapturedX509Certificate],
    include_crl:  bool,
    include_ocsp: bool,
) -> Option<Vec<u8>> {
    let (crl_data, ocsp_data) = fetch_revocation_data(chain, include_crl, include_ocsp);
    let encoded = encode_revocation_info_archival(crl_data, ocsp_data)?;

    let oid = Oid(Bytes::copy_from_slice(&[42, 134, 72, 134, 247, 47, 1, 1, 8]));
    let rev_raw = RawDerBytes(encoded.as_slice().to_vec());

    let attr = bcder::encode::sequence((
        oid.encode(),
        bcder::encode::set(rev_raw),
    ));
    Some(attr.to_captured(Der).as_slice().to_vec())
}

// ---------------------------------------------------------------------------
// DSS dictionary (incremental append)
// ---------------------------------------------------------------------------

/// Append a DSS (Document Security Store) dictionary to `pdf_bytes` as an
/// incremental update.  Fetches fresh CRL + OCSP data for every cert in
/// the chain and writes `/DSS` into the document catalog.
///
/// Mirrors `rust_pdf_signing::ltv::append_dss_dictionary`.
pub fn append_dss_dictionary(
    pdf_bytes: Vec<u8>,
    chain:     &[CapturedX509Certificate],
) -> Result<Vec<u8>, PdfError> {
    use crate::cos::{CosDictionary, CosName, CosObject, CosStream, ObjectId};
    use crate::writer::IncrementalWriter;
    use crate::Document;
    use std::collections::BTreeMap;

    let (crl_data, ocsp_data) = fetch_revocation_data(chain, true, true);

    let doc = Document::load_from_bytes(&pdf_bytes)?;

    // Find next free object ID
    let next = doc.objects.max_object_number() + 1;
    let mut obj_counter = next;
    let mut changed: BTreeMap<ObjectId, CosObject> = BTreeMap::new();

    // Helper: allocate next ObjectId
    let mut alloc = || {
        let id = ObjectId::new(obj_counter, 0);
        obj_counter += 1;
        id
    };

    // Add CRL streams
    let mut crl_refs: Vec<CosObject> = Vec::new();
    for crl in crl_data {
        let id = alloc();
        let mut dict = CosDictionary::new();
        dict.set(CosName::new(b"Length"), CosObject::Integer(crl.len() as i64));
        let stream = CosObject::Stream(CosStream::new(dict, crl));
        changed.insert(id, stream);
        crl_refs.push(CosObject::Reference(id));
    }

    // Add OCSP streams
    let mut ocsp_refs: Vec<CosObject> = Vec::new();
    for ocsp in ocsp_data {
        let id = alloc();
        let mut dict = CosDictionary::new();
        dict.set(CosName::new(b"Length"), CosObject::Integer(ocsp.len() as i64));
        let stream = CosObject::Stream(CosStream::new(dict, ocsp));
        changed.insert(id, stream);
        ocsp_refs.push(CosObject::Reference(id));
    }

    // Add cert streams
    let mut cert_refs: Vec<CosObject> = Vec::new();
    for cert in chain {
        let der = cert.encode_der().or_else(|_| cert.encode_ber()).map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("cert encode: {e}"),
        })?;
        let id = alloc();
        let mut dict = CosDictionary::new();
        dict.set(CosName::new(b"Length"), CosObject::Integer(der.len() as i64));
        let stream = CosObject::Stream(CosStream::new(dict, der));
        changed.insert(id, stream);
        cert_refs.push(CosObject::Reference(id));
    }

    // Build DSS dictionary object
    let dss_id = alloc();
    let mut dss_dict = CosDictionary::new();
    dss_dict.set(CosName::new(b"CRLs"),  CosObject::Array(crl_refs));
    dss_dict.set(CosName::new(b"OCSPs"), CosObject::Array(ocsp_refs));
    dss_dict.set(CosName::new(b"Certs"), CosObject::Array(cert_refs));
    changed.insert(dss_id, CosObject::Dictionary(dss_dict));

    // Update catalog to point to DSS
    let catalog_id = doc.catalog_ref().unwrap_or(ObjectId::new(1, 0));
    let mut cat = doc.objects
        .get(&catalog_id)
        .and_then(|o| o.as_dictionary())
        .cloned()
        .unwrap_or_else(CosDictionary::new);
    cat.set(CosName::new(b"DSS"), CosObject::Reference(dss_id));
    changed.insert(catalog_id, CosObject::Dictionary(cat));

    // Write incremental update
    let mut out = Vec::with_capacity(pdf_bytes.len() + 32768);
    IncrementalWriter::write_update(&pdf_bytes, &doc, &changed, &mut out)
        .map_err(|e| PdfError::Parse { offset: None, context: format!("DSS write: {e}") })?;

    Ok(out)
}

// ---------------------------------------------------------------------------
// RFC 3161 timestamp token fetch
// ---------------------------------------------------------------------------

/// Request an RFC 3161 timestamp token from `tsa_url`.
/// `message_digest` is the SHA-256 hash of the data to be timestamped.
/// Returns the raw DER-encoded `TimeStampToken` (CMS ContentInfo).
pub fn fetch_timestamp_token(tsa_url: &str, message_digest: &[u8]) -> Result<Vec<u8>, PdfError> {
    let to_err = |msg: String| PdfError::Parse { offset: None, context: msg };

    // SHA-256 OID: 2.16.840.1.101.3.4.2.1
    let sha256_oid_der: &[u8] = &[
        0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
    ];

    let mut alg_content = Vec::new();
    alg_content.extend_from_slice(sha256_oid_der);
    alg_content.extend_from_slice(&[0x05, 0x00]); // NULL
    let mut alg_id = vec![0x30u8];
    der_push_length(&mut alg_id, alg_content.len());
    alg_id.extend_from_slice(&alg_content);

    let mut hashed_msg = vec![0x04u8];
    der_push_length(&mut hashed_msg, message_digest.len());
    hashed_msg.extend_from_slice(message_digest);

    let mut mi_content = Vec::new();
    mi_content.extend_from_slice(&alg_id);
    mi_content.extend_from_slice(&hashed_msg);
    let mut msg_imprint = vec![0x30u8];
    der_push_length(&mut msg_imprint, mi_content.len());
    msg_imprint.extend_from_slice(&mi_content);

    // TimeStampReq = SEQUENCE { version INTEGER 1, messageImprint, certReq BOOLEAN TRUE }
    let version_der: &[u8] = &[0x02, 0x01, 0x01];
    let cert_req_der: &[u8] = &[0x01, 0x01, 0xff];

    let mut ts_req_content = Vec::new();
    ts_req_content.extend_from_slice(version_der);
    ts_req_content.extend_from_slice(&msg_imprint);
    ts_req_content.extend_from_slice(cert_req_der);

    let mut ts_req = vec![0x30u8];
    der_push_length(&mut ts_req, ts_req_content.len());
    ts_req.extend_from_slice(&ts_req_content);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| to_err(format!("TSA client build error: {e}")))?;

    let resp = client
        .post(tsa_url)
        .header("Content-Type", "application/timestamp-query")
        .body(ts_req)
        .send()
        .map_err(|e| to_err(format!("TSA request to {tsa_url} failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(to_err(format!("TSA returned HTTP {}", resp.status())));
    }

    let data: Vec<u8> = resp
        .bytes()
        .map_err(|e| to_err(format!("TSA response read error: {e}")))?
        .to_vec();

    if data.len() < 5 || data[0] != 0x30 {
        return Err(to_err("Invalid TSA response: not a SEQUENCE".into()));
    }

    // Parse TimeStampResp ::= SEQUENCE { PKIStatusInfo, TimeStampToken OPTIONAL }
    let (outer_content_start, _) = der_read_length(&data, 1)
        .ok_or_else(|| to_err("Invalid TSA response: bad outer length".into()))?;

    if outer_content_start >= data.len() || data[outer_content_start] != 0x30 {
        return Err(to_err("Invalid TSA response: missing PKIStatusInfo".into()));
    }
    let (status_cs, status_len) = der_read_length(&data, outer_content_start + 1)
        .ok_or_else(|| to_err("Invalid TSA response: bad PKIStatusInfo length".into()))?;

    // Check status == 0 (granted) or 1 (grantedWithMods)
    if status_cs < data.len() && data[status_cs] == 0x02 {
        let (val_start, val_len) = der_read_length(&data, status_cs + 1)
            .ok_or_else(|| to_err("Invalid TSA status".into()))?;
        if val_len == 1 && val_start < data.len() && data[val_start] > 2 {
            return Err(to_err(format!("TSA rejected: status {}", data[val_start])));
        }
    }

    let token_start = status_cs + status_len;
    if token_start >= data.len() {
        return Err(to_err("TSA response contains no TimeStampToken".into()));
    }

    Ok(data[token_start..].to_vec())
}

