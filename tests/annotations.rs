#![cfg(feature = "annotations")]

use rust_pdfbox::Document;

fn build_pdf_with_annotation() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 5 0 R /Annots [4 0 R] >>\nendobj\n",
    );

    let obj4_offset = pdf.len();
    pdf.extend_from_slice(
        b"4 0 obj\n<< /Type /Annot /Subtype /Text /Rect [10 20 30 40] /Contents (Note) /Name /Comment /F 4 /C [1 0 0] /CA 0.5 >>\nendobj\n",
    );

    let content = b"BT ET";
    let obj5_offset = pdf.len();
    pdf.extend_from_slice(
        format!("5 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes(),
    );
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj3_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj5_offset).as_bytes());

    pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    pdf
}

#[test]
fn reads_page_annotations() {
    let pdf = build_pdf_with_annotation();
    let doc = Document::load_from_bytes(&pdf).unwrap();
    let pages = doc.pages().unwrap();
    let page = pages.get(0).unwrap();

    let annots = page.annotations(&doc).unwrap();
    assert_eq!(annots.len(), 1);

    let annot = &annots[0];
    assert_eq!(annot.subtype, "Text");
    assert_eq!(annot.contents.as_deref(), Some("Note"));
    assert_eq!(annot.name.as_deref(), Some("Comment"));
    assert_eq!(annot.flags, Some(4));
    assert_eq!(annot.opacity, Some(0.5));
    assert_eq!(annot.color, Some([1.0, 0.0, 0.0]));
    assert_eq!(annot.rect.lower_left_x, 10.0);
    assert_eq!(annot.rect.lower_left_y, 20.0);
    assert_eq!(annot.rect.upper_right_x, 30.0);
    assert_eq!(annot.rect.upper_right_y, 40.0);
}

