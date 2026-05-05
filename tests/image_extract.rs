#![cfg(feature = "image-extract")]

use rust_pdfbox::Document;

fn build_single_image_pdf(image_dict_extra: &str, image_data: &[u8], content_stream: &[u8]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n",
    );

    let obj4_offset = pdf.len();
    pdf.extend_from_slice(
        format!("4 0 obj\n<< /Length {} >>\nstream\n", content_stream.len()).as_bytes(),
    );
    pdf.extend_from_slice(content_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let obj5_offset = pdf.len();
    pdf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB {} /Length {} >>\nstream\n",
            image_dict_extra,
            image_data.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(image_data);
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

fn make_stored_zlib(data: &[u8]) -> Vec<u8> {
    let cmf: u8 = 0x78;
    let rem = (cmf as u16 * 256) % 31;
    let flg: u8 = if rem == 0 { 0x01 } else { (31 - rem) as u8 };

    let mut out = vec![cmf, flg];
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)

    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);

    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in data {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    out.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
    out
}

#[test]
fn extract_dct_image_metadata_and_encoded_bytes() {
    let jpeg_like = vec![0xFF, 0xD8, 0xFF, 0xD9];
    let pdf = build_single_image_pdf("/Filter /DCTDecode", &jpeg_like, b"/Im1 Do");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();

    assert_eq!(images.len(), 1);
    let img = &images[0];
    assert_eq!(img.resource_name(), "Im1");
    assert_eq!(img.width(), 2);
    assert_eq!(img.height(), 1);
    assert_eq!(img.bits_per_component(), 8);
    assert_eq!(img.color_space(), Some("DeviceRGB"));
    assert_eq!(img.filter_names(), ["DCTDecode".to_string()]);
    assert_eq!(img.encoded_bytes(), jpeg_like.as_slice());
    assert!(img.decode_pixels().is_err());
}

#[test]
fn extract_flate_image_decodes_pixels() {
    let rgb = vec![255, 0, 0, 0, 255, 0]; // 2x1 RGB
    let compressed = make_stored_zlib(&rgb);
    let pdf = build_single_image_pdf("/Filter /FlateDecode", &compressed, b"q /Im1 Do Q");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();

    assert_eq!(images.len(), 1);
    let decoded = images[0].decode_pixels().unwrap();
    assert_eq!(decoded, rgb);
}

#[test]
fn extract_images_returns_empty_when_page_has_no_image_do() {
    let rgb = vec![10, 20, 30, 40, 50, 60];
    let pdf = build_single_image_pdf("", &rgb, b"BT ET");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();

    assert!(images.is_empty());
}

