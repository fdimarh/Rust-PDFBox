use crate::Document;
use crate::cos::{CosObject, CosName};
use super::ValidationError;

/// Trait for individual PDF/A validation rules.
pub trait PreflightRule {
    /// Evaluate the rule against the document. Returns a list of errors if violated.
    fn validate(&self, doc: &Document) -> Vec<ValidationError>;
}

// ---------------------------------------------------------------------------
// Basic Rules
// ---------------------------------------------------------------------------

/// PDF/A documents must not be encrypted.
pub struct NoEncryptionRule;

impl PreflightRule for NoEncryptionRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        if doc.trailer().contains_key(&CosName::new(b"Encrypt".to_vec())) {
            errors.push(ValidationError {
                rule_id: "1.0",
                message: "PDF/A-1b documents must not be encrypted (Encrypt dictionary found).".to_string(),
            });
        }
        errors
    }
}

/// PDF/A-1b forbids LZW compression.
pub struct NoLzwFilterRule;

impl PreflightRule for NoLzwFilterRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        
        let lzw_name = b"LZWDecode";
        let lzw_short = b"LZW";
        let filter_key = CosName::new(b"Filter".to_vec());

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Stream(stream) = obj {
                let has_lzw = match stream.dictionary.get(&filter_key) {
                    Some(CosObject::Name(n)) => n.as_bytes() == lzw_name || n.as_bytes() == lzw_short,
                    Some(CosObject::Array(arr)) => arr.iter().any(|f| {
                        if let CosObject::Name(n) = f {
                            n.as_bytes() == lzw_name || n.as_bytes() == lzw_short
                        } else {
                            false
                        }
                    }),
                    _ => false,
                };

                if has_lzw {
                    errors.push(ValidationError {
                        rule_id: "2.0",
                        message: format!("PDF/A-1b forbids LZW compression. Found in object {} {}", id.object_number, id.generation),
                    });
                }
            }
        }
        
        errors
    }
}

/// PDF/A-1b mandates that all fonts used in the document must be embedded.
pub struct FontEmbeddingRule;

impl PreflightRule for FontEmbeddingRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        
        let type_key = CosName::new(b"Type".to_vec());
        let subtype_key = CosName::new(b"Subtype".to_vec());
        let desc_key = CosName::new(b"FontDescriptor".to_vec());
        let basefont_key = CosName::new(b"BaseFont".to_vec());
        
        let file1_key = CosName::new(b"FontFile".to_vec());
        let file2_key = CosName::new(b"FontFile2".to_vec());
        let file3_key = CosName::new(b"FontFile3".to_vec());

        // Scan for Font dictionaries and check for FontFile streams
        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                if let Some(CosObject::Name(n)) = dict.get(&type_key) {
                    if n.as_bytes() == b"Font" {
                        let subtype = dict.get(&subtype_key).and_then(|o| o.as_name()).map(|n| n.as_bytes());
                        
                        if subtype == Some(b"Type3") {
                            continue;
                        }

                        let has_file = match dict.get(&desc_key) {
                            Some(CosObject::Reference(desc_id)) => {
                                if let Some(CosObject::Dictionary(desc_dict)) = doc.get_object_ref(desc_id.clone()) {
                                    desc_dict.contains_key(&file1_key) || 
                                    desc_dict.contains_key(&file2_key) ||
                                    desc_dict.contains_key(&file3_key)
                                } else {
                                    false
                                }
                            },
                            Some(CosObject::Dictionary(desc_dict)) => {
                                desc_dict.contains_key(&file1_key) || 
                                desc_dict.contains_key(&file2_key) ||
                                desc_dict.contains_key(&file3_key)
                            }
                            _ => false,
                        };

                        if !has_file {
                            let font_name = dict.get(&basefont_key).and_then(|o| o.as_name()).map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned()).unwrap_or_else(|| "Unknown".to_string());
                            
                            errors.push(ValidationError {
                                rule_id: "3.0",
                                message: format!("PDF/A-1b requires all fonts to be embedded. Font '{}' (Obj {} {}) lacks a FontFile stream.", font_name, id.object_number, id.generation),
                            });
                        }
                    }
                }
            }
        }

        errors
    }
}