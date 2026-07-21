use crate::Document;
use crate::cos::{CosObject, CosName};
use super::ValidationError;

/// Trait for individual PDF/A validation rules.
pub trait PreflightRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError>;
}

pub struct NoEncryptionRule;
impl PreflightRule for NoEncryptionRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        if doc.trailer().contains_key(&CosName::new(b"Encrypt".to_vec())) {
            errors.push(ValidationError {
                rule_id: "1.0",
                message: "PDF/A-1b documents must not be encrypted.".to_string(),
            });
        }
        errors
    }
}

pub struct NoLzwFilterRule;
impl PreflightRule for NoLzwFilterRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let filter_key = CosName::new(b"Filter".to_vec());
        let lzw_name = b"LZWDecode";
        let lzw_short = b"LZW";

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Stream(stream) = obj {
                let has_lzw = match stream.dictionary.get(&filter_key) {
                    Some(CosObject::Name(n)) => n.as_bytes() == lzw_name || n.as_bytes() == lzw_short,
                    Some(CosObject::Array(arr)) => arr.iter().any(|f| {
                        if let CosObject::Name(n) = f {
                            n.as_bytes() == lzw_name || n.as_bytes() == lzw_short
                        } else { false }
                    }),
                    _ => false,
                };
                if has_lzw {
                    errors.push(ValidationError {
                        rule_id: "2.0",
                        message: format!("PDF/A-1b forbids LZW compression (Obj {} {}).", id.object_number, id.generation),
                    });
                }
            }
        }
        errors
    }
}

/// PDF/A-1b forbids JavaScript actions.
pub struct NoJavaScriptRule;
impl PreflightRule for NoJavaScriptRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let js_key = CosName::new(b"JavaScript".to_vec());
        let js_short = CosName::new(b"JS".to_vec());
        let action_key = CosName::new(b"S".to_vec());

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                // Check if the dictionary contains a JavaScript key
                if dict.contains_key(&js_key) || dict.contains_key(&js_short) {
                    errors.push(ValidationError {
                        rule_id: "4.0",
                        message: format!("PDF/A-1b forbids JavaScript. Found JS entry in Obj {} {}", id.object_number, id.generation),
                    });
                }
                
                // Check if it's an Action dictionary of type JavaScript
                if let Some(CosObject::Name(action_type)) = dict.get(&action_key) {
                    if action_type.as_bytes() == b"JavaScript" {
                        errors.push(ValidationError {
                            rule_id: "4.1",
                            message: format!("PDF/A-1b forbids JavaScript Actions (Obj {} {}).", id.object_number, id.generation),
                        });
                    }
                }
            }
        }
        errors
    }
}

/// PDF/A-1b forbids Open Prepress Interface (OPI).
pub struct NoOpiRule;
impl PreflightRule for NoOpiRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let opi_key = CosName::new(b"OPI".to_vec());

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                if dict.contains_key(&opi_key) {
                    errors.push(ValidationError {
                        rule_id: "5.0",
                        message: format!("PDF/A-1b forbids OPI dictionaries (Obj {} {}).", id.object_number, id.generation),
                    });
                }
            }
        }
        errors
    }
}