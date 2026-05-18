use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::pdmodel::{rectangle_from_cos, Rectangle};
use crate::{PdfError, PdfResult};

#[derive(Debug, Clone)]
pub struct PdAnnotation {
    pub id: Option<ObjectId>,
    pub subtype: String,
    pub rect: Rectangle,
    pub contents: Option<String>,
    pub name: Option<String>,
    pub flags: Option<i64>,
    pub color: Option<[f64; 3]>,
    pub opacity: Option<f64>,
}

impl PdAnnotation {
    pub fn from_dict(dict: &CosDictionary, id: Option<ObjectId>) -> PdfResult<Self> {
        let subtype = dict
            .get(&CosName::new(b"Subtype".to_vec()))
            .and_then(|v| v.as_name())
            .map(|n| n.to_string())
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: "annotation missing /Subtype".to_string(),
            })?;

        let rect = dict
            .get(&CosName::new(b"Rect".to_vec()))
            .and_then(rectangle_from_cos)
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: "annotation missing /Rect".to_string(),
            })?;

        let contents = dict
            .get(&CosName::new(b"Contents".to_vec()))
            .and_then(string_from_cos);

        let name = dict
            .get(&CosName::new(b"Name".to_vec()))
            .and_then(|v| v.as_name())
            .map(|n| n.to_string());

        let flags = dict.get_int(&CosName::new(b"F".to_vec()));
        let opacity = dict.get_number(&CosName::new(b"CA".to_vec()));
        let color = dict
            .get(&CosName::new(b"C".to_vec()))
            .and_then(parse_rgb_array);

        Ok(Self {
            id,
            subtype,
            rect,
            contents,
            name,
            flags,
            color,
            opacity,
        })
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

fn parse_rgb_array(obj: &CosObject) -> Option<[f64; 3]> {
    let arr = obj.as_array()?;
    if arr.len() < 3 {
        return None;
    }
    Some([arr[0].as_number()?, arr[1].as_number()?, arr[2].as_number()?])
}

