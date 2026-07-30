use super::{rules::*, ValidationResult};
use crate::Document;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn pdf_a1b_has_all_rules() {
        let v = PreflightValidator::pdf_a1b();
        assert_eq!(v.rules.len(), 14);
    }

    #[test]
    fn with_rules_custom() {
        let rules: Vec<Box<dyn PreflightRule>> = vec![Box::new(NoEncryptionRule)];
        let v = PreflightValidator::with_rules(rules);
        assert_eq!(v.rules.len(), 1);
    }

    #[test]
    fn validate_empty_doc_passes_some_rules() {
        let doc = Document::empty();
        let v = PreflightValidator::with_rules(vec![
            Box::new(NoEncryptionRule),
            Box::new(NoJavaScriptRule),
        ]);
        let result = v.validate(&doc);
        // Empty doc has no encryption or JS
        assert!(result.is_valid);
    }

    #[test]
    fn validate_fails_no_encryption_on_encrypted_doc() {
        use crate::crypto::permissions::Permissions;
        use crate::protection::StandardProtectionPolicy;

        let mut doc = Document::empty();
        let policy =
            StandardProtectionPolicy::new("owner", "user", Permissions::from_bits_p(0xFFFFC0i32));
        doc.protect(&policy).unwrap();

        let v = PreflightValidator::with_rules(vec![Box::new(NoEncryptionRule)]);
        let result = v.validate(&doc);
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn validate_with_zero_rules() {
        let v = PreflightValidator::with_rules(vec![]);
        let doc = Document::empty();
        let result = v.validate(&doc);
        assert!(result.is_valid);
    }

    #[test]
    fn pdf_a1b_validate_empty_doc() {
        let v = PreflightValidator::pdf_a1b();
        let doc = Document::empty();
        let result = v.validate(&doc);
        // empty doc will fail some rules (metadata, output intent, etc.)
        assert!(!result.errors.is_empty());
    }
}
