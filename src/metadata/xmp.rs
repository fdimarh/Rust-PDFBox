#[derive(Debug, Clone)]
pub struct XmpMetadata {
    raw_xml: String,
    dc_title: Option<String>,
    dc_creator: Option<String>,
    dc_subject: Option<String>,
    pdf_keywords: Option<String>,
    xmp_creator_tool: Option<String>,
    pdf_producer: Option<String>,
    xmp_create_date: Option<String>,
    xmp_modify_date: Option<String>,
}

impl XmpMetadata {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let raw_xml = String::from_utf8(bytes.to_vec()).ok()?;
        let dc_title = extract_dc_title(&raw_xml);
        let dc_creator = extract_dc_creator(&raw_xml);
        let dc_subject = extract_dc_subject(&raw_xml);
        let pdf_keywords = extract_pdf_keywords(&raw_xml);
        let xmp_creator_tool = extract_xmp_creator_tool(&raw_xml);
        let pdf_producer = extract_pdf_producer(&raw_xml);
        let xmp_create_date = extract_xmp_create_date(&raw_xml);
        let xmp_modify_date = extract_xmp_modify_date(&raw_xml);

        Some(Self {
            raw_xml,
            dc_title,
            dc_creator,
            dc_subject,
            pdf_keywords,
            xmp_creator_tool,
            pdf_producer,
            xmp_create_date,
            xmp_modify_date,
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

    pub fn dc_subject(&self) -> Option<&str> {
        self.dc_subject.as_deref()
    }

    pub fn pdf_keywords(&self) -> Option<&str> {
        self.pdf_keywords.as_deref()
    }

    pub fn xmp_creator_tool(&self) -> Option<&str> {
        self.xmp_creator_tool.as_deref()
    }

    pub fn pdf_producer(&self) -> Option<&str> {
        self.pdf_producer.as_deref()
    }

    pub fn xmp_create_date(&self) -> Option<&str> {
        self.xmp_create_date.as_deref()
    }

    pub fn xmp_modify_date(&self) -> Option<&str> {
        self.xmp_modify_date.as_deref()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct XmpFields<'a> {
    pub title: Option<&'a str>,
    pub creator: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub keywords: Option<&'a str>,
    pub creator_tool: Option<&'a str>,
    pub producer: Option<&'a str>,
    pub create_date: Option<&'a str>,
    pub modify_date: Option<&'a str>,
}

pub fn build_minimal_xmp(title: Option<&str>, creator: Option<&str>) -> String {
    build_basic_xmp(XmpFields {
        title,
        creator,
        ..XmpFields::default()
    })
}

pub fn build_basic_xmp(fields: XmpFields<'_>) -> String {
    let mut parts = Vec::new();

    if let Some(title) = fields.title {
        let title_li = format!(
            "<rdf:li xml:lang=\"x-default\">{}</rdf:li>",
            escape_xml(title)
        );
        parts.push(format!("<dc:title><rdf:Alt>{}</rdf:Alt></dc:title>", title_li));
    }

    if let Some(creator) = fields.creator {
        let creator_li = format!("<rdf:li>{}</rdf:li>", escape_xml(creator));
        parts.push(format!("<dc:creator><rdf:Seq>{}</rdf:Seq></dc:creator>", creator_li));
    }

    if let Some(subject) = fields.subject {
        let subject_li = format!("<rdf:li>{}</rdf:li>", escape_xml(subject));
        parts.push(format!("<dc:subject><rdf:Bag>{}</rdf:Bag></dc:subject>", subject_li));
    }

    if let Some(keywords) = fields.keywords {
        parts.push(format!("<pdf:Keywords>{}</pdf:Keywords>", escape_xml(keywords)));
    }

    if let Some(tool) = fields.creator_tool {
        parts.push(format!("<xmp:CreatorTool>{}</xmp:CreatorTool>", escape_xml(tool)));
    }

    if let Some(producer) = fields.producer {
        parts.push(format!("<pdf:Producer>{}</pdf:Producer>", escape_xml(producer)));
    }

    if let Some(date) = fields.create_date {
        parts.push(format!("<xmp:CreateDate>{}</xmp:CreateDate>", escape_xml(date)));
    }

    if let Some(date) = fields.modify_date {
        parts.push(format!("<xmp:ModifyDate>{}</xmp:ModifyDate>", escape_xml(date)));
    }

    let description = if parts.is_empty() {
        "".to_string()
    } else {
        format!("\n      {}\n    ", parts.join("\n      "))
    };

    format!(
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\n    <rdf:Description>{description}</rdf:Description>\n  </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>"
    )
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

fn extract_dc_subject(xml: &str) -> Option<String> {
    extract_first_li(xml, "dc:subject")
        .or_else(|| extract_tag_text(xml, "dc:subject"))
        .map(decode_xml_entities)
}

fn extract_pdf_keywords(xml: &str) -> Option<String> {
    extract_tag_text(xml, "pdf:Keywords").map(decode_xml_entities)
}

fn extract_xmp_creator_tool(xml: &str) -> Option<String> {
    extract_tag_text(xml, "xmp:CreatorTool").map(decode_xml_entities)
}

fn extract_pdf_producer(xml: &str) -> Option<String> {
    extract_tag_text(xml, "pdf:Producer").map(decode_xml_entities)
}

fn extract_xmp_create_date(xml: &str) -> Option<String> {
    extract_tag_text(xml, "xmp:CreateDate").map(decode_xml_entities)
}

fn extract_xmp_modify_date(xml: &str) -> Option<String> {
    extract_tag_text(xml, "xmp:ModifyDate").map(decode_xml_entities)
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

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

