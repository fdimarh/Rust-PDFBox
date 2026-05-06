#![cfg(feature = "metadata")]

use rust_pdfbox::Document;

fn build_pdf_with_xmp(xmp_xml: Option<&[u8]>) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    if xmp_xml.is_some() {
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Metadata 5 0 R >>\nendobj\n");
    } else {
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    }

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
    if let Some(xmp_xml) = xmp_xml {
        obj5_offset = Some(pdf.len());
        pdf.extend_from_slice(
            format!(
                "5 0 obj\n<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n",
                xmp_xml.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(xmp_xml);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
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

    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\n", size).as_bytes());
    pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    pdf
}

#[test]
fn reads_xmp_title_and_creator() {
    let xmp = br#"<?xpacket begin='\uFEFF'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/'>
  <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'
           xmlns:dc='http://purl.org/dc/elements/1.1/'>
    <rdf:Description>
      <dc:title><rdf:Alt><rdf:li xml:lang='x-default'>Doc &amp; Title</rdf:li></rdf:Alt></dc:title>
      <dc:creator><rdf:Seq><rdf:li>Alice</rdf:li></rdf:Seq></dc:creator>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end='w'?>"#;

    let pdf = build_pdf_with_xmp(Some(xmp));
    let doc = Document::load_from_bytes(&pdf).unwrap();

    let meta = doc.xmp_metadata().expect("metadata should exist");
    assert!(meta.raw_xml().contains("xmpmeta"));
    assert_eq!(meta.dc_title(), Some("Doc & Title"));
    assert_eq!(meta.dc_creator(), Some("Alice"));
}

#[test]
fn missing_xmp_returns_none() {
    let pdf = build_pdf_with_xmp(None);
    let doc = Document::load_from_bytes(&pdf).unwrap();
    assert!(doc.xmp_metadata().is_none());
}

#[test]
fn malformed_xmp_is_tolerated() {
    let xmp = b"<x:xmpmeta><rdf:RDF><dc:title>broken";
    let pdf = build_pdf_with_xmp(Some(xmp));
    let doc = Document::load_from_bytes(&pdf).unwrap();

    let meta = doc.xmp_metadata().expect("raw metadata stream should be readable");
    assert_eq!(meta.dc_title(), None);
}

