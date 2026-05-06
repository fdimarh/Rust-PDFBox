#![cfg(feature = "image-extract")]

use rust_pdfbox::Document;
use rust_pdfbox::PdfError;
use rust_pdfbox::image_extract::ImageExportFormat;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_output_path(ext: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("rust_pdfbox_image_extract_{pid}_{nonce}_{seq}.{ext}"))
}

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

fn build_iccbased_image_pdf(
    image_data: &[u8],
    content_stream: &[u8],
    icc_profile_dict_body: &str,
) -> Vec<u8> {
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
            "5 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /BitsPerComponent 8 /ColorSpace [/ICCBased 6 0 R] /Filter /FlateDecode /Length {} >>\nstream\n",
            image_data.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(image_data);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let obj6_offset = pdf.len();
    let icc_stream = Vec::<u8>::new();
    pdf.extend_from_slice(
        format!("6 0 obj\n<< {} /Length {} >>\nstream\n", icc_profile_dict_body, icc_stream.len()).as_bytes(),
    );
    pdf.extend_from_slice(&icc_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj3_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj5_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj6_offset).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 7 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    pdf
}

fn build_single_image_with_smask_pdf(
    image_dict_extra: &str,
    image_data: &[u8],
    smask_data: &[u8],
) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n",
    );

    let content_stream = b"/Im1 Do";
    let obj4_offset = pdf.len();
    pdf.extend_from_slice(
        format!("4 0 obj\n<< /Length {} >>\nstream\n", content_stream.len()).as_bytes(),
    );
    pdf.extend_from_slice(content_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let obj5_offset = pdf.len();
    pdf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB {} /Filter /FlateDecode /SMask 6 0 R /Length {} >>\nstream\n",
            image_dict_extra,
            image_data.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(image_data);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let obj6_offset = pdf.len();
    pdf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /FlateDecode /Length {} >>\nstream\n",
            smask_data.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(smask_data);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj3_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj5_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj6_offset).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 7 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref_offset).as_bytes());

    pdf
}

fn build_inline_only_pdf(content_stream: &[u8]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();

    let obj1_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2_offset = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let obj3_offset = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R >>\nendobj\n",
    );

    let obj4_offset = pdf.len();
    pdf.extend_from_slice(
        format!("4 0 obj\n<< /Length {} >>\nstream\n", content_stream.len()).as_bytes(),
    );
    pdf.extend_from_slice(content_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj1_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj2_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj3_offset).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", obj4_offset).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
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

#[test]
fn export_png_writes_decoded_pixels() {
    let rgb = vec![255, 0, 0, 0, 255, 0]; // 2x1 RGB
    let compressed = make_stored_zlib(&rgb);
    let pdf = build_single_image_pdf("/Filter /FlateDecode", &compressed, b"/Im1 Do");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    images[0].save_as(&out, ImageExportFormat::Png).unwrap();

    let saved = image::open(&out).unwrap().to_rgb8();
    assert_eq!(saved.width(), 2);
    assert_eq!(saved.height(), 1);
    assert_eq!(saved.into_raw(), rgb);

    let _ = std::fs::remove_file(out);
}

#[test]
fn export_tiff_writes_decoded_pixels() {
    let rgb = vec![255, 0, 0, 0, 255, 0];
    let compressed = make_stored_zlib(&rgb);
    let pdf = build_single_image_pdf("/Filter /FlateDecode", &compressed, b"/Im1 Do");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("tiff");

    images[0].save_as(&out, ImageExportFormat::Tiff).unwrap();
    let saved = image::open(&out).unwrap().to_rgb8();
    assert_eq!(saved.width(), 2);
    assert_eq!(saved.height(), 1);
    assert_eq!(saved.into_raw(), rgb);

    let _ = std::fs::remove_file(out);
}

#[test]
fn export_jpeg_passthrough_writes_original_encoded_bytes() {
    let jpeg_like = vec![0xFF, 0xD8, 0x11, 0x22, 0x33, 0xFF, 0xD9];
    let pdf = build_single_image_pdf("/Filter /DCTDecode", &jpeg_like, b"/Im1 Do");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("jpg");

    images[0].save_as(&out, ImageExportFormat::Jpeg).unwrap();

    let saved = std::fs::read(&out).unwrap();
    assert_eq!(saved, jpeg_like);

    let _ = std::fs::remove_file(out);
}

#[test]
fn export_png_from_dct_image_is_not_supported() {
    let jpeg_like = vec![0xFF, 0xD8, 0xFF, 0xD9];
    let pdf = build_single_image_pdf("/Filter /DCTDecode", &jpeg_like, b"/Im1 Do");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    let err = images[0].save_as(&out, ImageExportFormat::Png).unwrap_err();
    assert!(matches!(err, PdfError::Unsupported { .. }));
}

#[test]
fn extract_inline_image_and_decode_pixels() {
    let inline_content = b"q BI /W 2 /H 1 /BPC 8 /CS /RGB ID \xFF\x00\x00\x00\xFF\x00 EI Q";
    let pdf = build_inline_only_pdf(inline_content);

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();

    assert_eq!(images.len(), 1);
    let img = &images[0];
    assert_eq!(img.resource_name(), "inline_1");
    assert_eq!(img.width(), 2);
    assert_eq!(img.height(), 1);
    assert_eq!(img.color_space(), Some("DeviceRGB"));
    assert!(img.filter_names().is_empty());
    assert_eq!(img.decode_pixels().unwrap(), vec![255, 0, 0, 0, 255, 0]);
}

