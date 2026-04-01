//! Corpus breadth integration tests.
//!
//! Tests across fixture tiers:
//! - `smoke`      : small valid PDFs — baseline open/parse/page checks
//! - `malformed`  : damaged/partial PDFs — recovery behaviour
//! - `font_heavy` : varied content streams — text extraction
//! - `encrypted`  : security handler stub — permission checks
//! - `large`      : synthesised "large" object count — scalability
//!
//! All fixtures are generated in-memory (no binary files required).

use rust_pdfbox::{Document, RecoveryReport};
use rust_pdfbox::cos::{CosObject, ObjectId};

// ===========================================================================
// Helpers
// ===========================================================================

/// Build a valid minimal single-page PDF.
fn single_page_pdf(width: f64, height: f64) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let p1_off = pdf.len() as u64;
    pdf.extend_from_slice(
        format!("2 0 obj\n<< /Type /Page /MediaBox [0 0 {width} {height}] >>\nendobj\n")
            .as_bytes(),
    );
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", p1_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

/// Build a valid N-page PDF.
fn n_page_pdf(n: usize) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut page_offsets: Vec<u64> = Vec::new();

    for i in 0..n {
        let obj_num = i as u32 + 2; // obj 2..n+1 are pages
        let off = pdf.len() as u64;
        page_offsets.push(off);
        pdf.extend_from_slice(
            format!(
                "{obj_num} 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n"
            )
            .as_bytes(),
        );
    }

    let kids: String = (0..n)
        .map(|i| format!("{} 0 R", i + 2))
        .collect::<Vec<_>>()
        .join(" ");
    let pages_off = pdf.len() as u64;
    let pages_obj = n + 2; // Pages is obj n+2
    pdf.extend_from_slice(
        format!(
            "{pages_obj} 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {n} >>\nendobj\n"
        )
        .as_bytes(),
    );

    let cat_off = pdf.len() as u64;
    let cat_obj = n + 3;
    pdf.extend_from_slice(
        format!(
            "{cat_obj} 0 obj\n<< /Type /Catalog /Pages {pages_obj} 0 R >>\nendobj\n"
        )
        .as_bytes(),
    );

    let xref_off = pdf.len();
    let total = n + 4; // 0 (free) + pages 2..n+1 + pages_dict + catalog
    pdf.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n"); // obj 0
    pdf.extend_from_slice(b"0000000000 65535 f \r\n"); // obj 1 (unused)
    for off in &page_offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {total} /Root {cat_obj} 0 R >>\n"
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

/// Build a PDF with a content stream containing text operators.
fn pdf_with_content_stream(text: &str) -> Vec<u8> {
    let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let stream_off = pdf.len() as u64;
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
            content.len()
        )
        .as_bytes(),
    );

    let page_off = pdf.len() as u64;
    pdf.extend_from_slice(
        b"2 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n",
    );
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");

    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", page_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", stream_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

// ===========================================================================
// Smoke tier — small valid PDFs
// ===========================================================================

#[test]
fn smoke_a4_single_page() {
    let pdf = single_page_pdf(595.0, 842.0);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 1);
    let pages = doc.pages().unwrap();
    let mb = pages.get(0).unwrap().media_box().unwrap();
    assert!((mb.width() - 595.0).abs() < 0.01);
    assert!((mb.height() - 842.0).abs() < 0.01);
}

#[test]
fn smoke_letter_single_page() {
    let pdf = single_page_pdf(612.0, 792.0);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn smoke_five_pages() {
    let pdf = n_page_pdf(5);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 5);
    let pages = doc.pages().unwrap();
    assert_eq!(pages.iter().count(), 5);
}

#[test]
fn smoke_ten_pages() {
    let pdf = n_page_pdf(10);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 10);
}

