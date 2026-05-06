#![cfg(feature = "metadata")]

use std::io::Cursor;

use rust_pdfbox::Document;

fn build_pdf_with_info(info_dict: Option<&str>) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R >>\nendobj\n",
    );

    let content = b"BT ET";
    let obj4_offset = pdf.len();
    pdf.extend_from_slice(
        format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes(),
    );
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let mut obj5_offset = None;
    if let Some(dict_body) = info_dict {
        obj5_offset = Some(pdf.len());
        pdf.extend_from_slice(format!("5 0 obj\n<< {} >>\nendobj\n", dict_body).as_bytes());
    }

    let xref_offset = pdf.len();
    let size = if obj5_offset.is_some() { 6 } else { 5 };
    pdf.extend_from_slice(format!("xref\n0 {}\n", size).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj3_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
    if let Some(obj5_offset) = obj5_offset {
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj5_offset).as_bytes());
    }

    let trailer = if obj5_offset.is_some() {
        format!("trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\n", size)
    } else {
        format!("trailer\n<< /Size {} /Root 1 0 R >>\n", size)
    };
    pdf.extend_from_slice(trailer.as_bytes());
    pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    pdf
}

#[test]
fn reads_document_info_fields() {
    let pdf = build_pdf_with_info(Some(
        "/Title (Hello) /Author (Alice) /Subject (S) /Keywords (k1 k2) /Creator (c) /Producer (p) /CreationDate (D:20260506120000) /ModDate (D:20260506121000)",
    ));
    let doc = Document::load_from_bytes(&pdf).unwrap();

    let info = doc.document_info();
    assert_eq!(info.title().as_deref(), Some("Hello"));
    assert_eq!(info.author().as_deref(), Some("Alice"));
    assert_eq!(info.subject().as_deref(), Some("S"));
    assert_eq!(info.keywords().as_deref(), Some("k1 k2"));
    assert_eq!(info.creator().as_deref(), Some("c"));
    assert_eq!(info.producer().as_deref(), Some("p"));
    assert_eq!(info.creation_date().as_deref(), Some("D:20260506120000"));
    assert_eq!(info.mod_date().as_deref(), Some("D:20260506121000"));
}

#[test]
fn creates_info_dict_when_missing_and_roundtrips() {
    let pdf = build_pdf_with_info(None);
    let mut doc = Document::load_from_bytes(&pdf).unwrap();

    {
        let mut info = doc.document_info_mut().unwrap();
        info.set_title("New Title").unwrap();
        info.set_author("Bob").unwrap();
    }

    let mut buf = Cursor::new(Vec::new());
    doc.save_to(&mut buf).unwrap();
    let saved = buf.into_inner();

    let reloaded = Document::load_from_bytes(&saved).unwrap();
    let info = reloaded.document_info();
    assert_eq!(info.title().as_deref(), Some("New Title"));
    assert_eq!(info.author().as_deref(), Some("Bob"));
}

