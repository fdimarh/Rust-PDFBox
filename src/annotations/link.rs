use crate::cos::{CosDictionary, CosName, CosObject};

use super::AnnotationCommon;

#[derive(Debug, Clone)]
pub struct LinkAnnotation {
    pub common: AnnotationCommon,
    pub uri: Option<String>,
    pub dest: Option<CosObject>,
}

impl LinkAnnotation {
    pub fn from_dict(common: AnnotationCommon, dict: &CosDictionary) -> Self {
        let mut uri = None;
        let mut dest = dict.get(&CosName::new(b"Dest".to_vec())).cloned();

        if let Some(action) = dict.get(&CosName::new(b"A".to_vec())) {
            let action_dict = match action {
                CosObject::Dictionary(d) => Some(d),
                _ => None,
            };

            if let Some(action_dict) = action_dict {
                if action_dict
                    .get(&CosName::new(b"S".to_vec()))
                    .and_then(|v| v.as_name())
                    .map(|n| n.as_bytes() == b"URI")
                    .unwrap_or(false)
                {
                    uri = action_dict
                        .get(&CosName::new(b"URI".to_vec()))
                        .and_then(string_from_cos);
                }

                if dest.is_none() {
                    dest = action_dict.get(&CosName::new(b"D".to_vec())).cloned();
                }
            }
        }

        Self { common, uri, dest }
    }

    pub fn apply_to_dict(&self, dict: &mut CosDictionary) {
        self.common.apply_to_dict(dict);

        if let Some(uri) = &self.uri {
            let mut action = CosDictionary::new();
            action.insert(
                CosName::new(b"S".to_vec()),
                CosObject::Name(CosName::new(b"URI".to_vec())),
            );
            action.insert(
                CosName::new(b"URI".to_vec()),
                CosObject::String(uri.as_bytes().to_vec()),
            );
            dict.insert(CosName::new(b"A".to_vec()), CosObject::Dictionary(action));
        }

        if let Some(dest) = &self.dest {
            dict.insert(CosName::new(b"Dest".to_vec()), dest.clone());
        }
    }
}

fn string_from_cos(obj: &CosObject) -> Option<String> {
    match obj {
        CosObject::String(bytes) | CosObject::HexString(bytes) => {
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        _ => None,
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
    fn test_link_from_dict_with_uri_action() {
        let mut dict = CosDictionary::new();
        let mut action = CosDictionary::new();
        action.insert(
            CosName::new(b"S".to_vec()),
            CosObject::Name(CosName::new(b"URI".to_vec())),
        );
        action.insert(
            CosName::new(b"URI".to_vec()),
            CosObject::String(b"https://example.com".to_vec()),
        );
        dict.insert(
            CosName::new(b"A".to_vec()),
            CosObject::Dictionary(action),
        );
        let link = LinkAnnotation::from_dict(make_common(), &dict);
        assert_eq!(link.uri.as_deref(), Some("https://example.com"));
        assert!(link.dest.is_none());
    }

    #[test]
    fn test_link_from_dict_with_dest() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"Dest".to_vec()),
            CosObject::Array(vec![
                CosObject::Integer(1),
                CosObject::Name(CosName::new(b"XYZ".to_vec())),
                CosObject::Integer(0),
                CosObject::Integer(0),
                CosObject::Integer(0),
            ]),
        );
        let link = LinkAnnotation::from_dict(make_common(), &dict);
        assert!(link.dest.is_some());
        assert!(link.uri.is_none());
    }

    #[test]
    fn test_link_roundtrip_uri() {
        let mut dict = CosDictionary::new();
        let mut action = CosDictionary::new();
        action.insert(
            CosName::new(b"S".to_vec()),
            CosObject::Name(CosName::new(b"URI".to_vec())),
        );
        action.insert(
            CosName::new(b"URI".to_vec()),
            CosObject::String(b"https://rust-lang.org".to_vec()),
        );
        dict.insert(
            CosName::new(b"A".to_vec()),
            CosObject::Dictionary(action),
        );

        let link = LinkAnnotation::from_dict(make_common(), &dict);
        assert_eq!(link.uri.as_deref(), Some("https://rust-lang.org"));

        let mut out_dict = CosDictionary::new();
        link.apply_to_dict(&mut out_dict);
        let out_action = out_dict
            .get(&CosName::new(b"A".to_vec()))
            .and_then(|v| v.as_dictionary())
            .unwrap();
        let out_uri = out_action
            .get(&CosName::new(b"URI".to_vec()))
            .and_then(|v| v.as_string())
            .unwrap();
        assert_eq!(out_uri, b"https://rust-lang.org");
    }

    #[test]
    fn test_link_no_action_or_dest() {
        let dict = CosDictionary::new();
        let link = LinkAnnotation::from_dict(make_common(), &dict);
        assert!(link.uri.is_none());
        assert!(link.dest.is_none());
    }

    #[test]
    fn test_string_from_cos_string() {
        let obj = CosObject::String(b"hello".to_vec());
        assert_eq!(string_from_cos(&obj), Some("hello".into()));
    }

    #[test]
    fn test_string_from_cos_hexstring() {
        // HexString stores decoded bytes internally
        let obj = CosObject::HexString(b"world".to_vec());
        assert_eq!(string_from_cos(&obj), Some("world".into()));
    }

    #[test]
    fn test_string_from_cos_other() {
        let obj = CosObject::Integer(42);
        assert_eq!(string_from_cos(&obj), None);
    }
}
