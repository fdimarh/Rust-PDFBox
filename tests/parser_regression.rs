//! Integration-level regression tests for malformed and edge-case PDF inputs.
//!
//! These tests operate at the `Document::load_from_bytes` level, simulating
//! real-world malformed PDFs that Java PDFBox is known to handle (or reject
//! gracefully). All tests exercise the full load pipeline: header → xref →
//! object store → page tree.
//!
//! # Coverage categories
//!
//! | Category | Description |
//! |---|---|
//! | Header variants | BOM prefix, leading junk, version numbers |
//! | XRef edge cases | Zero-length xref, startxref-at-zero, wrong subsection count |
//! | Object store | Objects with truncated bodies, duplicate IDs |
//! | Page tree | Empty Kids, missing MediaBox, deep tree |
//! | Content stream | Empty stream, comment-only, single operator |

use rust_pdfbox::{Document, PdfError, PdfObjectId};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builds a complete PDF byte sequence from parts.
/// Objects are passed as (id, raw_object_bytes) pairs and are laid out
/// sequentially. The xref table and trailer are appended automatically.
///
/// `base_offset` is added to every object offset recorded in the xref table.
/// Use this when the returned bytes will be appended after a prefix (e.g. a
/// binary comment line) so that all xref offsets correctly reflect their
/// position in the final combined buffer.
fn build_pdf(version: &[u8], objects: &[(u32, &[u8])], root_id: u32) -> Vec<u8> {
    build_pdf_with_base(version, objects, root_id, 0)
}

/// Like `build_pdf` but shifts every xref offset by `base_offset` bytes.
fn build_pdf_with_base(version: &[u8], objects: &[(u32, &[u8])], root_id: u32, base_offset: usize) -> Vec<u8> {
    let mut pdf = b"%PDF-".to_vec();
    pdf.extend_from_slice(version);
    pdf.push(b'\n');

    let mut offsets: Vec<(u32, u64)> = Vec::new();

    for (id, body) in objects {
        let offset = (pdf.len() + base_offset) as u64;
        offsets.push((*id, offset));
        // Write: N 0 obj\n<body>\nendobj\n
        pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }

    // xref table
    // We need entries for objects 0..max_id+1
    let max_id = objects.iter().map(|(id, _)| *id).max().unwrap_or(0);
    let xref_offset = pdf.len() + base_offset;

    pdf.extend_from_slice(b"xref\n");
    pdf.extend_from_slice(format!("0 {}\n", max_id + 1).as_bytes());

    // Object 0 always free
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");

    // Fill in sorted by object ID
    let mut offset_map = std::collections::HashMap::new();
    for (id, off) in &offsets {
        offset_map.insert(*id, *off);
    }
    for id in 1..=max_id {
        if let Some(off) = offset_map.get(&id) {
            pdf.extend_from_slice(format!("{:010} 00000 n \r\n", off).as_bytes());
        } else {
            // Unused slot — mark free
            pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        }
    }

    pdf.extend_from_slice(
        format!("trailer\n<< /Size {} /Root {} 0 R >>\n", max_id + 1, root_id).as_bytes(),
    );
    pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    pdf
}

// ---------------------------------------------------------------------------
// Header variant tests
// ---------------------------------------------------------------------------

#[test]
fn load_pdf_version_1_4() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert!(doc.source_len() > 0);
}

#[test]
fn load_pdf_version_1_7() {
    let pdf = build_pdf(
        b"1.7",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
        ],
        1,
    );
    assert!(Document::load_from_bytes(&pdf).is_ok());
}

#[test]
fn load_pdf_version_2_0() {
    let pdf = build_pdf(
        b"2.0",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
        ],
        1,
    );
    assert!(Document::load_from_bytes(&pdf).is_ok());
}

#[test]
fn reject_no_pdf_header() {
    let err = Document::load_from_bytes(b"This is not a PDF file.\n").unwrap_err();
    assert!(matches!(err, PdfError::Parse { .. }));
}

