//!
//! FDF and XFDF import — reads form field values from FDF (PDF-based) and
//! XFDF (XML) data files and applies them to a document's AcroForm.
//!
//! Maps to Java PDFBox's FDF/XFDF import utilities.

use crate::cos::ObjectId;
use crate::forms::field::set_field_value;
use crate::{Document, PdfResult};

/// Imports field values from an FDF byte buffer into the document's AcroForm.
///
/// FDF (Forms Data Format) is a PDF-based file format that contains
/// field value pairs. Fields are matched by their fully qualified name (`/T`).
///
/// # Arguments
///
/// * `doc` - Mutable reference to the target document.
/// * `fdf_data` - Raw FDF file bytes.
pub fn import_fdf(doc: &mut Document, fdf_data: &[u8]) -> PdfResult<usize> {
    if fdf_data.is_empty() {
        return Ok(0);
    }
    if !fdf_data.starts_with(b"%FDF") {
        return Ok(0);
    }

    let fields = parse_fdf_fields(fdf_data);
    if fields.is_empty() {
        return Ok(0);
    }

    let form = match doc.acro_form() {
        Some(f) => f,
        None => return Ok(0),
    };

    let mut imported = 0usize;
    let form_fields = form.fields();

    // Build a lookup from field name to ObjectId
    let mut field_map: std::collections::HashMap<String, ObjectId> = std::collections::HashMap::new();
    for field in &form_fields {
        let name = field.fully_qualified_name();
        if !name.is_empty() {
            field_map.insert(name, field.id());
        }
    }

    for (name, value) in &fields {
        if let Some(field_id) = field_map.get(name) {
            set_field_value(doc, *field_id, value);
            imported += 1;
        }
    }

    Ok(imported)
}

/// Imports field values from an XFDF byte buffer into the document's AcroForm.
///
/// XFDF (XML Forms Data Format) is an XML-based format for form field data.
///
/// # Arguments
///
/// * `doc` - Mutable reference to the target document.
/// * `xfdf_data` - Raw XFDF XML bytes.
pub fn import_xfdf(doc: &mut Document, xfdf_data: &[u8]) -> PdfResult<usize> {
    if xfdf_data.is_empty() {
        return Ok(0);
    }

    let fields = parse_xfdf_fields(xfdf_data);
    if fields.is_empty() {
        return Ok(0);
    }

    let form = match doc.acro_form() {
        Some(f) => f,
        None => return Ok(0),
    };

    let mut imported = 0usize;
    let form_fields = form.fields();

    let mut field_map: std::collections::HashMap<String, ObjectId> = std::collections::HashMap::new();
    for field in &form_fields {
        let name = field.fully_qualified_name();
        if !name.is_empty() {
            field_map.insert(name, field.id());
        }
    }

    for (name, value) in &fields {
        if let Some(field_id) = field_map.get(name) {
            set_field_value(doc, *field_id, value);
            imported += 1;
        }
    }

    Ok(imported)
}

