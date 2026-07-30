use crate::ObjectStore;
use crate::cos::{CosDictionary, ObjectId};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
    use crate::ObjectStore;

    fn make_widget_dict() -> (CosDictionary, ObjectStore, ObjectId) {
        let store = ObjectStore::new();
        let id = ObjectId::new(10, 0);
        let mut dict = CosDictionary::new();
        dict.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Annot".to_vec())));
        dict.insert(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Widget".to_vec())));
        dict.insert(CosName::new(b"Rect".to_vec()), CosObject::Array(vec![
            CosObject::Integer(0), CosObject::Integer(0),
            CosObject::Integer(100), CosObject::Integer(50),
        ]));
        (dict, store, id)
    }

    #[test]
    fn test_widget_new() {
        let (dict, store, id) = make_widget_dict();
        let w = PdWidget::new(id, &dict, &store);
        assert_eq!(w.id, id);
    }

    #[test]
    fn test_widget_type_subtype() {
        let (dict, store, id) = make_widget_dict();
        let w = PdWidget::new(id, &dict, &store);
        assert_eq!(
            w.dict.get(&CosName::new(b"Type".to_vec())),
            Some(&CosObject::Name(CosName::new(b"Annot".to_vec())))
        );
        assert_eq!(
            w.dict.get(&CosName::new(b"Subtype".to_vec())),
            Some(&CosObject::Name(CosName::new(b"Widget".to_vec())))
        );
    }

    #[test]
    fn test_widget_rect() {
        let (dict, store, id) = make_widget_dict();
        let w = PdWidget::new(id, &dict, &store);
        let rect = w.dict.get(&CosName::new(b"Rect".to_vec()));
        assert!(rect.is_some());
    }

    #[test]
    fn test_widget_missing_da() {
        let (dict, store, id) = make_widget_dict();
        let w = PdWidget::new(id, &dict, &store);
        assert!(w.dict.get(&CosName::new(b"DA".to_vec())).is_none());
        assert!(w.dict.get(&CosName::new(b"FT".to_vec())).is_none());
    }

    #[test]
    fn test_widget_debug_formats() {
        let (dict, store, id) = make_widget_dict();
        let w = PdWidget::new(id, &dict, &store);
        let debug = format!("{:?}", w);
        assert!(debug.contains("PdWidget"));
    }

    #[test]
    fn test_widget_clone() {
        let (dict, store, id) = make_widget_dict();
        let w1 = PdWidget::new(id, &dict, &store);
        let w2 = w1.clone();
        assert_eq!(w1.id, w2.id);
    }

    #[test]
    fn test_widget_large_rect() {
        let mut dict = make_widget_dict().0;
        dict.insert(CosName::new(b"Rect".to_vec()), CosObject::Array(vec![
            CosObject::Integer(0), CosObject::Integer(0),
            CosObject::Integer(10000), CosObject::Integer(8000),
        ]));
        let store = ObjectStore::new();
        let w = PdWidget::new(ObjectId::new(1, 0), &dict, &store);
        let rect = w.dict.get(&CosName::new(b"Rect".to_vec())).unwrap();
        let arr = rect.as_array().unwrap();
        assert_eq!(arr.len(), 4);
    }
}