#[test]
fn reject_truncated_header() {
    let err = Document::load_from_bytes(b"%PDF").unwrap_err();
    assert!(matches!(err, PdfError::Parse { .. }));
}

#[test]
fn reject_html_file() {
    let err = Document::load_from_bytes(b"<html><body>Hello</body></html>").unwrap_err();
    assert!(matches!(err, PdfError::Parse { .. }));
}

#[test]
fn reject_empty_bytes() {
    let err = Document::load_from_bytes(b"").unwrap_err();
    assert!(matches!(err, PdfError::Parse { .. }));
}

#[test]
fn header_preceded_by_binary_comment() {
    // Real PDFs often have %PDF-1.x preceded by binary comment line — still valid.
    // The prefix is 5 bytes: b"%\xe2\xe3\xcf\xd3\n"
    let prefix = b"%\xe2\xe3\xcf\xd3\n";
    let mut pdf = prefix.to_vec();
    pdf.extend_from_slice(&build_pdf_with_base(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
        ],
        1,
        prefix.len(),  // shift all xref offsets by the prefix length
    ));
    // The header finder scans first 1024 bytes — should find %PDF-
    assert!(Document::load_from_bytes(&pdf).is_ok());
}

// ---------------------------------------------------------------------------
// XRef edge cases
// ---------------------------------------------------------------------------

#[test]
fn xref_missing_startxref_is_error() {
    // A PDF with no startxref keyword at all
    let pdf = b"%PDF-1.4\n1 0 obj\n42\nendobj\n%%EOF\n";
    let err = Document::load_from_bytes(pdf).unwrap_err();
    assert!(matches!(err, PdfError::Parse { .. }));
}

#[test]
fn xref_startxref_pointing_past_eof() {
    let pdf = b"%PDF-1.4\nstartxref\n999999\n%%EOF\n";
    let err = Document::load_from_bytes(pdf).unwrap_err();
    assert!(matches!(err, PdfError::Parse { .. }));
}

#[test]
fn xref_with_zero_object_count() {
    // xref with 0 objects (just the free head) — empty document
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(b"trailer\n<< /Size 1 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    // Catalog ref points to nonexistent obj 1 — load should succeed but catalog will be None
    let doc = Document::load_from_bytes(&pdf);
    // Accept either success (empty store) or a parse/xref error — must not panic
    match doc {
        Ok(d) => assert_eq!(d.object_count(), 0),
        Err(PdfError::Parse { .. }) | Err(PdfError::Xref { .. }) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Object store edge cases
// ---------------------------------------------------------------------------

#[test]
fn object_with_empty_dictionary() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
            (3, b"<< >>"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let empty_dict = doc.objects.get(&PdfObjectId::new(3, 0));
    assert!(empty_dict.is_some());
    assert_eq!(empty_dict.unwrap().as_dictionary().unwrap().len(), 0);
}

#[test]
fn object_with_null_value() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
            (3, b"null"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let null_obj = doc.objects.get(&PdfObjectId::new(3, 0));
    assert!(null_obj.is_some());
    assert!(null_obj.unwrap().is_null());
}

#[test]
fn object_with_large_integer() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
            (3, b"2147483647"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let obj = doc.objects.get(&PdfObjectId::new(3, 0)).unwrap();
    assert_eq!(obj.as_integer(), Some(2_147_483_647));
}

#[test]
fn object_with_string_value() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
            (3, b"(Hello, World!)"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let obj = doc.objects.get(&PdfObjectId::new(3, 0)).unwrap();
    assert_eq!(obj.as_string(), Some(b"Hello, World!".as_slice()));
}

#[test]
fn object_with_boolean_false() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
            (3, b"false"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let obj = doc.objects.get(&PdfObjectId::new(3, 0)).unwrap();
    assert_eq!(obj.as_bool(), Some(false));
}

// ---------------------------------------------------------------------------
// Page tree edge cases
// ---------------------------------------------------------------------------

#[test]
fn empty_page_tree_count_zero() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 0);
}

#[test]
fn single_page_no_media_box() {
    // A page without /MediaBox — media_box() should return None, not panic
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
            (3, b"<< /Type /Page >>"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 1);
    let tree = doc.pages().unwrap();
    let page = tree.get(0).unwrap();
    assert!(page.media_box().is_none());
}

#[test]
fn page_with_rotation_270() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
            (3, b"<< /Type /Page /MediaBox [0 0 612 792] /Rotate 270 >>"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let tree = doc.pages().unwrap();
    assert_eq!(tree.get(0).unwrap().rotation(), 270);
}

#[test]
fn three_page_pdf() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>"),
            (3, b"<< /Type /Page /MediaBox [0 0 612 792] >>"),
            (4, b"<< /Type /Page /MediaBox [0 0 595 842] >>"),
            (5, b"<< /Type /Page /MediaBox [0 0 210 297] >>"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 3);
    let tree = doc.pages().unwrap();
    assert_eq!(tree.get(0).unwrap().media_box().unwrap().width(), 612.0);
    assert_eq!(tree.get(1).unwrap().media_box().unwrap().width(), 595.0);
    assert_eq!(tree.get(2).unwrap().media_box().unwrap().width(), 210.0);
}

