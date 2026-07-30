use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::parser::xref::XRefEntry;
use crate::{Document, PdfResult};

#[derive(Debug, Clone, Copy)]
pub enum PageSize {
    A4,
    Letter,
    /// Custom width and height in user units
    Custom(f64, f64),
}

impl PageSize {
    pub fn dimensions(&self) -> (f64, f64) {
        match self {
            Self::A4 => (595.28, 841.89),   // 210 x 297 mm
            Self::Letter => (612.0, 792.0), // 8.5 x 11 inches
            Self::Custom(w, h) => (*w, *h),
        }
    }
}

pub struct DocumentBuilder {
    page_size: PageSize,
}

impl DocumentBuilder {
    pub fn new() -> Self {
        Self {
            page_size: PageSize::A4,
        }
    }

    pub fn page_size(mut self, size: PageSize) -> Self {
        self.page_size = size;
        self
    }

    /// Constructs an empty Document, populating its ObjectStore with
    /// `/Catalog`, `/Pages`, and a base `/Page`.
    pub fn build(self) -> PdfResult<Document> {
        let mut doc = Document::empty();

        let catalog_id = ObjectId::new(1, 0);
        let pages_id = ObjectId::new(2, 0);
        let page_id = ObjectId::new(3, 0);

        // 1. Trailer
        doc.xref.trailer.insert(
            CosName::new(b"Size".to_vec()),
            CosObject::Integer(page_id.object_number as i64 + 1), // ids are 1, 2, 3 so size is 4
        );
        doc.xref.trailer.insert(
            CosName::new(b"Root".to_vec()),
            CosObject::Reference(catalog_id),
        );

        // 2. Catalog
        let mut catalog = CosDictionary::new();
        catalog.insert(
            CosName::type_name(),
            CosObject::Name(CosName::new(b"Catalog".to_vec())),
        );
        catalog.insert(CosName::pages(), CosObject::Reference(pages_id));
        doc.insert_object(catalog_id, CosObject::Dictionary(catalog));

        // 3. Pages
        let mut pages = CosDictionary::new();
        pages.insert(
            CosName::type_name(),
            CosObject::Name(CosName::new(b"Pages".to_vec())),
        );
        pages.insert(
            CosName::kids(),
            CosObject::Array(vec![CosObject::Reference(page_id)]),
        );
        pages.insert(CosName::count(), CosObject::Integer(1));

        // Add basic resources so fonts like Helvetica work out-of-the box
        let mut helvetica = CosDictionary::new();
        helvetica.insert(
            CosName::type_name(),
            CosObject::Name(CosName::new(b"Font".to_vec())),
        );
        helvetica.insert(
            CosName::new(b"Subtype".to_vec()),
            CosObject::Name(CosName::new(b"Type1".to_vec())),
        );
        helvetica.insert(
            CosName::new(b"BaseFont".to_vec()),
            CosObject::Name(CosName::new(b"Helvetica".to_vec())),
        );

        let mut fonts = CosDictionary::new();
        fonts.insert(
            CosName::new(b"Helvetica".to_vec()),
            CosObject::Dictionary(helvetica),
        );

        let mut resources = CosDictionary::new();
        resources.insert(CosName::new(b"Font".to_vec()), CosObject::Dictionary(fonts));

        pages.insert(
            CosName::new(b"Resources".to_vec()),
            CosObject::Dictionary(resources),
        );

        doc.insert_object(pages_id, CosObject::Dictionary(pages));

        // 4. Page
        let mut page = CosDictionary::new();
        page.insert(
            CosName::type_name(),
            CosObject::Name(CosName::new(b"Page".to_vec())),
        );
        page.insert(
            CosName::new(b"Parent".to_vec()),
            CosObject::Reference(pages_id),
        );
        let (w, h) = self.page_size.dimensions();
        page.insert(
            CosName::new(b"MediaBox".to_vec()),
            CosObject::Array(vec![
                CosObject::Real(0.0),
                CosObject::Real(0.0),
                CosObject::Real(w),
                CosObject::Real(h),
            ]),
        );
        doc.insert_object(page_id, CosObject::Dictionary(page));

        // 5. XRef Entries (InUse entries so it can be saved)
        doc.xref.insert_if_absent(
            catalog_id,
            XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );
        doc.xref.insert_if_absent(
            pages_id,
            XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );
        doc.xref.insert_if_absent(
            page_id,
            XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );

        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_a4() {
        let (w, h) = PageSize::A4.dimensions();
        assert!((w - 595.28).abs() < 0.01);
        assert!((h - 841.89).abs() < 0.01);
    }

    #[test]
    fn page_size_letter() {
        let (w, h) = PageSize::Letter.dimensions();
        assert!((w - 612.0).abs() < 0.01);
        assert!((h - 792.0).abs() < 0.01);
    }

    #[test]
    fn page_size_custom() {
        let (w, h) = PageSize::Custom(300.0, 400.0).dimensions();
        assert!((w - 300.0).abs() < 0.01);
        assert!((h - 400.0).abs() < 0.01);
    }

    #[test]
    fn document_builder_default_page_size() {
        let builder = DocumentBuilder::new();
        let doc = builder.build().unwrap();
        let pages_ids: Vec<_> = doc.page_object_ids().collect();
        assert_eq!(pages_ids.len(), 1);
        // Verify catalog exists
        assert!(doc.catalog().is_some());
        // Verify page has MediaBox
        let page_id = pages_ids[0];
        let page_obj = doc.get_object_ref(page_id).unwrap();
        let page_dict = page_obj.as_dictionary().unwrap();
        assert!(page_dict
            .get(&CosName::new(b"MediaBox".to_vec()))
            .is_some());
    }

    #[test]
    fn document_builder_custom_size() {
        let builder = DocumentBuilder::new().page_size(PageSize::Letter);
        let doc = builder.build().unwrap();
        let page_id = doc.page_object_ids().next().unwrap();
        let page_obj = doc.get_object_ref(page_id).unwrap();
        let page_dict = page_obj.as_dictionary().unwrap();
        let media_box = page_dict
            .get(&CosName::new(b"MediaBox".to_vec()))
            .unwrap();
        let arr = media_box.as_array().unwrap();
        assert!((arr[2].as_real().unwrap() - 612.0).abs() < 0.01);
        assert!((arr[3].as_real().unwrap() - 792.0).abs() < 0.01);
    }

    #[test]
    fn document_builder_has_helvetica_font() {
        let builder = DocumentBuilder::new();
        let doc = builder.build().unwrap();
        let catalog = doc.catalog().unwrap();
        let pages_ref = catalog.get(&CosName::pages()).unwrap().as_reference().unwrap();
        let pages_obj = doc.get_object_ref(pages_ref).unwrap();
        let pages_dict = pages_obj.as_dictionary().unwrap();
        let resources = pages_dict
            .get(&CosName::new(b"Resources".to_vec()))
            .unwrap();
        let res_dict = resources.as_dictionary().unwrap();
        let fonts = res_dict.get(&CosName::new(b"Font".to_vec())).unwrap();
        let font_dict = fonts.as_dictionary().unwrap();
        assert!(font_dict
            .get(&CosName::new(b"Helvetica".to_vec()))
            .is_some());
    }

    #[test]
    fn document_builder_multiple_pages() {
        let builder = DocumentBuilder::new().page_size(PageSize::A4);
        let doc = builder.build().unwrap();
        // Base builder only creates 1 page
        let page_count = doc.page_object_ids().count();
        assert_eq!(page_count, 1);
    }

    #[test]
    fn document_builder_debug_clone() {
        let builder = DocumentBuilder::new();
        let _ = builder.page_size(PageSize::A4);
    }

    #[test]
    fn page_size_debug() {
        let _ = format!("{:?}", PageSize::A4);
        let _ = format!("{:?}", PageSize::Letter);
        let _ = format!("{:?}", PageSize::Custom(100.0, 200.0));
    }
}
