use crate::cos::{CosDictionary, ObjectId};
use crate::ObjectStore;

/// Represents a widget annotation linking a form field to the visual page.
///
/// Maps to `PDAnnotationWidget` in Java PDFBox.
#[derive(Debug, Clone)]
pub struct PdWidget<'a> {
    pub id: ObjectId,
    pub dict: &'a CosDictionary,
    #[allow(dead_code)]
    store: &'a ObjectStore,
}

impl<'a> PdWidget<'a> {
    pub fn new(id: ObjectId, dict: &'a CosDictionary, store: &'a ObjectStore) -> Self {
        Self { id, dict, store }
    }
}
