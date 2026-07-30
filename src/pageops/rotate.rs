use crate::cos::{CosName, CosObject};
use crate::{Document, PdfResult};

/// Rotates a page by the given number of degrees.
/// The `degrees` should be a multiple of 90.
///
/// Modifies the document directly.
pub fn rotate_page(doc: &mut Document, page_index: usize, degrees: i64) -> PdfResult<()> {
    let tree = doc.pages()?;
    let page = tree.get(page_index).ok_or_else(|| crate::PdfError::Parse {
        offset: None,
        context: format!("page index out of bounds: {}", page_index),
    })?;

    let current_rotation = page.rotation();
    let new_rotation = (current_rotation + degrees) % 360;
    let new_rotation = if new_rotation < 0 {
        new_rotation + 360
    } else {
        new_rotation
    };

    let page_id = page.id;
    doc.mutate_object(page_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.insert(
                CosName::new(b"Rotate".to_vec()),
                CosObject::Integer(new_rotation),
            );
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};

    /// Build a raw 2-page minimal PDF byte buffer (no compressed streams).
    /// Closely mirrors the pattern used in editor.rs.
    fn minimal_pdf_bytes() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();

        let o1 = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let o2 = pdf.len();
        pdf.extend_from_slice(
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>\nendobj\n",
        );

        let o3 = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate 0 >>\nendobj\n",
        );

        let o4 = pdf.len();
        pdf.extend_from_slice(
            b"4 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate 0 >>\nendobj\n",
        );

        let xref = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n");
        pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", o1).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", o2).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", o3).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", o4).as_bytes());

        pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{}\n%%EOF\n", xref).as_bytes());
        pdf
    }

    fn minimal_doc() -> Document {
        let catalog_id = ObjectId::new(1, 0);
        let pages_id = ObjectId::new(2, 0);
        let page_id = ObjectId::new(3, 0);
        let content = b"BT /F1 12 Tf (Hello) Tj ET";
        let content_id = ObjectId::new(4, 0);

        let mut doc = Document::empty();
        doc.insert_object(
            catalog_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Catalog".to_vec())),
                );
                d.insert(
                    CosName::new(b"Pages".to_vec()),
                    CosObject::Reference(pages_id),
                );
                d
            }),
        );
        doc.insert_object(
            pages_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Pages".to_vec())),
                );
                d.insert(CosName::new(b"Count".to_vec()), CosObject::Integer(1));
                d.insert(
                    CosName::new(b"Kids".to_vec()),
                    CosObject::Array(vec![CosObject::Reference(page_id)]),
                );
                d
            }),
        );
        doc.insert_object(
            page_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Page".to_vec())),
                );
                d.insert(
                    CosName::new(b"Parent".to_vec()),
                    CosObject::Reference(pages_id),
                );
                d.insert(
                    CosName::new(b"MediaBox".to_vec()),
                    CosObject::Array(vec![
                        CosObject::Integer(0),
                        CosObject::Integer(0),
                        CosObject::Integer(612),
                        CosObject::Integer(792),
                    ]),
                );
                d.insert(CosName::contents(), CosObject::Reference(content_id));
                d.insert(CosName::new(b"Rotate".to_vec()), CosObject::Integer(0));
                d
            }),
        );
        doc.insert_object(
            content_id,
            CosObject::Stream(crate::cos::CosStream::new(
                CosDictionary::new(),
                content.to_vec(),
            )),
        );
        doc.xref.trailer.insert(
            CosName::new(b"Root".to_vec()),
            CosObject::Reference(catalog_id),
        );
        doc
    }

    #[test]
    fn test_rotate_page_90() {
        let mut doc = minimal_doc();
        rotate_page(&mut doc, 0, 90).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 90);
    }

    #[test]
    fn test_rotate_page_360_wraps() {
        let mut doc = minimal_doc();
        rotate_page(&mut doc, 0, 360).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 0);
    }

    #[test]
    fn test_rotate_page_negative() {
        let mut doc = minimal_doc();
        rotate_page(&mut doc, 0, -90).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 270);
    }

    #[test]
    fn test_rotate_page_cumulative() {
        let mut doc = minimal_doc();
        rotate_page(&mut doc, 0, 90).unwrap();
        rotate_page(&mut doc, 0, 180).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 270);
    }

    // ── load_from_bytes-based tests ──────────────────────────────────────

    #[test]
    fn test_rotate_from_bytes_90() {
        let mut doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        rotate_page(&mut doc, 0, 90).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 90);
        // Second page was initial-0, still 0
        assert_eq!(tree.get(1).unwrap().rotation(), 0);
    }

    #[test]
    fn test_rotate_from_bytes_360_wraps() {
        let mut doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        rotate_page(&mut doc, 0, 360).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 0);
    }

    #[test]
    fn test_rotate_from_bytes_multiple_pages() {
        let mut doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        rotate_page(&mut doc, 0, 90).unwrap();
        rotate_page(&mut doc, 1, 180).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 90);
        assert_eq!(tree.get(1).unwrap().rotation(), 180);
    }

    #[test]
    fn test_rotate_from_bytes_negative() {
        let mut doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        rotate_page(&mut doc, 1, -90).unwrap();
        let tree = doc.pages().unwrap();
        assert_eq!(tree.get(1).unwrap().rotation(), 270);
    }

    #[test]
    fn test_rotate_from_bytes_cumulative() {
        let mut doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        rotate_page(&mut doc, 1, 90).unwrap();
        rotate_page(&mut doc, 1, 180).unwrap();
        rotate_page(&mut doc, 1, 45).unwrap();
        let tree = doc.pages().unwrap();
        // (0 + 90 + 180 + 45) % 360 = 315
        assert_eq!(tree.get(1).unwrap().rotation(), 315);
    }

    #[test]
    fn test_rotate_from_bytes_out_of_bounds() {
        let mut doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        let result = rotate_page(&mut doc, 99, 90);
        assert!(result.is_err());
    }

    #[test]
    fn test_rotate_from_bytes_save_and_reload() {
        let mut doc = Document::load_from_bytes(&minimal_pdf_bytes()).unwrap();
        rotate_page(&mut doc, 0, 270).unwrap();
        rotate_page(&mut doc, 1, 90).unwrap();

        // Save to buffer and reload
        let mut buf = std::io::Cursor::new(Vec::new());
        doc.save_to(&mut buf).unwrap();
        let reloaded = Document::load_from_bytes(buf.get_ref()).unwrap();
        let tree = reloaded.pages().unwrap();
        assert_eq!(tree.get(0).unwrap().rotation(), 270);
        assert_eq!(tree.get(1).unwrap().rotation(), 90);
        assert_eq!(reloaded.page_count(), 2);
    }
}
