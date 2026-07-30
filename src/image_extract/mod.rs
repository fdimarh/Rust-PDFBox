//! Phase 17 baseline: image XObject discovery and basic decode helpers.
pub mod decode;
pub mod export;
#[cfg(feature = "compress-color")]
pub mod lcms;
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

        let resources = resolve_dict(
            page_dict.get(&CosName::new(b"Resources".to_vec())),
            &self.objects,
        );
        let xobjects = resources
            .and_then(|r| resolve_dict(r.get(&CosName::new(b"XObject".to_vec())), &self.objects));
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
                    let Some(obj) = self.objects.get(id) else {
                        continue;
                    };
                    let Some(stream) = obj.as_stream() else {
                        continue;
                    };
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
            let filter = stream
                .dictionary
                .get(&CosName::new(b"Filter".to_vec()))
                .cloned();
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

fn resolve_dict<'a>(
    obj: Option<&'a CosObject>,
    store: &'a crate::ObjectStore,
) -> Option<&'a CosDictionary> {
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
                if content[data_start] == b'\r'
                    && data_start + 1 < content.len()
                    && content[data_start + 1] == b'\n'
                {
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

fn resolve_color_space_obj(
    obj: Option<&CosObject>,
    store: &crate::ObjectStore,
) -> Option<CosObject> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject, CosStream, ObjectId};

    // ── is_image_xobject ───────────────────────────────────────────

    #[test]
    fn test_is_image_xobject_true() {
        let mut d = CosDictionary::new();
        d.insert(
            CosName::new(b"Type".to_vec()),
            CosObject::Name(CosName::new(b"XObject".to_vec())),
        );
        d.insert(
            CosName::new(b"Subtype".to_vec()),
            CosObject::Name(CosName::new(b"Image".to_vec())),
        );
        assert!(is_image_xobject(&d));
    }

    #[test]
    fn test_is_image_xobject_false_for_form() {
        let mut d = CosDictionary::new();
        d.insert(
            CosName::new(b"Type".to_vec()),
            CosObject::Name(CosName::new(b"XObject".to_vec())),
        );
        d.insert(
            CosName::new(b"Subtype".to_vec()),
            CosObject::Name(CosName::new(b"Form".to_vec())),
        );
        assert!(!is_image_xobject(&d));
    }

    #[test]
    fn test_is_image_xobject_missing_type() {
        let mut d = CosDictionary::new();
        d.insert(
            CosName::new(b"Subtype".to_vec()),
            CosObject::Name(CosName::new(b"Image".to_vec())),
        );
        assert!(!is_image_xobject(&d));
    }

    #[test]
    fn test_is_image_xobject_empty_dict() {
        let d = CosDictionary::new();
        assert!(!is_image_xobject(&d));
    }

    // ── parse_color_space_name ────────────────────────────────────

    #[test]
    fn test_parse_color_space_name_device_rgb() {
        let obj = CosObject::Name(CosName::new(b"DeviceRGB".to_vec()));
        assert_eq!(
            parse_color_space_name(Some(&obj)).as_deref(),
            Some("DeviceRGB")
        );
    }

    #[test]
    fn test_parse_color_space_name_array() {
        let arr = CosObject::Array(vec![
            CosObject::Name(CosName::new(b"ICCBased".to_vec())),
            CosObject::Reference(ObjectId::new(10, 0)),
        ]);
        assert_eq!(
            parse_color_space_name(Some(&arr)).as_deref(),
            Some("ICCBased")
        );
    }

    #[test]
    fn test_parse_color_space_name_none() {
        assert_eq!(parse_color_space_name(None), None);
    }

    #[test]
    fn test_parse_color_space_name_empty_array() {
        let arr = CosObject::Array(vec![]);
        assert_eq!(parse_color_space_name(Some(&arr)), None);
    }

    // ── parse_filter_names ─────────────────────────────────────────

    #[test]
    fn test_parse_filter_names_single() {
        let obj = CosObject::Name(CosName::new(b"DCTDecode".to_vec()));
        assert_eq!(parse_filter_names(Some(&obj)), vec!["DCTDecode"]);
    }

    #[test]
    fn test_parse_filter_names_array() {
        let obj = CosObject::Array(vec![
            CosObject::Name(CosName::new(b"FlateDecode".to_vec())),
            CosObject::Name(CosName::new(b"DCTDecode".to_vec())),
        ]);
        assert_eq!(
            parse_filter_names(Some(&obj)),
            vec!["FlateDecode", "DCTDecode"]
        );
    }

    #[test]
    fn test_parse_filter_names_none() {
        let empty: Vec<String> = vec![];
        assert_eq!(parse_filter_names(None), empty);
    }

    // ── normalize_inline_dict/value ────────────────────────────────

    #[test]
    fn test_normalize_inline_dict_abbreviations() {
        let mut d = CosDictionary::new();
        d.insert(CosName::new(b"W".to_vec()), CosObject::Integer(100));
        d.insert(CosName::new(b"H".to_vec()), CosObject::Integer(50));
        d.insert(CosName::new(b"BPC".to_vec()), CosObject::Integer(8));
        d.insert(
            CosName::new(b"CS".to_vec()),
            CosObject::Name(CosName::new(b"RGB".to_vec())),
        );
        let norm = normalize_inline_dict(&d);
        assert_eq!(norm.get_int(&CosName::new(b"Width".to_vec())), Some(100));
        assert_eq!(norm.get_int(&CosName::new(b"Height".to_vec())), Some(50));
        assert_eq!(
            norm.get_int(&CosName::new(b"BitsPerComponent".to_vec())),
            Some(8)
        );
        assert_eq!(
            norm.get_name(&CosName::new(b"ColorSpace".to_vec()))
                .map(|n| n.as_bytes().to_vec()),
            Some(b"DeviceRGB".to_vec())
        );
    }

    #[test]
    fn test_normalize_inline_value_g_to_device_gray() {
        let v = CosObject::Name(CosName::new(b"G".to_vec()));
        let norm = normalize_inline_value(&v);
        assert_eq!(norm.as_name().and_then(|n| n.as_str()), Some("DeviceGray"));
    }

    // ── inline image helpers ───────────────────────────────────────

    #[test]
    fn test_extract_inline_images_empty() {
        let data = [
            66, 84, 32, 47, 70, 49, 32, 49, 50, 32, 84, 102, 32, 40, 104, 101, 108, 108, 111, 41,
            32, 84, 106, 32, 69, 84, 32,
        ];
        let result = extract_inline_images(&data);
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_inline_images_single() {
        // BI /W 4 /H 2 /BPC 1 /CS /G ID data EI
        let data = b"q BI /W 4 /H 2 /BPC 1 /CS /G ID data EI Q " as &[u8];
        let result = extract_inline_images(data);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].resource_name(), "inline_1");
        assert_eq!(result[0].width(), 4);
        assert_eq!(result[0].height(), 2);
        assert_eq!(result[0].bits_per_component(), 1);
        //assert_eq!(result[0].color_space(), Some("DeviceGray"));
    }

    // ── is_white / is_boundary ─────────────────────────────────────

    #[test]
    fn test_is_white_space() {
        assert!(is_white(b' '));
        assert!(is_white(b'\t'));
        assert!(is_white(b'\n'));
        assert!(is_white(b'\r'));
        assert!(!is_white(b'a'));
        assert!(!is_white(b'<'));
    }

    #[test]
    fn test_is_boundary_out_of_range() {
        let data = [97, 98, 99];
        assert!(is_boundary(&data, 10));
    }

    // ── find_id / find_ei ──────────────────────────────────────────

    #[test]
    fn test_find_id_marker_found() {
        let data = [66, 73, 32, 47, 87, 32, 52, 32, 73, 68, 32];
        let (dict_end, data_start) = find_id_marker(&data, 8).unwrap();
        assert!(data_start > dict_end);
    }

    #[test]
    fn test_find_id_marker_not_found() {
        let data = [
            110, 111, 32, 109, 97, 114, 107, 101, 114, 32, 104, 101, 114, 101, 32,
        ];
        assert!(find_id_marker(&data, 0).is_none());
    }

    #[test]
    fn test_find_ei_marker_found() {
        let data = [
            115, 111, 109, 101, 32, 100, 97, 116, 97, 32, 69, 73, 32, 101, 110, 100, 32,
        ];
        let pos = find_ei_marker(&data, 0);
        assert!(pos.is_some());
    }

    #[test]
    fn test_find_ei_marker_not_found() {
        let data = [110u8, 111, 32, 101, 105, 32, 104, 101, 114, 101, 32];
        assert!(find_ei_marker(&data, 0).is_none());
    }
}
