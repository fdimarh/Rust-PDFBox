use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::content::ContentStreamWriter;
use crate::pdmodel::{rectangle_from_cos, Rectangle};
use crate::{Document, PdfError, PdfResult};

pub mod link;
pub mod markup;
pub mod text;

pub use link::LinkAnnotation;
pub use markup::{MarkupAnnotation, MarkupType};
pub use text::TextAnnotation;

#[derive(Debug, Clone)]
pub struct AnnotationCommon {
    pub id: Option<ObjectId>,
    pub rect: Rectangle,
    pub contents: Option<String>,
    pub name: Option<String>,
    pub flags: Option<i64>,
    pub color: Option<[f64; 3]>,
    pub opacity: Option<f64>,
}

impl AnnotationCommon {
    fn from_dict(dict: &CosDictionary, id: Option<ObjectId>) -> PdfResult<Self> {
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
            .map(name_to_string);

        let flags = dict.get_int(&CosName::new(b"F".to_vec()));
        let opacity = dict.get_number(&CosName::new(b"CA".to_vec()));
        let color = dict
            .get(&CosName::new(b"C".to_vec()))
            .and_then(parse_rgb_array);

        Ok(Self {
            id,
            rect,
            contents,
            name,
            flags,
            color,
            opacity,
        })
    }

    fn apply_to_dict(&self, dict: &mut CosDictionary) {
        dict.insert(
            CosName::new(b"Rect".to_vec()),
            CosObject::Array(vec![
                CosObject::Real(self.rect.lower_left_x),
                CosObject::Real(self.rect.lower_left_y),
                CosObject::Real(self.rect.upper_right_x),
                CosObject::Real(self.rect.upper_right_y),
            ]),
        );

        if let Some(contents) = &self.contents {
            dict.insert(
                CosName::new(b"Contents".to_vec()),
                CosObject::String(contents.as_bytes().to_vec()),
            );
        }

        if let Some(name) = &self.name {
            dict.insert(
                CosName::new(b"Name".to_vec()),
                CosObject::Name(CosName::new(name.as_bytes().to_vec())),
            );
        }

        if let Some(flags) = self.flags {
            dict.insert(CosName::new(b"F".to_vec()), CosObject::Integer(flags));
        }

        if let Some(color) = self.color {
            dict.insert(
                CosName::new(b"C".to_vec()),
                CosObject::Array(vec![
                    CosObject::Real(color[0]),
                    CosObject::Real(color[1]),
                    CosObject::Real(color[2]),
                ]),
            );
        }

        if let Some(opacity) = self.opacity {
            dict.insert(CosName::new(b"CA".to_vec()), CosObject::Real(opacity));
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenericAnnotation {
    pub common: AnnotationCommon,
    pub subtype: String,
}

#[derive(Debug, Clone)]
pub enum PdAnnotation {
    Text(TextAnnotation),
    Link(LinkAnnotation),
    Markup(MarkupAnnotation),
    Generic(GenericAnnotation),
}

impl PdAnnotation {
    pub fn from_dict(dict: &CosDictionary, id: Option<ObjectId>) -> PdfResult<Self> {
        let subtype = dict
            .get(&CosName::new(b"Subtype".to_vec()))
            .and_then(|v| v.as_name())
            .map(name_to_string)
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: "annotation missing /Subtype".to_string(),
            })?;

        let common = AnnotationCommon::from_dict(dict, id)?;

        match subtype.as_str() {
            "Text" => Ok(PdAnnotation::Text(TextAnnotation::from_dict(common, dict))),
            "Link" => Ok(PdAnnotation::Link(LinkAnnotation::from_dict(common, dict))),
            "Highlight" => Ok(PdAnnotation::Markup(MarkupAnnotation::from_dict(
                common,
                MarkupType::Highlight,
                dict,
            ))),
            "Underline" => Ok(PdAnnotation::Markup(MarkupAnnotation::from_dict(
                common,
                MarkupType::Underline,
                dict,
            ))),
            "StrikeOut" => Ok(PdAnnotation::Markup(MarkupAnnotation::from_dict(
                common,
                MarkupType::StrikeOut,
                dict,
            ))),
            "Squiggly" => Ok(PdAnnotation::Markup(MarkupAnnotation::from_dict(
                common,
                MarkupType::Squiggly,
                dict,
            ))),
            _ => Ok(PdAnnotation::Generic(GenericAnnotation { common, subtype })),
        }
    }

    pub fn id(&self) -> Option<ObjectId> {
        match self {
            PdAnnotation::Text(a) => a.common.id,
            PdAnnotation::Link(a) => a.common.id,
            PdAnnotation::Markup(a) => a.common.id,
            PdAnnotation::Generic(a) => a.common.id,
        }
    }

    pub fn subtype(&self) -> &str {
        match self {
            PdAnnotation::Text(_) => "Text",
            PdAnnotation::Link(_) => "Link",
            PdAnnotation::Markup(a) => a.subtype.as_str(),
            PdAnnotation::Generic(a) => a.subtype.as_str(),
        }
    }

    pub fn to_dictionary(&self) -> CosDictionary {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"Type".to_vec()),
            CosObject::Name(CosName::new(b"Annot".to_vec())),
        );
        dict.insert(
            CosName::new(b"Subtype".to_vec()),
            CosObject::Name(CosName::new(self.subtype().as_bytes().to_vec())),
        );

