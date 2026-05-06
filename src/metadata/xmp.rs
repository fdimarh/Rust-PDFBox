#[derive(Debug, Clone)]
pub struct XmpMetadata {
    raw_xml: String,
    dc_title: Option<String>,
    dc_creator: Option<String>,
}

impl XmpMetadata {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let raw_xml = String::from_utf8(bytes.to_vec()).ok()?;
        let dc_title = extract_dc_title(&raw_xml);
        let dc_creator = extract_dc_creator(&raw_xml);

        Some(Self {
            raw_xml,
            dc_title,
            dc_creator,
        })
    }

    pub fn raw_xml(&self) -> &str {
        &self.raw_xml
    }

    pub fn dc_title(&self) -> Option<&str> {
        self.dc_title.as_deref()
    }

    pub fn dc_creator(&self) -> Option<&str> {
        self.dc_creator.as_deref()
    }
}

fn extract_dc_title(xml: &str) -> Option<String> {
    extract_first_li(xml, "dc:title")
        .or_else(|| extract_tag_text(xml, "dc:title"))
        .map(decode_xml_entities)
}

fn extract_dc_creator(xml: &str) -> Option<String> {
    extract_first_li(xml, "dc:creator")
        .or_else(|| extract_tag_text(xml, "dc:creator"))
        .map(decode_xml_entities)
}

fn extract_first_li<'a>(xml: &'a str, parent_tag: &str) -> Option<&'a str> {
    let parent_body = extract_tag_body(xml, parent_tag)?;
    extract_tag_text(parent_body, "rdf:li").or_else(|| extract_tag_text(parent_body, "li"))
}

fn extract_tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let body = extract_tag_body(xml, tag)?;
    let trimmed = body.trim();
    if trimmed.is_empty() || trimmed.contains('<') {
        None
    } else {
        Some(trimmed)
    }
}

fn extract_tag_body<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let open_pos = xml.find(&open)?;
    let open_end_rel = xml[open_pos..].find('>')?;
    let content_start = open_pos + open_end_rel + 1;
    let close = format!("</{tag}>");
    let close_pos_rel = xml[content_start..].find(&close)?;
    let content_end = content_start + close_pos_rel;
    Some(&xml[content_start..content_end])
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