/// Parses field name/value pairs from FDF data by scanning for `/T` and `/V` entries.
fn parse_fdf_fields(data: &[u8]) -> Vec<(String, String)> {
    let text = match std::str::from_utf8(data) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut fields = Vec::new();

    // Simple token-based FDF parser
    // Look for sequences like: /T (fieldname) /V (value)
    let tokens = tokenize_fdf(text);
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "/T" && i + 2 < tokens.len() {
            let name = unescape_pdf_string(&tokens[i + 1]);
            // Look ahead for /V
            let mut found_v = false;
            for j in (i + 2)..tokens.len().min(i + 10) {
                if tokens[j] == "/V" && j + 1 < tokens.len() {
                    let value = unescape_pdf_string(&tokens[j + 1]);
                    fields.push((name, value));
                    found_v = true;
                    i = j + 1;
                    break;
                }
                // Also handle /V followed by a name (for checkboxes)
                if tokens[j] == "/V" && j + 1 < tokens.len() && tokens[j + 1].starts_with('/') {
                    let value = tokens[j + 1].trim_start_matches('/').to_string();
                    fields.push((name, value));
                    found_v = true;
                    i = j + 1;
                    break;
                }
            }
            if !found_v {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    fields
}

/// Simple FDF/PDF tokenizer.
fn tokenize_fdf(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_paren = false;
    let mut escape = false;

    for c in text.chars() {
        if escape {
            current.push(c);
            escape = false;
            continue;
        }
        if c == '\\' && in_paren {
            current.push(c);
            escape = true;
            continue;
        }

        if in_paren {
            if c == ')' {
                current.push(c);
                tokens.push(current.clone());
                current.clear();
                in_paren = false;
            } else {
                current.push(c);
            }
            continue;
        }

        if c == '(' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(c);
            in_paren = true;
            continue;
        }

        if c == '/' && !current.is_empty() && !current.starts_with('/') {
            tokens.push(current.clone());
            current.clear();
        }

        if c == '/' || c == '[' || c == ']' || c == '<' || c == '>' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            if c == '/' {
                current.push(c);
            } else {
                tokens.push(c.to_string());
            }
            continue;
        }

        if c.is_whitespace() || c == '\n' || c == '\r' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            continue;
        }

        current.push(c);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn unescape_pdf_string(s: &str) -> String {
    let s = s.trim();
    let s = if s.starts_with('(') && s.ends_with(')') {
        &s[1..s.len() - 1]
    } else {
        s
    };

    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('(') => out.push('('),
                Some(')') => out.push(')'),
                Some('\\') => out.push('\\'),
                Some(d) if d.is_ascii_digit() => {
                    // Octal escape
                    let mut octal = String::new();
                    octal.push(d);
                    for _ in 0..2 {
                        if let Some(n) = chars.next() {
                            if n.is_ascii_digit() {
                                octal.push(n);
                            } else {
                                break;
                            }
                        }
                    }
                    if let Ok(code) = u8::from_str_radix(&octal, 8) {
                        out.push(code as char);
                    }
                }
                Some(c) => out.push(c),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parses field name/value pairs from XFDF XML data using simple string scanning.
fn parse_xfdf_fields(data: &[u8]) -> Vec<(String, String)> {
    let text = match std::str::from_utf8(data) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut fields = Vec::new();

    // Simple state-machine XML parser for XFDF
    let mut pos = 0;
    let bytes = text.as_bytes();

    while pos < bytes.len() {
        // Look for <field name="...">
        if let Some(start) = find_substring(bytes, pos, b"<field name=\"") {
            let after_attr = start + b"<field name=\"".len();
            if let Some(end_attr) = find_byte(bytes, after_attr, b'\"') {
                let name = String::from_utf8_lossy(&bytes[after_attr..end_attr]).to_string();
                // Find value between <value> and </value>
                if let Some(val_start) = find_substring(bytes, end_attr, b"<value>") {
                    let after_val = val_start + b"<value>".len();
                    if let Some(val_end) = find_substring(bytes, after_val, b"</value>") {
                        let value = String::from_utf8_lossy(&bytes[after_val..val_end]).to_string();
                        fields.push((xml_unescape(&name), xml_unescape(&value)));
                        pos = val_end + b"</value>".len();
                    } else {
                        pos = end_attr + 1;
                    }
                } else {
                    pos = end_attr + 1;
                }
            } else {
                pos = start + 1;
            }
        } else {
            break;
        }
    }

    fields
}

fn find_substring(data: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    if start >= data.len() {
        return None;
    }
    data[start..].windows(needle.len()).position(|w| w == needle).map(|i| start + i)
}

fn find_byte(data: &[u8], start: usize, byte: u8) -> Option<usize> {
    data[start..].iter().position(|&b| b == byte).map(|i| start + i)
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn doc_with_form() -> Document {
        let bytes = b"%PDF-1.7\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [5 0 R] >>\nendobj\n\
            4 0 obj\n<< /Fields [5 0 R] /DA (/Helv 10 Tf 0 g) >>\nendobj\n\
            5 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /Rect [100 700 300 720] /P 3 0 R >>\nendobj\n\
            xref\n0 6\n0000000000 65535 f \n0000000009 00000 n \n0000000075 00000 n \n\
            0000000150 00000 n \n0000000288 00000 n \n0000000370 00000 n \n\
            trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n490\n%%EOF";
        let (doc, _) = Document::load_lenient(bytes);
        doc
    }

    #[test]
    fn test_parse_fdf_fields_basic() {
        let fdf = b"%FDF-1.2\n1 0 obj<< /FDF << /Fields [<< /T (Name) /V (Alice) >>] >> >>endobj\ntrailer<< /Root 1 0 R >>\n%%EOF";
        let fields = parse_fdf_fields(fdf);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "Name");
        assert_eq!(fields[0].1, "Alice");
    }

    #[test]
    fn test_parse_xfdf_fields_basic() {
        let xfdf = b"<?xml version=\"1.0\"?><xfdf><fields><field name=\"Name\"><value>Bob</value></field></fields></xfdf>";
        let fields = parse_xfdf_fields(xfdf);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "Name");
        assert_eq!(fields[0].1, "Bob");
    }

    #[test]
    fn test_import_fdf() {
        let mut doc = doc_with_form();
        let fdf = b"%FDF-1.2\n1 0 obj<< /FDF << /Fields [<< /T (Name) /V (Charlie) >>] >> >>endobj\ntrailer<< /Root 1 0 R >>\n%%EOF";
        let count = import_fdf(&mut doc, fdf).unwrap();
        assert_eq!(count, 1, "should import 1 field");

        // Verify the value was set
        let form = doc.acro_form().unwrap();
        let field = form.get_field("Name").unwrap();
        let value = field.value().and_then(|v| v.as_string()).map(|s| String::from_utf8_lossy(s).to_string());
        assert_eq!(value, Some("Charlie".to_string()));
    }

    #[test]
    fn test_import_xfdf() {
        let mut doc = doc_with_form();
        let xfdf = b"<?xml version=\"1.0\"?><xfdf><fields><field name=\"Name\"><value>Dave</value></field></fields></xfdf>";
        let count = import_xfdf(&mut doc, xfdf).unwrap();
        assert_eq!(count, 1, "should import 1 field");

        let form = doc.acro_form().unwrap();
        let field = form.get_field("Name").unwrap();
        let value = field.value().and_then(|v| v.as_string()).map(|s| String::from_utf8_lossy(s).to_string());
        assert_eq!(value, Some("Dave".to_string()));
    }

    #[test]
    fn test_import_empty_fdf() {
        let mut doc = doc_with_form();
        let count = import_fdf(&mut doc, b"").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_import_empty_xfdf() {
        let mut doc = doc_with_form();
        let count = import_xfdf(&mut doc, b"").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_tokenize_fdf_simple() {
        let tokens = tokenize_fdf("/T (Name) /V (Value)");
        assert!(tokens.contains(&"/T".to_string()));
        assert!(tokens.contains(&"(Name)".to_string()));
        assert!(tokens.contains(&"/V".to_string()));
        assert!(tokens.contains(&"(Value)".to_string()));
    }

    #[test]
    fn test_unescape_pdf_string() {
        assert_eq!(unescape_pdf_string("(Hello)"), "Hello");
        assert_eq!(unescape_pdf_string("(a\\(b\\)c)"), "a(b)c");
        assert_eq!(unescape_pdf_string("(a\\\\b)"), "a\\b");
    }

    #[test]
    fn test_xml_unescape() {
        assert_eq!(xml_unescape("a&amp;b"), "a&b");
        assert_eq!(xml_unescape("&lt;tag&gt;"), "<tag>");
    }
}
