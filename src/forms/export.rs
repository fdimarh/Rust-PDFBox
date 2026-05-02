//!
//! FDF and XFDF export — writes form field values to FDF (PDF-based) and
//! XFDF (XML) data files.
//!
//! Maps to Java PDFBox's `FDFCatalog` / `XFDF` export utilities.

use crate::cos::{CosDictionary, CosName, CosObject};
use crate::forms::field::{get_field_value_for_export};
use crate::{Document, PdfResult};

/// Exports all AcroForm field values to an FDF byte buffer (PDF-based format).
///
/// FDF (Forms Data Format) is a PDF-based file that contains field values
/// that can be imported back into a compatible PDF form.
///
/// # Arguments
///
/// * `doc` - Reference to the document with form fields.
///
/// # Returns
///
/// Raw FDF file bytes suitable for writing to a `.fdf` file.
pub fn export_fdf(doc: &Document) -> PdfResult<Vec<u8>> {
    let fields = collect_field_values(doc);
    if fields.is_empty() {
        return Ok(Vec::new());
    }

    let mut fdf_fields = Vec::new();
    for (name, value) in &fields {
        let mut field_dict = CosDictionary::new();
        field_dict.insert(
            CosName::new(b"T".to_vec()),
            CosObject::String(name.as_bytes().to_vec()),
        );
        if let Some(val) = value {
            field_dict.insert(
                CosName::new(b"V".to_vec()),
                CosObject::String(val.as_bytes().to_vec()),
            );
        }
        fdf_fields.push(CosObject::Dictionary(field_dict));
    }

    let mut fdf_dict = CosDictionary::new();
    fdf_dict.insert(
        CosName::new(b"Fields".to_vec()),
        CosObject::Array(fdf_fields),
    );

    // Build FDF document structure
    let mut root_dict = CosDictionary::new();
    root_dict.insert(CosName::new(b"FDF".to_vec()), CosObject::Dictionary(fdf_dict));

    // Serialize as a minimal PDF-like structure
    let mut out = Vec::new();
    out.extend_from_slice(b"%FDF-1.2\n");
    out.extend_from_slice(b"1 0 obj\n");
    out.extend_from_slice(serialize_cos(&CosObject::Dictionary(root_dict)).as_bytes());
    out.extend_from_slice(b"\nendobj\n");
    out.extend_from_slice(b"trailer\n");
    out.extend_from_slice(b"<< /Root 1 0 R >>\n");
    out.extend_from_slice(b"%%EOF\n");

    Ok(out)
}

/// Exports all AcroForm field values to an XFDF byte buffer (XML-based format).
///
/// XFDF (XML Forms Data Format) is an XML-based file format for form field data.
///
/// # Arguments
///
/// * `doc` - Reference to the document with form fields.
///
/// # Returns
///
/// Raw XFDF XML bytes suitable for writing to a `.xfdf` file.
pub fn export_xfdf(doc: &Document) -> PdfResult<Vec<u8>> {
    let fields = collect_field_values(doc);
    if fields.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.extend_from_slice(b"<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n");
    out.extend_from_slice(b"  <fields>\n");

    for (name, value) in &fields {
        out.extend_from_slice(b"    <field name=\"");
        out.extend_from_slice(xml_escape(name).as_bytes());
        out.extend_from_slice(b"\">\n");
        if let Some(val) = value {
            out.extend_from_slice(b"      <value>");
            out.extend_from_slice(xml_escape(val).as_bytes());
            out.extend_from_slice(b"</value>\n");
        }
        out.extend_from_slice(b"    </field>\n");
    }

    out.extend_from_slice(b"  </fields>\n");
    out.extend_from_slice(b"</xfdf>\n");

    Ok(out)
}

/// Collects field name/value pairs from the document's AcroForm.
fn collect_field_values(doc: &Document) -> Vec<(String, Option<String>)> {
    let form = match doc.acro_form() {
        Some(f) => f,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    for field in form.fields() {
        let name = field.fully_qualified_name();
        if name.is_empty() {
            continue;
        }
        let value = get_field_value_for_export(field.dictionary());
        result.push((name, value));
    }
    result
}

/// Escapes special XML characters.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Simple COS serializer for FDF output.
fn serialize_cos(obj: &CosObject) -> String {
    match obj {
        CosObject::Null => "null".to_string(),
        CosObject::Bool(b) => {
            if *b { "true" } else { "false" }.to_string()
        }
        CosObject::Integer(i) => i.to_string(),
        CosObject::Real(f) => f.to_string(),
        CosObject::Name(name) => format!("{}", name),
        CosObject::String(bytes) | CosObject::HexString(bytes) => {
            let s = String::from_utf8_lossy(bytes);
            format!("({})", escape_pdf_string(&s))
        }
        CosObject::Array(arr) => {
            let items: Vec<String> = arr.iter().map(serialize_cos).collect();
            format!("[{}]", items.join(" "))
        }
        CosObject::Dictionary(dict) => {
            let mut parts = Vec::new();
            for (key, val) in dict.iter() {
                parts.push(format!("/{} {}", key, serialize_cos(val)));
            }
            format!("<< {} >>", parts.join("\n"))
        }
        CosObject::Reference(id) => {
            format!("{} {} R", id.object_number, id.generation)
        }
        CosObject::Stream(_) => "<< /Length 0 >>\nstream\nendstream".to_string(),
    }
}

fn escape_pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
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
            5 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /Rect [100 700 300 720] /P 3 0 R /V (Alice) >>\nendobj\n\
            xref\n0 6\n0000000000 65535 f \n0000000009 00000 n \n0000000075 00000 n \n\
            0000000150 00000 n \n0000000288 00000 n \n0000000370 00000 n \n\
            trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n490\n%%EOF";
        let (doc, _) = Document::load_lenient(bytes);
        doc
    }

    #[test]
    fn test_export_fdf_non_empty() {
        let doc = doc_with_form();
        let fdf = export_fdf(&doc).unwrap();
        assert!(!fdf.is_empty(), "FDF output should not be empty");
        assert!(fdf.starts_with(b"%FDF-1.2"), "should start with FDF header");
        let s = String::from_utf8_lossy(&fdf);
        assert!(s.contains("Name"), "should contain field name");
        assert!(s.contains("Alice"), "should contain field value");
        assert!(s.contains("%%EOF"), "should end with EOF marker");
    }

    #[test]
    fn test_export_xfdf_non_empty() {
        let doc = doc_with_form();
        let xfdf = export_xfdf(&doc).unwrap();
        assert!(!xfdf.is_empty(), "XFDF output should not be empty");
        let s = String::from_utf8_lossy(&xfdf);
        assert!(s.contains("<?xml"), "should have XML declaration");
        assert!(s.contains("<xfdf"), "should have xfdf root");
        assert!(s.contains("Name"), "should contain field name");
        assert!(s.contains("Alice"), "should contain field value");
    }

    #[test]
    fn test_export_fdf_empty_doc() {
        let bytes = b"%PDF-1.7\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
            xref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000054 00000 n \n\
            0000000129 00000 n \ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n194\n%%EOF";
        let (doc, _) = Document::load_lenient(bytes);
        let fdf = export_fdf(&doc).unwrap();
        assert!(fdf.is_empty(), "FDF should be empty for doc without form");
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(xml_escape("hello"), "hello");
    }

    #[test]
    fn test_escape_pdf_string() {
        let s = escape_pdf_string("a(b)c");
        assert_eq!(s, "a\\(b\\)c");
    }
}