        match self {
            PdAnnotation::Text(a) => a.apply_to_dict(&mut dict),
            PdAnnotation::Link(a) => a.apply_to_dict(&mut dict),
            PdAnnotation::Markup(a) => a.apply_to_dict(&mut dict),
            PdAnnotation::Generic(a) => a.common.apply_to_dict(&mut dict),
        }

        dict
    }
}

pub fn add_annotation_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    annot: PdAnnotation,
) -> PdfResult<ObjectId> {
    let annot_id = doc.allocate_object_id();
    let dict = annot.to_dictionary();

    doc.insert_object(annot_id, CosObject::Dictionary(dict));
    doc.xref.insert_if_absent(
        annot_id,
        crate::parser::xref::XRefEntry::InUse {
            offset: 0,
            generation: 0,
        },
    );

    push_annotation_reference(doc, page_id, annot_id)?;
    Ok(annot_id)
}

pub fn remove_annotation_from_page(
    doc: &mut Document,
    page_id: ObjectId,
    index: usize,
) -> PdfResult<()> {
    let annots_name = CosName::new(b"Annots".to_vec());
    let annots_obj = doc
        .get_object_ref(page_id)
        .and_then(|obj| obj.as_dictionary())
        .and_then(|dict| dict.get(&annots_name))
        .cloned();

    match annots_obj {
        None => Ok(()),
        Some(CosObject::Array(arr)) => {
            let mut new_arr = arr.clone();
            if index < new_arr.len() {
                new_arr.remove(index);
            }
            doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(dict) = obj {
                    if new_arr.is_empty() {
                        dict.remove(&annots_name);
                    } else {
                        dict.insert(annots_name.clone(), CosObject::Array(new_arr));
                    }
                }
            });
            Ok(())
        }
        Some(CosObject::Reference(id)) => {
            let mut new_arr = doc
                .get_object_ref(id)
                .and_then(|obj| obj.as_array())
                .map(|arr| arr.to_vec())
                .unwrap_or_default();
            if index < new_arr.len() {
                new_arr.remove(index);
            }
            if new_arr.is_empty() {
                doc.mutate_object(page_id, |obj| {
                    if let CosObject::Dictionary(dict) = obj {
                        dict.remove(&annots_name);
                    }
                });
            } else {
                doc.insert_object(id, CosObject::Array(new_arr));
            }
            Ok(())
        }
        _ => {
            doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(dict) = obj {
                    dict.remove(&annots_name);
                }
            });
            Ok(())
        }
    }
}

pub fn flatten_annotations(
    doc: &mut Document,
    page_id: ObjectId,
    page_index: usize,
    annotations: &[PdAnnotation],
) -> PdfResult<()> {
    if annotations.is_empty() {
        return Ok(());
    }

    let page_id_copy = page_id;
    let mut appearances = Vec::new();
    for (idx, annot) in annotations.iter().enumerate() {
        let Some(annot_id) = annot.id() else { continue };
        if let Some(result) = resolve_annotation_appearance(doc, page_id_copy, annot_id, idx) {
            appearances.push(result);
        }
    }

    let mut writer = ContentStreamWriter::new(doc, page_index)?;
    for (xobject_name, transform) in appearances {
        writer.save_state()?;
        writer.draw_xobject(&xobject_name, transform.0, transform.1, transform.2, transform.3, transform.4, transform.5)?;
        writer.restore_state()?;
    }

    writer.close()?;
    remove_all_annotations(doc, page_id_copy);
    Ok(())
}

fn remove_all_annotations(doc: &mut Document, page_id: ObjectId) {
    let annots_name = CosName::new(b"Annots".to_vec());
    doc.mutate_object(page_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.remove(&annots_name);
        }
    });
}