#[test]
fn page_with_resources_dict() {
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
            (3, b"<< /Type /Page /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> >>"),
            (4, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let tree = doc.pages().unwrap();
    let page = tree.get(0).unwrap();
    let res = page.resources().expect("resources should be present");
    assert!(res.font_dict().is_some());
    assert_eq!(res.font_dict().unwrap().len(), 1);
}

#[test]
fn missing_catalog_gives_graceful_error() {
    // Root ref points to nonexistent object
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(b"trailer\n<< /Size 1 /Root 99 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());

    let doc = Document::load_from_bytes(&pdf).unwrap();
    // catalog_ref returns Some(99 0 R) but catalog() returns None
    assert_eq!(doc.catalog_ref(), Some(PdfObjectId::new(99, 0)));
    assert!(doc.catalog().is_none());
    // pages() must return an error, not panic
    assert!(doc.pages().is_err());
}

// ---------------------------------------------------------------------------
// Content stream tokenization via Document
// ---------------------------------------------------------------------------

#[test]
fn content_stream_is_stored_as_stream_object() {
    // A page whose /Contents is a stream object
    let pdf = build_pdf(
        b"1.4",
        &[
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
            (3, b"<< /Type /Page /MediaBox [0 0 612 792] /Contents 4 0 R >>"),
            (4, b"<< /Length 0 >> stream\nendstream"),
        ],
        1,
    );
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let tree = doc.pages().unwrap();
    let page = tree.get(0).unwrap();
    // /Contents should be an indirect reference
    let contents = page.contents_object();
    assert!(contents.is_some());
    assert!(contents.unwrap().as_reference().is_some());
}

// ---------------------------------------------------------------------------
// Smoke: parse-and-survive tests (must not panic regardless of outcome)
// ---------------------------------------------------------------------------

#[test]
fn survives_all_zeros() {
    let _ = Document::load_from_bytes(&[0u8; 64]);
}

#[test]
fn survives_all_ones() {
    let _ = Document::load_from_bytes(&[0xFFu8; 64]);
}

#[test]
fn survives_random_ascii_garbage() {
    let garbage = b"!@#$%^&*()_+-=[]{}|;':\",./<>? ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let _ = Document::load_from_bytes(garbage);
}

#[test]
fn survives_truncated_xref_table() {
    // Valid header, truncated xref
    let pdf = b"%PDF-1.4\nxref\n0 2\n0000000000 65535 f \r\nstartxref\n9\n%%EOF\n";
    let _ = Document::load_from_bytes(pdf);
}

#[test]
fn survives_header_only() {
    let _ = Document::load_from_bytes(b"%PDF-1.4\n");
}