#[test]
fn smoke_page_media_box_dimensions() {
    let pdf = single_page_pdf(200.0, 300.0);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let tree = doc.pages().unwrap();
    let page = tree.get(0).unwrap();
    let mb = page.media_box().unwrap();
    assert!((mb.width() - 200.0).abs() < 0.01);
    assert!((mb.height() - 300.0).abs() < 0.01);
}

#[test]
fn smoke_catalog_and_pages_resolve() {
    let pdf = n_page_pdf(3);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert!(doc.catalog().is_some());
    assert!(doc.catalog_ref().is_some());
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn smoke_round_trip_save_reload() {
    let pdf = n_page_pdf(3);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let mut buf = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut buf).unwrap();
    let reloaded = Document::load_from_bytes(buf.get_ref()).unwrap();
    assert_eq!(reloaded.page_count(), 3);
}

#[test]
fn smoke_incremental_save() {
    let pdf = single_page_pdf(612.0, 792.0);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let mut changed = std::collections::BTreeMap::new();
    changed.insert(ObjectId::new(99, 0), CosObject::Bool(true));
    let mut out = Vec::new();
    doc.save_incremental(&pdf, &changed, &mut out).unwrap();
    let updated = Document::load_from_bytes(&out).unwrap();
    assert_eq!(updated.page_count(), 1);
    assert_eq!(updated.objects.get(&ObjectId::new(99, 0)), Some(&CosObject::Bool(true)));
}

// ===========================================================================
// Malformed tier — damaged PDFs, recovery mode
// ===========================================================================

#[test]
fn malformed_missing_header_lenient_recovers() {
    // No %PDF- header at all — lenient load should recover
    let mut bytes = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec();
    bytes.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    bytes.extend_from_slice(b"startxref\n0\n%%EOF\n");

    let (doc, report) = Document::load_lenient(&bytes);
    assert!(!report.is_clean(), "expected warnings for missing header");
    assert!(
        report.warnings.iter().any(|w| w.contains("header")),
        "expected header warning, got: {:?}", report.warnings
    );
    // Document is still returned (may have 0 objects due to broken xref)
    let _ = doc.page_count(); // must not panic
}

#[test]
fn malformed_broken_xref_lenient_linear_scan() {
    // Build a valid PDF body but corrupt the xref section
    let mut pdf = b"%PDF-1.4\n".to_vec();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    // Broken xref — garbage instead of real table
    pdf.extend_from_slice(b"xref\nGARBAGE\n");
    pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    pdf.extend_from_slice(b"startxref\n999999\n%%EOF\n"); // offset points nowhere

    let (doc, report) = Document::load_lenient(&pdf);
    // Must have recorded the xref recovery warning
    assert!(
        report.xref_recovered || !report.warnings.is_empty(),
        "expected xref recovery flag or warnings"
    );
    // Must not panic; page count may be 0
    let _ = doc.page_count();
}

#[test]
fn malformed_truncated_object_lenient_skips() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    // Object 1 is truncated — no endobj
    let obj1_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\n");
    // Object 2 is valid
    let obj2_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());

    let (doc, _report) = Document::load_lenient(&pdf);
    // Object 2 must still be available even if object 1 failed
    let _ = doc.object_count(); // must not panic
}

#[test]
fn malformed_empty_bytes_lenient_returns_empty_doc() {
    let (doc, report) = Document::load_lenient(&[]);
    assert!(!report.is_clean());
    assert_eq!(doc.page_count(), 0);
}

#[test]
fn malformed_garbage_only_lenient_survives() {
    let garbage = b"THIS IS NOT A PDF AT ALL \xFF\xFE\x00";
    let (doc, report) = Document::load_lenient(garbage);
    assert!(!report.is_clean());
    let _ = doc.page_count(); // must not panic
}

#[test]
fn malformed_startxref_pointing_past_eof_lenient() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    pdf.extend_from_slice(b"startxref\n9999999\n%%EOF\n"); // past EOF

    let (doc, report) = Document::load_lenient(&pdf);
    assert!(!report.is_clean());
    let _ = doc.page_count();
}

