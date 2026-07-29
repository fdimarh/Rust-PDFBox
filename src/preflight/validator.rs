use crate::Document;
use super::{ValidationResult, rules::*};

/// Main validator for PDF/A specifications.
pub struct PreflightValidator {
    rules: Vec<Box<dyn PreflightRule>>,
}

impl PreflightValidator {
    /// Creates a new validator configured for PDF/A-1b.
    pub fn pdf_a1b() -> Self {
        let rules: Vec<Box<dyn PreflightRule>> = vec![
            // Security & encoding
            Box::new(NoEncryptionRule),
            Box::new(NoJavaScriptRule),
            Box::new(NoLaunchActionsRule),
            // Filters
            Box::new(NoLzwFilterRule),
            Box::new(NoDeprecatedFiltersRule),
            // Content restrictions
            Box::new(NoTransparencyRule),
            Box::new(NoOpiRule),
            Box::new(ColorSpaceRule),
            // Fonts & metadata
            Box::new(FontEmbeddingRule),
            Box::new(MetadataRule),
            Box::new(OutputIntentRule),
            // Pages & annotations
            Box::new(AnnotationRule),
            Box::new(PageRule),
            Box::new(EmbeddedFileRule),
        ];

        Self { rules }
    }

    /// Creates a validator with a custom set of rules.
    pub fn with_rules(rules: Vec<Box<dyn PreflightRule>>) -> Self {
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
