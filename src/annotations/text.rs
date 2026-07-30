use crate::cos::{CosDictionary, CosName, CosObject};

use super::AnnotationCommon;

#[derive(Debug, Clone)]
pub struct TextAnnotation {
    pub common: AnnotationCommon,
    pub open: Option<bool>,
}

impl TextAnnotation {
    pub fn from_dict(common: AnnotationCommon, dict: &CosDictionary) -> Self {
        let open = dict
            .get(&CosName::new(b"Open".to_vec()))
            .and_then(|v| v.as_bool());

        Self { common, open }
    }

    pub fn apply_to_dict(&self, dict: &mut CosDictionary) {
        self.common.apply_to_dict(dict);
        if let Some(open) = self.open {
            dict.insert(CosName::new(b"Open".to_vec()), CosObject::Bool(open));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
    use crate::pdmodel::Rectangle;

    fn make_common() -> AnnotationCommon {
        AnnotationCommon {
            id: Some(ObjectId::new(1, 0)),
            rect: Rectangle::new(0.0, 0.0, 100.0, 50.0),
            contents: None,
            name: None,
            flags: None,
            color: None,
            opacity: None,
        }
    }

    #[test]
    fn test_text_annotation_from_dict_open() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::new(b"Open".to_vec()), CosObject::Bool(true));
        let ta = TextAnnotation::from_dict(make_common(), &dict);
        assert_eq!(ta.open, Some(true));
    }

    #[test]
    fn test_text_annotation_from_dict_no_open() {
        let dict = CosDictionary::new();
        let ta = TextAnnotation::from_dict(make_common(), &dict);
        assert!(ta.open.is_none());
    }

    #[test]
    fn test_text_annotation_from_dict_open_false() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::new(b"Open".to_vec()), CosObject::Bool(false));
        let ta = TextAnnotation::from_dict(make_common(), &dict);
        assert_eq!(ta.open, Some(false));
    }

    #[test]
    fn test_text_annotation_apply_to_dict() {
        let ta = TextAnnotation {
            common: make_common(),
            open: Some(true),
        };
        let mut dict = CosDictionary::new();
        ta.apply_to_dict(&mut dict);
        let open = dict
            .get(&CosName::new(b"Open".to_vec()))
            .and_then(|v| v.as_bool());
        assert_eq!(open, Some(true));
    }

    #[test]
    fn test_text_annotation_apply_to_dict_no_open() {
        let ta = TextAnnotation {
            common: make_common(),
            open: None,
        };
        let mut dict = CosDictionary::new();
        ta.apply_to_dict(&mut dict);
        assert!(dict.get(&CosName::new(b"Open".to_vec())).is_none());
    }

    #[test]
    fn test_text_annotation_debug() {
        let ta = TextAnnotation {
            common: make_common(),
            open: Some(true),
        };
        let d = format!("{:?}", ta);
        assert!(d.contains("TextAnnotation"));
    }

    #[test]
    fn test_text_annotation_clone() {
        let ta = TextAnnotation {
            common: make_common(),
            open: Some(true),
        };
        let cloned = ta.clone();
        assert_eq!(ta.open, cloned.open);
    }
}
