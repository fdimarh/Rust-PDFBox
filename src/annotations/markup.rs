use crate::cos::{CosDictionary, CosName, CosObject};

use super::AnnotationCommon;

#[derive(Debug, Clone, Copy)]
pub enum MarkupType {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

impl MarkupType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MarkupType::Highlight => "Highlight",
            MarkupType::Underline => "Underline",
            MarkupType::StrikeOut => "StrikeOut",
            MarkupType::Squiggly => "Squiggly",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarkupAnnotation {
    pub common: AnnotationCommon,
    pub subtype: MarkupType,
    pub quad_points: Vec<[f64; 8]>,
}

impl MarkupAnnotation {
    pub fn from_dict(common: AnnotationCommon, subtype: MarkupType, dict: &CosDictionary) -> Self {
        let quad_points = dict
            .get(&CosName::new(b"QuadPoints".to_vec()))
            .and_then(parse_quad_points)
            .unwrap_or_default();

        Self {
            common,
            subtype,
            quad_points,
        }
    }

    pub fn apply_to_dict(&self, dict: &mut CosDictionary) {
        self.common.apply_to_dict(dict);
        if !self.quad_points.is_empty() {
            let mut items = Vec::new();
            for quad in &self.quad_points {
                for value in quad {
                    items.push(CosObject::Real(*value));
                }
            }
            dict.insert(
                CosName::new(b"QuadPoints".to_vec()),
                CosObject::Array(items),
            );
        }
    }
}

fn parse_quad_points(obj: &CosObject) -> Option<Vec<[f64; 8]>> {
    let arr = obj.as_array()?;
    if arr.len() % 8 != 0 {
        return None;
    }

    let mut quads = Vec::new();
    let mut idx = 0;
    while idx + 7 < arr.len() {
        quads.push([
            arr[idx].as_number()?,
            arr[idx + 1].as_number()?,
            arr[idx + 2].as_number()?,
            arr[idx + 3].as_number()?,
            arr[idx + 4].as_number()?,
            arr[idx + 5].as_number()?,
            arr[idx + 6].as_number()?,
            arr[idx + 7].as_number()?,
        ]);
        idx += 8;
    }

    Some(quads)
}
