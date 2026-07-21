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
