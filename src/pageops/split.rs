use crate::{Document, PdfResult};
use super::extract::extract_pages;

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

