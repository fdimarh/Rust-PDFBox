use super::extract::extract_pages;
use crate::{Document, PdfResult};

/// Splits a document into multiple documents.
pub struct PdfSplitter<'a> {
    doc: &'a mut Document,
}

impl<'a> PdfSplitter<'a> {
    pub fn new(doc: &'a mut Document) -> Self {
        Self { doc }
    }

    /// Splits the document into chunks of `pages_per_doc` pages.
    pub fn split(&mut self, pages_per_doc: usize) -> PdfResult<Vec<Document>> {
        let total_pages = self.doc.page_count();
        let mut results = Vec::new();

        let mut start = 0;
        while start < total_pages {
            let end = (start + pages_per_doc).min(total_pages);
            let indices: Vec<usize> = (start..end).collect();
            let new_doc = extract_pages(self.doc, &indices)?;
            results.push(new_doc);
            start = end;
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};

    fn two_page_doc() -> Document {
        let catalog_id = ObjectId::new(1, 0);
        let pages_id = ObjectId::new(2, 0);
        let page1_id = ObjectId::new(3, 0);
        let page2_id = ObjectId::new(4, 0);
        let content_id = ObjectId::new(5, 0);

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
                d.insert(CosName::new(b"Count".to_vec()), CosObject::Integer(2));
                d.insert(
                    CosName::new(b"Kids".to_vec()),
                    CosObject::Array(vec![
                        CosObject::Reference(page1_id),
                        CosObject::Reference(page2_id),
                    ]),
                );
                d
            }),
        );
        for (i, pid) in [page1_id, page2_id].iter().enumerate() {
            doc.insert_object(
                *pid,
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
        }
        doc.insert_object(
            content_id,
            CosObject::Stream(crate::cos::CosStream::new(
                CosDictionary::new(),
                b"BT ET".to_vec(),
            )),
        );
        doc.xref.trailer.insert(
            CosName::new(b"Root".to_vec()),
            CosObject::Reference(catalog_id),
        );
        doc
    }

    #[test]
    fn test_split_one_per_doc() {
        let mut doc = two_page_doc();
        let mut splitter = PdfSplitter::new(&mut doc);
        let result = splitter.split(1).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_split_all_in_one() {
        let mut doc = two_page_doc();
        let mut splitter = PdfSplitter::new(&mut doc);
        let result = splitter.split(10).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_split_empty_doc() {
        let mut doc = Document::empty();
        let mut splitter = PdfSplitter::new(&mut doc);
        let result = splitter.split(1).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_splitter_new_creates_empty_splitter() {
        let mut doc = Document::empty();
        let splitter = PdfSplitter::new(&mut doc);
        assert!(splitter.doc.catalog_id().is_some() || splitter.doc.page_count() == 0);
    }

    #[test]
    fn test_split_exact_pages() {
        // split 2-page doc into exactly 2 pages per doc → 1 doc
        let mut doc = two_page_doc();
        let mut splitter = PdfSplitter::new(&mut doc);
        let result = splitter.split(2).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].page_count(), 2);
    }

    #[test]
    fn test_split_uneven_chunks() {
        // split 2-page doc into 3 pages per doc → first doc has 2 pages
        let mut doc = two_page_doc();
        let mut splitter = PdfSplitter::new(&mut doc);
        let result = splitter.split(3).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].page_count(), 2);
    }

    #[test]
    fn test_split_1_page_should_produce_2_docs() {
        let mut doc = two_page_doc();
        let mut splitter = PdfSplitter::new(&mut doc);
        let result = splitter.split(1).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].page_count(), 1);
        assert_eq!(result[1].page_count(), 1);
    }
}