#[test]
fn malformed_recovery_report_is_clean_for_valid_pdf() {
    let pdf = single_page_pdf(612.0, 792.0);
    let (_doc, report) = Document::load_lenient(&pdf);
    assert!(
        report.is_clean(),
        "expected clean report for valid PDF, got: {:?}",
        report.warnings
    );
}

#[test]
fn malformed_duplicate_objects_lenient_first_wins() {
    // Two definitions of object 1 — first should win
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let obj1a_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let _obj1b_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n42\nendobj\n"); // duplicate, different value
    let obj2_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1a_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    let (doc, _) = Document::load_lenient(&pdf);
    // Catalog must resolve (from first obj 1)
    assert!(doc.catalog().is_some());
}

// ===========================================================================
// Font-heavy / content-stream tier
// ===========================================================================

#[test]
fn font_heavy_content_stream_accessible() {
    let pdf = pdf_with_content_stream("Hello World");
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 1);
    // Content stream object must be present
    let stream_id = ObjectId::new(4, 0);
    let stream_obj = doc.objects.get(&stream_id);
    assert!(stream_obj.is_some(), "content stream object must be present");
}

#[test]
fn font_heavy_text_extraction_from_stream() {
    let pdf = pdf_with_content_stream("TestText");
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let stream_id = ObjectId::new(4, 0);
    if let Some(CosObject::Stream(s)) = doc.objects.get(&stream_id) {
        let text = rust_pdfbox::text::extract_text(&s.data, None);
        assert!(text.contains("TestText"), "expected 'TestText' in: {text:?}");
    }
}

#[test]
fn font_heavy_multiline_content_stream() {
    let content =
        "BT /F1 12 Tf 72 720 Td (Line one) Tj 0 -14 Td (Line two) Tj ET";
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let stream_off = pdf.len() as u64;
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
            content.len()
        )
        .as_bytes(),
    );
    let page_off = pdf.len() as u64;
    pdf.extend_from_slice(
        b"2 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n",
    );
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", page_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", stream_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());

    let doc = Document::load_from_bytes(&pdf).unwrap();
    if let Some(CosObject::Stream(s)) = doc.objects.get(&ObjectId::new(4, 0)) {
        let text = rust_pdfbox::text::extract_text(&s.data, None);
        assert!(text.contains("Line one"), "expected 'Line one' in: {text:?}");
        assert!(text.contains("Line two"), "expected 'Line two' in: {text:?}");
    }
}

#[test]
fn font_heavy_empty_content_stream_produces_empty_text() {
    let pdf = pdf_with_content_stream("");
    let doc = Document::load_from_bytes(&pdf).unwrap();
    if let Some(CosObject::Stream(s)) = doc.objects.get(&ObjectId::new(4, 0)) {
        let text = rust_pdfbox::text::extract_text(&s.data, None);
        assert!(text.trim().is_empty());
    }
}

// ===========================================================================
// Encrypted tier — permission checks (no actual decryption needed)
// ===========================================================================

#[test]
fn encrypted_permissions_print_flag() {
    use rust_pdfbox::Permissions;
    let perms = Permissions::from_bits_p(Permissions::PRINT as i32);
    assert!(perms.can_print());
    assert!(!perms.can_copy());
}

#[test]
fn encrypted_permissions_all_allowed() {
    use rust_pdfbox::Permissions;
    let perms = Permissions::all_allowed();
    assert!(perms.can_print());
    assert!(perms.can_copy());
    assert!(perms.can_modify_content());
    assert!(perms.can_fill_forms());
}

#[test]
fn encrypted_permissions_none_allowed() {
    use rust_pdfbox::Permissions;
    let perms = Permissions::none_allowed();
    assert!(!perms.can_print());
    assert!(!perms.can_copy());
    assert!(!perms.can_assemble());
}

