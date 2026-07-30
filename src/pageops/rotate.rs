use crate::{Document, PdfResult};
use crate::cos::{CosName, CosObject};

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
    let new_rotation = if new_rotation < 0 { new_rotation + 360 } else { new_rotation };
    
    let page_id = page.id;
    doc.mutate_object(page_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.insert(CosName::new(b"Rotate".to_vec()), CosObject::Integer(new_rotation));
        }
    });
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};

    fn minimal_doc() -> Document {
        let catalog_id = ObjectId::new(1, 0);
        let pages_id = ObjectId::new(2, 0);
        let page_id = ObjectId::new(3, 0);
        let content = b"BT /F1 12 Tf (Hello) Tj ET";
        let content_id = ObjectId::new(4, 0);

        let mut doc = Document::empty();
        doc.insert_object(catalog_id, CosObject::Dictionary({
            let mut d = CosDictionary::new();
            d.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Catalog".to_vec())));
            d.insert(CosName::new(b"Pages".to_vec()), CosObject::Reference(pages_id));
            d
        }));
        doc.insert_object(pages_id, CosObject::Dictionary({
            let mut d = CosDictionary::new();
            d.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Pages".to_vec())));
            d.insert(CosName::new(b"Count".to_vec()), CosObject::Integer(1));
            d.insert(CosName::new(b"Kids".to_vec()), CosObject::Array(vec![CosObject::Reference(page_id)]));
            d
        }));
        doc.insert_object(page_id, CosObject::Dictionary({
            let mut d = CosDictionary::new();
            d.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Page".to_vec())));
            d.insert(CosName::new(b"Parent".to_vec()), CosObject::Reference(pages_id));
            d.insert(CosName::new(b"MediaBox".to_vec()), CosObject::Array(vec![
                CosObject::Integer(0), CosObject::Integer(0),
                CosObject::Integer(612), CosObject::Integer(792),
            ]));
            d.insert(CosName::contents(), CosObject::Reference(content_id));
            d.insert(CosName::new(b"Rotate".to_vec()), CosObject::Integer(0));
            d
        }));
        doc.insert_object(content_id, CosObject::Stream(crate::cos::CosStream::new(
            CosDictionary::new(), content.to_vec(),
        )));
        doc.xref.trailer.insert(CosName::new(b"Root".to_vec()), CosObject::Reference(catalog_id));
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
}

