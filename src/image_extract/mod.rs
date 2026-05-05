//! Phase 17 baseline: image XObject discovery and basic decode helpers.

mod decode;
mod export;

use std::collections::HashSet;

use crate::content::parse_content_stream;
use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, PdfError, PdfResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageExportFormat {
    Png,
    Jpeg,
}

#[derive(Debug, Clone)]
pub struct PdImage {
    object_id: Option<ObjectId>,
    resource_name: String,
    width: u32,
    height: u32,
    bits_per_component: u8,
    color_space: Option<String>,
    filter_names: Vec<String>,
    data: Vec<u8>,
    filter: Option<CosObject>,
}

impl PdImage {
    pub fn object_id(&self) -> Option<ObjectId> {
        self.object_id
    }

    pub fn resource_name(&self) -> &str {
        &self.resource_name
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn bits_per_component(&self) -> u8 {
        self.bits_per_component
    }

    pub fn color_space(&self) -> Option<&str> {
        self.color_space.as_deref()
    }

    pub fn filter_names(&self) -> &[String] {
        &self.filter_names
    }

    pub fn encoded_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl Document {
    /// Extracts image XObjects referenced by `Do` on the given page index.
    pub fn extract_images(&self, page_index: usize) -> PdfResult<Vec<PdImage>> {
        let page_id = self
            .page_object_ids()
            .nth(page_index)
            .ok_or_else(|| PdfError::Parse {
                offset: None,
                context: format!("page index out of range: {page_index}"),
            })?;

        let page_obj = self.objects.get(&page_id).ok_or_else(|| PdfError::Xref {
            object_id: Some(page_id),
        })?;
        let page_dict = page_obj.as_dictionary().ok_or_else(|| PdfError::Parse {
            offset: None,
            context: format!("page object {page_id:?} is not a dictionary"),
        })?;

        let resources = resolve_dict(page_dict.get(&CosName::new(b"Resources".to_vec())), &self.objects);
        let Some(resources) = resources else {
            return Ok(Vec::new());
        };

        let xobjects = resolve_dict(resources.get(&CosName::new(b"XObject".to_vec())), &self.objects);
        let Some(xobjects) = xobjects else {
            return Ok(Vec::new());
        };

        let instructions = parse_content_stream(&self.page_content_bytes(page_id)?).map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("content stream parse failed: {e}"),
        })?;

        let mut out = Vec::new();
        let mut seen_object_ids = HashSet::new();
        let mut seen_inline_names = HashSet::new();

        for instr in instructions {
            if !instr.operator.is_do() || instr.operands.len() != 1 {
                continue;
            }
            let Some(name) = instr.operands[0].as_name() else {
                continue;
            };

            let resource_name = String::from_utf8_lossy(name.as_bytes()).into_owned();
            let Some(xobj_value) = xobjects.get(name) else {
                continue;
            };

            let (object_id, stream) = match xobj_value {
                CosObject::Reference(id) => {
                    let Some(obj) = self.objects.get(id) else { continue };
                    let Some(stream) = obj.as_stream() else { continue };
                    (Some(*id), stream)
                }
                CosObject::Stream(stream) => (None, stream),
                _ => continue,
            };

            if !is_image_xobject(&stream.dictionary) {
                continue;
            }

            if let Some(id) = object_id {
                if !seen_object_ids.insert(id) {
                    continue;
                }
            } else if !seen_inline_names.insert(resource_name.clone()) {
                continue;
            }

            let width = stream
                .dictionary
                .get(&CosName::new(b"Width".to_vec()))
                .and_then(|v| v.as_integer())
                .unwrap_or(0)
                .max(0) as u32;
            let height = stream
                .dictionary
                .get(&CosName::new(b"Height".to_vec()))
                .and_then(|v| v.as_integer())
                .unwrap_or(0)
                .max(0) as u32;
            let bits_per_component = stream
                .dictionary
                .get(&CosName::new(b"BitsPerComponent".to_vec()))
                .and_then(|v| v.as_integer())
                .unwrap_or(8)
                .clamp(0, 255) as u8;
            let color_space = parse_color_space_name(stream.dictionary.get(&CosName::new(b"ColorSpace".to_vec())));
            let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec())).cloned();
            let filter_names = parse_filter_names(filter.as_ref());

            out.push(PdImage {
                object_id,
                resource_name,
                width,
                height,
                bits_per_component,
                color_space,
                filter_names,
                data: stream.data.clone(),
                filter,
            });
        }

        Ok(out)
    }
}

fn resolve_dict<'a>(obj: Option<&'a CosObject>, store: &'a crate::ObjectStore) -> Option<&'a CosDictionary> {
    match obj? {
        CosObject::Dictionary(d) => Some(d),
        CosObject::Reference(id) => store.get(id)?.as_dictionary(),
        _ => None,
    }
}

fn is_image_xobject(dict: &CosDictionary) -> bool {
    let subtype = dict
        .get(&CosName::new(b"Subtype".to_vec()))
        .and_then(|v| v.as_name())
        .and_then(|n| n.as_str());
    let xtype = dict
        .get(&CosName::new(b"Type".to_vec()))
        .and_then(|v| v.as_name())
        .and_then(|n| n.as_str());
    matches!(subtype, Some("Image")) && matches!(xtype, Some("XObject"))
}

fn parse_color_space_name(obj: Option<&CosObject>) -> Option<String> {
    match obj {
        Some(CosObject::Name(name)) => Some(String::from_utf8_lossy(name.as_bytes()).into_owned()),
        Some(CosObject::Array(values)) => values
            .first()
            .and_then(|v| v.as_name())
            .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned()),
        _ => None,
    }
}

fn parse_filter_names(filter: Option<&CosObject>) -> Vec<String> {
    match filter {
        Some(CosObject::Name(name)) => vec![String::from_utf8_lossy(name.as_bytes()).into_owned()],
        Some(CosObject::Array(values)) => values
            .iter()
            .filter_map(|v| v.as_name())
            .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned())
            .collect(),
        _ => Vec::new(),
    }
}