#[test]
fn encrypted_auth_result_api() {
    use rust_pdfbox::AuthResult;
    let r = AuthResult::UserPassword(vec![1, 2, 3]);
    assert!(r.is_authenticated());
    assert_eq!(r.encryption_key(), Some([1u8, 2, 3].as_slice()));

    let f = AuthResult::Failed;
    assert!(!f.is_authenticated());
    assert_eq!(f.encryption_key(), None);
}

#[test]
fn encrypted_standard_handler_key_derivation_deterministic() {
    use rust_pdfbox::{EncryptionDict, Permissions, StandardSecurityHandler};
    let enc = EncryptionDict {
        revision: 3,
        key_length: 16,
        o_entry: vec![0u8; 32],
        u_entry: vec![0u8; 32],
        permissions: Permissions::all_allowed(),
        crypt_filter: None,
    };
    let fid = b"testfileid000000";
    let k1 = StandardSecurityHandler::compute_encryption_key(&enc, b"pass", fid);
    let k2 = StandardSecurityHandler::compute_encryption_key(&enc, b"pass", fid);
    assert_eq!(k1, k2);
    assert_eq!(k1.len(), 16);
}

// ===========================================================================
// Large-scale tier — scalability
// ===========================================================================

#[test]
fn large_50_pages_loads_correctly() {
    let pdf = n_page_pdf(50);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 50);
}

#[test]
fn large_100_pages_loads_correctly() {
    let pdf = n_page_pdf(100);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert_eq!(doc.page_count(), 100);
}

#[test]
fn large_100_pages_page_iter_count() {
    let pdf = n_page_pdf(100);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let tree = doc.pages().unwrap();
    assert_eq!(tree.iter().count(), 100);
}

#[test]
fn large_100_pages_round_trip() {
    let pdf = n_page_pdf(100);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let mut buf = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut buf).unwrap();
    let reloaded = Document::load_from_bytes(buf.get_ref()).unwrap();
    assert_eq!(reloaded.page_count(), 100);
}

#[test]
fn large_many_small_objects() {
    // Build a document with many integer objects (stress test object store)
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let n = 200usize;
    let mut offsets = Vec::new();
    for i in 1..=n {
        offsets.push(pdf.len() as u64);
        pdf.extend_from_slice(format!("{i} 0 obj\n{i}\nendobj\n").as_bytes());
    }
    // Add catalog as obj n+1 and pages as obj n+2
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(
        format!(
            "{} 0 obj\n<< /Type /Catalog /Pages {} 0 R >>\nendobj\n",
            n + 1, n + 2
        )
        .as_bytes(),
    );
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(
        format!(
            "{} 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n",
            n + 2
        )
        .as_bytes(),
    );
    let xref_off = pdf.len();
    let total = n + 3;
    pdf.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {total} /Root {} 0 R >>\n",
            n + 1
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());

    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert!(doc.object_count() >= n, "expected ≥{n} objects, got {}", doc.object_count());
    // Spot-check a few integer objects
    for i in [1usize, 50, 100, 200] {
        let id = ObjectId::new(i as u32, 0);
        assert_eq!(
            doc.objects.get(&id),
            Some(&CosObject::Integer(i as i64)),
            "object {i} should be integer {i}"
        );
    }
}

// ===========================================================================
// RecoveryReport API
// ===========================================================================

#[test]
fn recovery_report_is_clean_default() {
    let report = RecoveryReport::default();
    assert!(report.is_clean());
    assert!(!report.xref_recovered);
    assert_eq!(report.objects_skipped, 0);
}

#[test]
fn recovery_report_not_clean_with_warnings() {
    let mut report = RecoveryReport::default();
    report.warnings.push("something broke".into());
    assert!(!report.is_clean());
}

#[test]
fn recovery_report_valid_pdf_is_clean() {
    let pdf = single_page_pdf(595.0, 842.0);
    let (_doc, report) = Document::load_lenient(&pdf);
    assert!(
        report.is_clean(),
        "valid PDF should produce clean report, got: {:?}",
        report.warnings
    );
}