#[test]
fn export_inline_image_as_png() {
    let inline_content = b"BI /W 2 /H 1 /BPC 8 /CS /RGB ID \x00\x00\xFF\xFF\xFF\xFF EI";
    let pdf = build_inline_only_pdf(inline_content);

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    images[0].save_as(&out, ImageExportFormat::Png).unwrap();
    let saved = image::open(&out).unwrap().to_rgb8();
    assert_eq!(saved.width(), 2);
    assert_eq!(saved.height(), 1);
    assert_eq!(saved.into_raw(), vec![0, 0, 255, 255, 255, 255]);

    let _ = std::fs::remove_file(out);
}

#[test]
fn extract_indexed_image_decodes_to_rgb_pixels() {
    let indices = vec![0u8, 1u8];
    let compressed = make_stored_zlib(&indices);
    let pdf = build_single_image_pdf(
        "/ColorSpace [/Indexed /DeviceRGB 1 <FF000000FF00>] /Filter /FlateDecode",
        &compressed,
        b"/Im1 Do",
    );

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].color_space(), Some("Indexed"));
    let decoded = images[0].decode_pixels().unwrap();
    assert_eq!(decoded, vec![255, 0, 0, 0, 255, 0]);
}

#[test]
fn export_indexed_image_as_png() {
    let indices = vec![0u8, 1u8];
    let compressed = make_stored_zlib(&indices);
    let pdf = build_single_image_pdf(
        "/ColorSpace [/Indexed /DeviceRGB 1 <0000FFFFFFFF>] /Filter /FlateDecode",
        &compressed,
        b"/Im1 Do",
    );

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    images[0].save_as(&out, ImageExportFormat::Png).unwrap();
    let saved = image::open(&out).unwrap().to_rgb8();
    assert_eq!(saved.width(), 2);
    assert_eq!(saved.height(), 1);
    assert_eq!(saved.into_raw(), vec![0, 0, 255, 255, 255, 255]);

    let _ = std::fs::remove_file(out);
}

#[test]
fn export_cmyk_image_as_png() {
    // 2x1 CMYK: cyan then black
    let cmyk = vec![255, 0, 0, 0, 0, 0, 0, 255];
    let compressed = make_stored_zlib(&cmyk);
    let pdf = build_single_image_pdf(
        "/ColorSpace /DeviceCMYK /Filter /FlateDecode",
        &compressed,
        b"/Im1 Do",
    );

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    images[0].save_as(&out, ImageExportFormat::Png).unwrap();
    let saved = image::open(&out).unwrap().to_rgb8();
    assert_eq!(saved.width(), 2);
    assert_eq!(saved.height(), 1);
    assert_eq!(saved.into_raw(), vec![0, 255, 255, 0, 0, 0]);

    let _ = std::fs::remove_file(out);
}

#[test]
fn export_cmyk_dct_image_as_png_is_not_supported() {
    let jpeg_like = vec![0xFF, 0xD8, 0xFF, 0xD9];
    let pdf = build_single_image_pdf(
        "/ColorSpace /DeviceCMYK /Filter /DCTDecode",
        &jpeg_like,
        b"/Im1 Do",
    );

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    let err = images[0].save_as(&out, ImageExportFormat::Png).unwrap_err();
    assert!(matches!(err, PdfError::Unsupported { .. }));
}

#[test]
fn extract_iccbased_alternate_rgb_decodes_pixels() {
    let rgb = vec![255, 0, 0, 0, 255, 0];
    let compressed = make_stored_zlib(&rgb);
    let pdf = build_iccbased_image_pdf(&compressed, b"/Im1 Do", "/N 3 /Alternate /DeviceRGB");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].color_space(), Some("ICCBased"));
    assert_eq!(images[0].decode_pixels().unwrap(), rgb);
}

#[test]
fn export_png_with_smask_alpha() {
    let rgb = vec![255, 0, 0, 0, 255, 0];
    let alpha = vec![0u8, 255u8];
    let rgb_compressed = make_stored_zlib(&rgb);
    let alpha_compressed = make_stored_zlib(&alpha);
    let pdf = build_single_image_with_smask_pdf("", &rgb_compressed, &alpha_compressed);

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    images[0].save_as(&out, ImageExportFormat::Png).unwrap();
    let saved = image::open(&out).unwrap().to_rgba8();
    assert_eq!(saved.width(), 2);
    assert_eq!(saved.height(), 1);
    assert_eq!(saved.into_raw(), vec![255, 0, 0, 0, 0, 255, 0, 255]);

    let _ = std::fs::remove_file(out);
}

#[test]
fn export_iccbased_alternate_cmyk_as_png() {
    // 2x1 CMYK: cyan then black
    let cmyk = vec![255, 0, 0, 0, 0, 0, 0, 255];
    let compressed = make_stored_zlib(&cmyk);
    let pdf = build_iccbased_image_pdf(&compressed, b"/Im1 Do", "/N 4 /Alternate /DeviceCMYK");

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let out = temp_output_path("png");

    images[0].save_as(&out, ImageExportFormat::Png).unwrap();
    let saved = image::open(&out).unwrap().to_rgb8();
    assert_eq!(saved.width(), 2);
    assert_eq!(saved.height(), 1);
    assert_eq!(saved.into_raw(), vec![0, 255, 255, 0, 0, 0]);

    let _ = std::fs::remove_file(out);
}

#[test]
fn extract_indexed_non_rgb_base_is_rejected() {
    let indices = vec![0u8, 1u8];
    let compressed = make_stored_zlib(&indices);
    let pdf = build_single_image_pdf(
        "/ColorSpace [/Indexed /DeviceGray 1 <00FF>] /Filter /FlateDecode",
        &compressed,
        b"/Im1 Do",
    );

    let doc = Document::load_from_bytes(&pdf).unwrap();
    let images = doc.extract_images(0).unwrap();
    let err = images[0].decode_pixels().unwrap_err();
    assert!(matches!(err, PdfError::Unsupported { .. }));
}

