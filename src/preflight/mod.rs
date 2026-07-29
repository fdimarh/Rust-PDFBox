//! PDF/A Preflight Validation engine.
//!
//! Maps to Java PDFBox `org.apache.pdfbox.preflight`.
//! Currently focuses on basic PDF/A-1b (ISO 19005-1) compliance.

pub mod rules;
pub mod validator;

use std::fmt;

/// Preflight validation result.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
}

/// A specific violation of the PDF/A specification.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub rule_id: &'static str,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.rule_id, self.message)
    }
}

pub use validator::PreflightValidator;
