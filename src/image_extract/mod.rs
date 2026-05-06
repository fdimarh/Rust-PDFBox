//! Phase 17 baseline: image XObject discovery and basic decode helpers.

mod decode;
mod export;

use std::collections::HashSet;

use crate::content::parse_content_stream;
use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::parser::Parser;
use crate::{Document, PdfError, PdfResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageExportFormat {
    Png,
    Jpeg,
    Tiff,
}

#[derive(Debug, Clone)]
pub(crate) struct ImageMask {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bits_per_component: u8,
    pub(crate) data: Vec<u8>,
    pub(crate) filter: Option<CosObject>,
}

#[derive(Debug, Clone)]
pub struct PdImage {
    object_id: Option<ObjectId>,
    resource_name: String,
    width: u32,
    height: u32,
    bits_per_component: u8,
    color_space: Option<String>,
    color_space_obj: Option<CosObject>,
    smask: Option<ImageMask>,
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

    pub(crate) fn effective_color_space(&self) -> Option<&str> {
        if self.color_space.as_deref() != Some("ICCBased") {
            return self.color_space.as_deref();
        }

        let arr = self.color_space_obj.as_ref()?.as_array()?;
        let tag = arr.first()?.as_name()?.as_str()?;
        if tag != "ICCBased" {
            return self.color_space.as_deref();
        }

        let profile_obj = arr.get(1)?;
        let profile_dict = profile_obj
            .as_stream()
            .map(|s| &s.dictionary)
            .or_else(|| profile_obj.as_dictionary())?;

        let alternate = profile_dict
            .get(&CosName::new(b"Alternate".to_vec()))
            .and_then(|v| v.as_name())
            .and_then(|n| n.as_str());
        match alternate {
            Some("DeviceGray") | Some("DeviceRGB") | Some("DeviceCMYK") => return alternate,
            _ => {}
        }

        let n = profile_dict
            .get(&CosName::new(b"N".to_vec()))
            .and_then(|v| v.as_integer());
        match n {
            Some(1) => Some("DeviceGray"),
            Some(3) => Some("DeviceRGB"),
            Some(4) => Some("DeviceCMYK"),
            _ => None,
        }
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

        let content_bytes = self.page_content_bytes(page_id)?;
        let instructions = parse_content_stream(&content_bytes).unwrap_or_default();

        let mut out = extract_inline_images(&content_bytes);
        let mut seen_object_ids = HashSet::new();
        let mut seen_inline_names = HashSet::new();

        let resources = resolve_dict(page_dict.get(&CosName::new(b"Resources".to_vec())), &self.objects);
        let xobjects = resources.and_then(|r| resolve_dict(r.get(&CosName::new(b"XObject".to_vec())), &self.objects));
        let Some(xobjects) = xobjects else {
            return Ok(out);
        };

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
            let color_space_obj = resolve_color_space_obj(
                stream.dictionary.get(&CosName::new(b"ColorSpace".to_vec())),
                &self.objects,
            );
            let color_space = parse_color_space_name(color_space_obj.as_ref());
            let smask = extract_smask(&stream.dictionary, &self.objects);
            let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec())).cloned();
            let filter_names = parse_filter_names(filter.as_ref());

            out.push(PdImage {
                object_id,
                resource_name,
                width,
                height,
                bits_per_component,
                color_space,
                color_space_obj,
                smask,
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

fn extract_inline_images(content: &[u8]) -> Vec<PdImage> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut index = 1usize;

    while i + 2 < content.len() {
        if !((i == 0 || is_boundary(content, i - 1))
            && &content[i..i + 2] == b"BI"
            && is_boundary(content, i + 2))
        {
            i += 1;
            continue;
        }

        let mut j = i + 2;
        while j < content.len() && is_white(content[j]) {
            j += 1;
        }
        if j >= content.len() {
            break;
        }

        let Some((dict_end, data_start)) = find_id_marker(content, j) else {
            break;
        };
        let Some(data_end) = find_ei_marker(content, data_start) else {
            break;
        };

        let dict_slice = &content[j..dict_end];
        let data = content[data_start..data_end].to_vec();

        if let Some(img) = build_inline_image(dict_slice, data, index) {
            out.push(img);
            index += 1;
        }

        i = data_end + 2;
    }

    out
}

fn build_inline_image(dict_slice: &[u8], data: Vec<u8>, index: usize) -> Option<PdImage> {
    let mut wrapped = Vec::with_capacity(dict_slice.len() + 6);
    wrapped.extend_from_slice(b"<< ");
    wrapped.extend_from_slice(dict_slice);
    wrapped.extend_from_slice(b" >>");

    let mut parser = Parser::new(&wrapped);
    let obj = parser.parse_object().ok().flatten()?;
    let dict = obj.as_dictionary()?.clone();
    let dict = normalize_inline_dict(&dict);

    let width = dict
        .get(&CosName::new(b"Width".to_vec()))
        .and_then(|v| v.as_integer())
        .unwrap_or(0)
        .max(0) as u32;
    let height = dict
        .get(&CosName::new(b"Height".to_vec()))
        .and_then(|v| v.as_integer())
        .unwrap_or(0)
        .max(0) as u32;
    let bits_per_component = dict
        .get(&CosName::new(b"BitsPerComponent".to_vec()))
        .and_then(|v| v.as_integer())
        .unwrap_or(8)
        .clamp(0, 255) as u8;
    let color_space_obj = dict.get(&CosName::new(b"ColorSpace".to_vec())).cloned();
    let color_space = parse_color_space_name(color_space_obj.as_ref());
    let smask = None;
    let filter = dict.get(&CosName::new(b"Filter".to_vec())).cloned();
    let filter_names = parse_filter_names(filter.as_ref());

    Some(PdImage {
        object_id: None,
        resource_name: format!("inline_{index}"),
        width,
        height,
        bits_per_component,
        color_space,
        color_space_obj,
        smask,
        filter_names,
        data,
        filter,
    })
}

fn normalize_inline_dict(input: &CosDictionary) -> CosDictionary {
    let mut out = CosDictionary::new();
    for (k, v) in input.iter() {
        let key = match k.as_str() {
            Some("W") => CosName::new(b"Width".to_vec()),
            Some("H") => CosName::new(b"Height".to_vec()),
            Some("BPC") => CosName::new(b"BitsPerComponent".to_vec()),
            Some("CS") => CosName::new(b"ColorSpace".to_vec()),
            Some("F") => CosName::new(b"Filter".to_vec()),
            _ => k.clone(),
        };
        out.insert(key, normalize_inline_value(v));
    }
    out
}

fn normalize_inline_value(v: &CosObject) -> CosObject {
    match v {
        CosObject::Name(n) => {
            let mapped = match n.as_str() {
                Some("G") => CosName::new(b"DeviceGray".to_vec()),
                Some("RGB") => CosName::new(b"DeviceRGB".to_vec()),
                Some("CMYK") => CosName::new(b"DeviceCMYK".to_vec()),
                _ => n.clone(),
            };
            CosObject::Name(mapped)
        }
        CosObject::Array(a) => CosObject::Array(a.iter().map(normalize_inline_value).collect()),
        other => other.clone(),
    }
}

fn is_white(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00)
}

fn is_boundary(content: &[u8], i: usize) -> bool {
    if i >= content.len() {
        return true;
    }
    is_white(content[i])
}

fn find_id_marker(content: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut i = start;
    while i + 2 < content.len() {
        if content[i] == b'I'
            && content[i + 1] == b'D'
            && (i == 0 || is_boundary(content, i - 1))
            && is_boundary(content, i + 2)
        {
            let mut data_start = i + 2;
            if data_start < content.len() {
                if content[data_start] == b'\r' && data_start + 1 < content.len() && content[data_start + 1] == b'\n' {
                    data_start += 2;
                } else if is_white(content[data_start]) {
                    data_start += 1;
                }
            }
            return Some((i, data_start));
        }
        i += 1;
    }
    None
}

fn find_ei_marker(content: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 2 < content.len() {
        if content[i] == b'E'
            && content[i + 1] == b'I'
            && (i == 0 || is_boundary(content, i - 1))
            && is_boundary(content, i + 2)
        {
            return Some(i.saturating_sub(1));
        }
        i += 1;
    }
    None
}

fn resolve_color_space_obj(obj: Option<&CosObject>, store: &crate::ObjectStore) -> Option<CosObject> {
    fn resolve(obj: &CosObject, store: &crate::ObjectStore) -> CosObject {
        match obj {
            CosObject::Reference(id) => store
                .get(id)
                .map(|o| resolve(o, store))
                .unwrap_or_else(|| CosObject::Reference(*id)),
            CosObject::Array(values) => {
                CosObject::Array(values.iter().map(|v| resolve(v, store)).collect())
            }
            other => other.clone(),
        }
    }

    obj.map(|v| resolve(v, store))
}

fn extract_smask(dict: &CosDictionary, store: &crate::ObjectStore) -> Option<ImageMask> {
    let smask_obj = dict.get(&CosName::new(b"SMask".to_vec()))?;
    let smask_stream = match smask_obj {
        CosObject::Reference(id) => store.get(id)?.as_stream()?,
        CosObject::Stream(s) => s,
        _ => return None,
    };

    let width = smask_stream
        .dictionary
        .get(&CosName::new(b"Width".to_vec()))
        .and_then(|v| v.as_integer())
        .unwrap_or(0)
        .max(0) as u32;
    let height = smask_stream
        .dictionary
        .get(&CosName::new(b"Height".to_vec()))
        .and_then(|v| v.as_integer())
        .unwrap_or(0)
        .max(0) as u32;
    let bits_per_component = smask_stream
        .dictionary
        .get(&CosName::new(b"BitsPerComponent".to_vec()))
        .and_then(|v| v.as_integer())
        .unwrap_or(8)
        .clamp(0, 255) as u8;
    let filter = smask_stream
        .dictionary
        .get(&CosName::new(b"Filter".to_vec()))
        .cloned();

    Some(ImageMask {
        width,
        height,
        bits_per_component,
        data: smask_stream.data.clone(),
        filter,
    })
}

