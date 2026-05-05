#![cfg(feature = "forms")]

use rust_pdfbox::Document;

fn make_pdf_with_single_stream_xfa() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let obj4_offset = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n<< /Fields [5 0 R] /XFA 6 0 R >>\nendobj\n");

    let obj5_offset = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n<< /FT /Tx /T <6e616d65> /V <416c696365> >>\nendobj\n");

    let xfa_payload = b"<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><xfa/></xdp:xdp>";
    let obj6_offset = pdf.len();
    pdf.extend_from_slice(format!("6 0 obj\n<< /Length {} >>\nstream\n", xfa_payload.len()).as_bytes());
    pdf.extend_from_slice(xfa_payload);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n"); // obj 3 free
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj5_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj6_offset).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 7 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    pdf
}

fn make_pdf_with_packet_array_xfa() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

    let obj4_offset = pdf.len();
    pdf.extend_from_slice(
        b"4 0 obj\n<< /Fields [5 0 R] /XFA [<74656d706c617465> 6 0 R <6461746173657473> 7 0 R] >>\nendobj\n",
    );

    let obj5_offset = pdf.len();
    pdf.extend_from_slice(b"5 0 obj\n<< /FT /Tx /T <6e616d65> /V <426f62> >>\nendobj\n");

    let template_payload = b"<template/>";
    let obj6_offset = pdf.len();
    pdf.extend_from_slice(format!("6 0 obj\n<< /Length {} >>\nstream\n", template_payload.len()).as_bytes());
    pdf.extend_from_slice(template_payload);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let datasets_payload = b"<datasets><name>Bob</name></datasets>";
    let obj7_offset = pdf.len();
    pdf.extend_from_slice(format!("7 0 obj\n<< /Length {} >>\nstream\n", datasets_payload.len()).as_bytes());
    pdf.extend_from_slice(datasets_payload);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 8\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n"); // obj 3 free
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj5_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj6_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj7_offset).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 8 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    pdf
}

#[test]
fn single_stream_xfa_is_detected_and_readable() {
    let bytes = make_pdf_with_single_stream_xfa();
    let doc = Document::load_from_bytes(&bytes).unwrap();

    assert!(doc.has_xfa_form());

    let form = doc.acro_form().unwrap();
    assert!(form.has_xfa());
    assert!(form.is_hybrid_xfa());

    let xfa = doc.xfa_form().unwrap();
    assert_eq!(xfa.packets().len(), 1);
    assert!(xfa.raw_xml().starts_with(b"<xdp:xdp"));
}

#[test]
fn packet_array_xfa_supports_named_packet_lookup() {
    let bytes = make_pdf_with_packet_array_xfa();
    let doc = Document::load_from_bytes(&bytes).unwrap();

    let xfa = doc.xfa_form().unwrap();
    assert_eq!(xfa.packet_names(), vec!["template", "datasets"]);
    assert_eq!(xfa.packet("template").unwrap().xml(), b"<template/>");
    assert_eq!(
        xfa.datasets_xml(),
        Some(b"<datasets><name>Bob</name></datasets>".as_slice())
    );
}

