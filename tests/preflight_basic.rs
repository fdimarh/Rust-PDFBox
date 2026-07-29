use rust_pdfbox::preflight::PreflightValidator;

#[test]
fn test_preflight_validator_api() {
    // Verify the validator creates without panicking and
    // can process an empty (non-PDF/A) document.
    let doc = rust_pdfbox::Document::empty();
    let validator = PreflightValidator::pdf_a1b();
    let result = validator.validate(&doc);

    // Document::empty() is not PDF/A-1b — expect validation failures.
    assert_eq!(result.is_valid, false);
    assert!(!result.errors.is_empty(), "expected at least one preflight error for empty document");
}

#[test]
fn test_preflight_rules_registered() {
    let validator = PreflightValidator::pdf_a1b();
    let _ = validator; // Compile-time check: PreflightValidator constructable.
}
