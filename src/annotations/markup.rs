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
    fn test_markup_type_as_str() {
        assert_eq!(MarkupType::Highlight.as_str(), "Highlight");
        assert_eq!(MarkupType::Underline.as_str(), "Underline");
        assert_eq!(MarkupType::StrikeOut.as_str(), "StrikeOut");
        assert_eq!(MarkupType::Squiggly.as_str(), "Squiggly");
    }

    #[test]
    fn test_markup_annotation_from_dict_no_quads() {
        let dict = CosDictionary::new();
        let ma = MarkupAnnotation::from_dict(make_common(), MarkupType::Highlight, &dict);
        assert_eq!(ma.subtype as usize, MarkupType::Highlight as usize);
        assert!(ma.quad_points.is_empty());
    }

    #[test]
    fn test_markup_annotation_from_dict_with_quads() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"QuadPoints".to_vec()),
            CosObject::Array(vec![
                CosObject::Real(0.0), CosObject::Real(0.0),
                CosObject::Real(100.0), CosObject::Real(0.0),
                CosObject::Real(0.0), CosObject::Real(50.0),
                CosObject::Real(100.0), CosObject::Real(50.0),
            ]),
        );
        let ma = MarkupAnnotation::from_dict(make_common(), MarkupType::Underline, &dict);
        assert_eq!(ma.quad_points.len(), 1);
        assert_eq!(ma.quad_points[0][7], 50.0);
    }

    #[test]
    fn test_markup_apply_to_dict_with_quads() {
        let ma = MarkupAnnotation {
            common: make_common(),
            subtype: MarkupType::Highlight,
            quad_points: vec![[0.0, 0.0, 100.0, 0.0, 0.0, 50.0, 100.0, 50.0]],
        };
        let mut dict = CosDictionary::new();
        ma.apply_to_dict(&mut dict);
        let qp = dict.get(&CosName::new(b"QuadPoints".to_vec()));
        assert!(qp.is_some());
    }

    #[test]
    fn test_markup_apply_to_dict_no_quads() {
        let ma = MarkupAnnotation {
            common: make_common(),
            subtype: MarkupType::StrikeOut,
            quad_points: vec![],
        };
        let mut dict = CosDictionary::new();
        ma.apply_to_dict(&mut dict);
        assert!(dict.get(&CosName::new(b"QuadPoints".to_vec())).is_none());
    }

    #[test]
    fn test_parse_quad_points_valid() {
        let arr = CosObject::Array(vec![
            CosObject::Real(0.0), CosObject::Real(0.0),
            CosObject::Real(100.0), CosObject::Real(0.0),
            CosObject::Real(0.0), CosObject::Real(50.0),
            CosObject::Real(100.0), CosObject::Real(50.0),
        ]);
        let result = parse_quad_points(&arr);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_quad_points_not_array() {
        let obj = CosObject::Integer(42);
        assert!(parse_quad_points(&obj).is_none());
    }

    #[test]
    fn test_parse_quad_points_wrong_length() {
        let arr = CosObject::Array(vec![CosObject::Real(1.0), CosObject::Real(2.0)]);
        assert!(parse_quad_points(&arr).is_none());
    }
}