fn resolve_annotation_appearance(
    doc: &mut Document,
    page_id: ObjectId,
    annot_id: ObjectId,
    index: usize,
) -> Option<(String, (f64, f64, f64, f64, f64, f64))> {
    let dict = doc.get_object_ref(annot_id)?.as_dictionary()?.clone();
    let ap = dict.get(&CosName::new(b"AP".to_vec()))?.clone();
    let ap_dict = match ap {
        CosObject::Dictionary(d) => d,
        CosObject::Reference(id) => doc.get_object_ref(id)?.as_dictionary()?.clone(),
        _ => return None,
    };

    let normal = ap_dict.get(&CosName::new(b"N".to_vec()))?.clone();
    let (stream_id, stream_dict) = match normal {
        CosObject::Reference(id) => {
            let stream = doc.get_object_ref(id)?.as_stream()?;
            (id, stream.dictionary.clone())
        }
        CosObject::Stream(stream) => {
            let id = doc.allocate_object_id();
            doc.insert_object(id, CosObject::Stream(stream.clone()));
            doc.xref.insert_if_absent(
                id,
                crate::parser::xref::XRefEntry::InUse { offset: 0, generation: 0 },
            );
            (id, stream.dictionary.clone())
        }
        CosObject::Dictionary(states) => {
            let first = states.values().find_map(|v| v.as_reference())?;
            let stream = doc.get_object_ref(first)?.as_stream()?;
            (first, stream.dictionary.clone())
        }
        _ => return None,
    };

    let name = register_appearance_xobject(doc, page_id, stream_id, index)?;
    let bbox = stream_dict
        .get(&CosName::new(b"BBox".to_vec()))
        .and_then(|v| v.as_array())
        .and_then(parse_bbox);

    let rect = dict
        .get(&CosName::new(b"Rect".to_vec()))
        .and_then(rectangle_from_cos)
        .unwrap_or(Rectangle::new(0.0, 0.0, 0.0, 0.0));

    let transform = if let Some((x0, y0, x1, y1)) = bbox {
        let width = (x1 - x0).abs().max(1.0);
        let height = (y1 - y0).abs().max(1.0);
        let scale_x = rect.width() / width;
        let scale_y = rect.height() / height;
        (scale_x, 0.0, 0.0, scale_y, rect.lower_left_x, rect.lower_left_y)
    } else {
        (1.0, 0.0, 0.0, 1.0, rect.lower_left_x, rect.lower_left_y)
    };

    Some((name, transform))
}

fn register_appearance_xobject(
    doc: &mut Document,
    page_id: ObjectId,
    stream_id: ObjectId,
    index: usize,
) -> Option<String> {
    let resources_name = CosName::new(b"Resources".to_vec());
    let xobject_name = CosName::new(b"XObject".to_vec());
    let name = format!("AnnotAp{}", index);

    let resources_obj = doc
        .get_object_ref(page_id)
        .and_then(|obj| obj.as_dictionary())
        .and_then(|dict| dict.get(&resources_name))
        .cloned();

    let (mut resources_dict, resources_id) = match resources_obj {
        Some(CosObject::Reference(id)) => {
            let dict = doc
                .get_object_ref(id)
                .and_then(|obj| obj.as_dictionary())
                .cloned()
                .unwrap_or_default();
            (dict, Some(id))
        }
        Some(CosObject::Dictionary(dict)) => (dict, None),
        _ => (CosDictionary::new(), None),
    };

    let mut xobject_dict = resources_dict
        .get(&xobject_name)
        .and_then(|v| v.as_dictionary())
        .cloned()
        .unwrap_or_default();

    xobject_dict.insert(
        CosName::new(name.as_bytes().to_vec()),
        CosObject::Reference(stream_id),
    );
    resources_dict.insert(xobject_name, CosObject::Dictionary(xobject_dict));

    if let Some(resources_id) = resources_id {
        doc.insert_object(resources_id, CosObject::Dictionary(resources_dict));
    } else {
        doc.mutate_object(page_id, |obj| {
            if let CosObject::Dictionary(dict) = obj {
                dict.insert(resources_name.clone(), CosObject::Dictionary(resources_dict));
            }
        });
    }

    Some(name)
}

fn push_annotation_reference(doc: &mut Document, page_id: ObjectId, annot_id: ObjectId) -> PdfResult<()> {
    let annots_name = CosName::new(b"Annots".to_vec());
    let annots_obj = doc
        .get_object_ref(page_id)
        .and_then(|obj| obj.as_dictionary())
        .and_then(|dict| dict.get(&annots_name))
        .cloned();

    match annots_obj {
        None => {
            doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(dict) = obj {
                    dict.insert(
                        annots_name.clone(),
                        CosObject::Array(vec![CosObject::Reference(annot_id)]),
                    );
                }
            });
        }
        Some(CosObject::Array(arr)) => {
            let mut new_arr = arr.clone();
            new_arr.push(CosObject::Reference(annot_id));
            doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(dict) = obj {
                    dict.insert(annots_name.clone(), CosObject::Array(new_arr));
                }
            });
        }
        Some(CosObject::Reference(id)) => {
            let mut new_arr = doc
                .get_object_ref(id)
                .and_then(|obj| obj.as_array())
                .map(|arr| arr.to_vec())
                .unwrap_or_default();
            new_arr.push(CosObject::Reference(annot_id));
            doc.insert_object(id, CosObject::Array(new_arr));
        }
        _ => {
            doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(dict) = obj {
                    dict.insert(
                        annots_name.clone(),
                        CosObject::Array(vec![CosObject::Reference(annot_id)]),
                    );
                }
            });
        }
    }

    Ok(())
}

fn parse_bbox(arr: &[CosObject]) -> Option<(f64, f64, f64, f64)> {
    if arr.len() != 4 {
        return None;
    }
    Some((
        arr[0].as_number()?,
        arr[1].as_number()?,
        arr[2].as_number()?,
        arr[3].as_number()?,
    ))
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

fn name_to_string(name: &CosName) -> String {
    name.as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from_utf8_lossy(name.as_bytes()).into_owned())
}
