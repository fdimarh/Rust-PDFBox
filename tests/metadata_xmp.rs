#![cfg(feature = "metadata")]

use rust_pdfbox::metadata::SyncPolicy;
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
fn reads_extended_xmp_fields() {
    let xmp = br#"<?xpacket begin='\uFEFF'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/'>
  <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'
           xmlns:dc='http://purl.org/dc/elements/1.1/'
           xmlns:xmp='http://ns.adobe.com/xap/1.0/'
           xmlns:pdf='http://ns.adobe.com/pdf/1.3/'>
    <rdf:Description>
      <dc:subject><rdf:Bag><rdf:li>Subject</rdf:li></rdf:Bag></dc:subject>
      <pdf:Keywords>k1,k2</pdf:Keywords>
      <xmp:CreatorTool>Tool</xmp:CreatorTool>
      <pdf:Producer>Producer</pdf:Producer>
      <xmp:CreateDate>2026-05-06T12:00:00Z</xmp:CreateDate>
      <xmp:ModifyDate>2026-05-06T12:30:00Z</xmp:ModifyDate>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end='w'?>"#;

    let pdf = build_pdf_with_xmp(Some(xmp));
    let doc = Document::load_from_bytes(&pdf).unwrap();

    let meta = doc.xmp_metadata().expect("metadata should exist");
    assert_eq!(meta.dc_subject(), Some("Subject"));
    assert_eq!(meta.pdf_keywords(), Some("k1,k2"));
    assert_eq!(meta.xmp_creator_tool(), Some("Tool"));
    assert_eq!(meta.pdf_producer(), Some("Producer"));
    assert_eq!(meta.xmp_create_date(), Some("2026-05-06T12:00:00Z"));
    assert_eq!(meta.xmp_modify_date(), Some("2026-05-06T12:30:00Z"));
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

#[test]
fn set_xmp_metadata_raw_roundtrip() {
    let pdf = build_pdf_with_xmp(None);
    let mut doc = Document::load_from_bytes(&pdf).unwrap();

    let xml = "<x:xmpmeta xmlns:x='adobe:ns:meta/'><dc:title xmlns:dc='http://purl.org/dc/elements/1.1/'>Raw</dc:title></x:xmpmeta>";
    doc.set_xmp_metadata_raw(xml).unwrap();

    let mut out = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut out).unwrap();
    let reloaded = Document::load_from_bytes(&out.into_inner()).unwrap();

    let meta = reloaded.xmp_metadata().unwrap();
    assert!(meta.raw_xml().contains("xmpmeta"));
}

#[test]
fn sync_docinfo_to_xmp_writes_title_and_author() {
    let pdf = build_pdf_with_xmp(None);
    let mut doc = Document::load_from_bytes(&pdf).unwrap();

    {
        let mut info = doc.document_info_mut().unwrap();
        info.set_title("Synced Title").unwrap();
        info.set_author("Synced Author").unwrap();
    }
    doc.sync_docinfo_to_xmp().unwrap();

    let mut out = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut out).unwrap();
    let reloaded = Document::load_from_bytes(&out.into_inner()).unwrap();

    let meta = reloaded.xmp_metadata().unwrap();
    assert_eq!(meta.dc_title(), Some("Synced Title"));
    assert_eq!(meta.dc_creator(), Some("Synced Author"));
}

#[test]
fn sync_xmp_to_docinfo_populates_fields() {
    let xmp = br#"<?xpacket begin='\uFEFF'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/'>
  <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'
           xmlns:dc='http://purl.org/dc/elements/1.1/'
           xmlns:xmp='http://ns.adobe.com/xap/1.0/'
           xmlns:pdf='http://ns.adobe.com/pdf/1.3/'>
    <rdf:Description>
      <dc:title><rdf:Alt><rdf:li xml:lang='x-default'>Title</rdf:li></rdf:Alt></dc:title>
      <dc:creator><rdf:Seq><rdf:li>Author</rdf:li></rdf:Seq></dc:creator>
      <dc:subject><rdf:Bag><rdf:li>Subject</rdf:li></rdf:Bag></dc:subject>
      <pdf:Keywords>k1 k2</pdf:Keywords>
      <xmp:CreatorTool>Tool</xmp:CreatorTool>
      <pdf:Producer>Producer</pdf:Producer>
      <xmp:CreateDate>2026-05-06T12:00:00Z</xmp:CreateDate>
      <xmp:ModifyDate>2026-05-06T12:30:00Z</xmp:ModifyDate>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end='w'?>"#;

    let pdf = build_pdf_with_xmp(Some(xmp));
    let mut doc = Document::load_from_bytes(&pdf).unwrap();
    doc.sync_xmp_to_docinfo_with(SyncPolicy::all_fields()).unwrap();

    let info = doc.document_info();
    assert_eq!(info.title().as_deref(), Some("Title"));
    assert_eq!(info.author().as_deref(), Some("Author"));
    assert_eq!(info.subject().as_deref(), Some("Subject"));
    assert_eq!(info.keywords().as_deref(), Some("k1 k2"));
    assert_eq!(info.creator().as_deref(), Some("Tool"));
    assert_eq!(info.producer().as_deref(), Some("Producer"));
    assert_eq!(info.creation_date().as_deref(), Some("2026-05-06T12:00:00Z"));
    assert_eq!(info.mod_date().as_deref(), Some("2026-05-06T12:30:00Z"));
}

