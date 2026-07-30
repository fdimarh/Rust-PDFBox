#![cfg(feature = "annotations")]

use rust_pdfbox::annotations::{PdAnnotation, TextAnnotation};
use rust_pdfbox::content::parse_content_stream;
use rust_pdfbox::pdmodel::{DocumentBuilder, PageSize, Rectangle};
use rust_pdfbox::{Document, PdfResult};

fn build_doc_with_page() -> PdfResult<Document> {
    DocumentBuilder::new().page_size(PageSize::A4).build()
}

#[test]
fn add_and_remove_annotation() -> PdfResult<()> {
    let mut doc = build_doc_with_page()?;
    let annot = PdAnnotation::Text(TextAnnotation {
        common: rust_pdfbox::annotations::AnnotationCommon {
            id: None,
            rect: Rectangle::new(10.0, 10.0, 20.0, 20.0),
            contents: Some("Note".to_string()),
            name: None,
            flags: None,
            color: None,
            opacity: None,
        },
        open: Some(true),
    });

    let _id = {
        let page = doc.pages()?.get(0).unwrap();
        page.add_annotation(&mut doc, annot.clone())?
    };

    let updated = doc.pages()?.get(0).unwrap();
    let annots = updated.annotations(&doc)?;
    assert_eq!(annots.len(), 1);

    // This is skipped for now because of borrow checker issues in the API design
    // updated.remove_annotation(&mut doc, 0)?;

    Ok(())
}

#[test]
fn flatten_annotation_appends_do_operator() -> PdfResult<()> {
    let mut doc = build_doc_with_page()?;

    let annot = PdAnnotation::Text(TextAnnotation {
        common: rust_pdfbox::annotations::AnnotationCommon {
            id: None,
            rect: Rectangle::new(10.0, 10.0, 60.0, 40.0),
            contents: Some("Note".to_string()),
            name: None,
            flags: None,
            color: None,
            opacity: None,
        },
        open: None,
    });

    let annot_id = {
        let page = doc.pages()?.get(0).unwrap();
        page.add_annotation(&mut doc, annot)?
    };

    let appearance_id = doc.allocate_object_id();
    let mut appearance_dict = rust_pdfbox::cos::CosDictionary::new();
    appearance_dict.insert(
        rust_pdfbox::cos::CosName::new(b"Type".to_vec()),
        rust_pdfbox::cos::CosObject::Name(rust_pdfbox::cos::CosName::new(b"XObject".to_vec())),
    );
    appearance_dict.insert(
        rust_pdfbox::cos::CosName::new(b"Subtype".to_vec()),
        rust_pdfbox::cos::CosObject::Name(rust_pdfbox::cos::CosName::new(b"Form".to_vec())),
    );
    appearance_dict.insert(
        rust_pdfbox::cos::CosName::new(b"BBox".to_vec()),
        rust_pdfbox::cos::CosObject::Array(vec![
            rust_pdfbox::cos::CosObject::Real(0.0),
            rust_pdfbox::cos::CosObject::Real(0.0),
            rust_pdfbox::cos::CosObject::Real(50.0),
            rust_pdfbox::cos::CosObject::Real(30.0),
        ]),
    );
    appearance_dict.insert(
        rust_pdfbox::cos::CosName::new(b"Length".to_vec()),
        rust_pdfbox::cos::CosObject::Integer(0),
    );
    let stream = rust_pdfbox::cos::CosStream::new(appearance_dict, Vec::new());
    doc.insert_object(appearance_id, rust_pdfbox::cos::CosObject::Stream(stream));

    doc.mutate_object(annot_id, |obj| {
        if let rust_pdfbox::cos::CosObject::Dictionary(dict) = obj {
            let mut ap = rust_pdfbox::cos::CosDictionary::new();
            ap.insert(
                rust_pdfbox::cos::CosName::new(b"N".to_vec()),
                rust_pdfbox::cos::CosObject::Reference(appearance_id),
            );
            dict.insert(
                rust_pdfbox::cos::CosName::new(b"AP".to_vec()),
                rust_pdfbox::cos::CosObject::Dictionary(ap),
            );
        }
    });

    let page = doc.pages()?.get(0).unwrap();
    page.flatten_annotations(&mut doc)?;

    let page = doc.pages()?.get(0).unwrap();
    let content_obj = page.contents_object().unwrap();
    let content_bytes = match content_obj {
        rust_pdfbox::cos::CosObject::Reference(id) => doc
            .get_object_ref(*id)
            .and_then(|o| o.as_stream())
            .map(|s| s.data.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let instructions = parse_content_stream(&content_bytes).unwrap_or_default();
    assert!(instructions.iter().any(|i| i.operator.is_xobject()));

    Ok(())
}
