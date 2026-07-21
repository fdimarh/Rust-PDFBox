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
