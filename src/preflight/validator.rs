use crate::Document;
use super::{ValidationResult, rules::*};

/// Main validator for PDF/A specifications.
pub struct PreflightValidator {
    rules: Vec<Box<dyn PreflightRule>>,
}

impl PreflightValidator {
    /// Creates a new validator configured for PDF/A-1b.
    pub fn pdf_a1b() -> Self {
        let mut rules: Vec<Box<dyn PreflightRule>> = Vec::new();
        
        // Add ISO 19005-1 (PDF/A-1b) rules
        rules.push(Box::new(NoEncryptionRule));
        rules.push(Box::new(NoLzwFilterRule));
        rules.push(Box::new(FontEmbeddingRule));
        
        // TODO: MetadataRule, ColorSpaceRule
        
        Self { rules }
    }

    /// Runs all configured rules against the given PDF document.
    pub fn validate(&self, doc: &Document) -> ValidationResult {
        let mut all_errors = Vec::new();

        for rule in &self.rules {
            let errors = rule.validate(doc);
            all_errors.extend(errors);
        }

        ValidationResult {
            is_valid: all_errors.is_empty(),
            errors: all_errors,
        }
    }
}