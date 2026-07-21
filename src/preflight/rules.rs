use crate::Document;
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
        if doc.trailer().contains_key(&crate::cos::CosName::new(b"Encrypt".to_vec())) {
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
    fn validate(&self, _doc: &Document) -> Vec<ValidationError> {
        let errors = Vec::new();
        // TODO: Iterate over all stream objects in the document and check if 
        // /Filter == /LZWDecode.
        errors
    }
}
