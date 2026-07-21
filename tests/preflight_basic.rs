use rust_pdfbox::Document;
use rust_pdfbox::preflight::validator::PreflightValidator;

#[test]
fn test_preflight_valid_blank() {
    let doc = Document::empty();
    let validator = PreflightValidator::pdf_a1b();
    let result = validator.validate(&doc);
    
    // A blank document without encryption should pass the initial stub rules.
    assert!(result.is_valid);
    assert_eq!(result.errors.len(), 0);
}

#[test]
fn test_preflight_encrypted_fails() {
    let mut doc = Document::empty();
    
    // Inject a fake Encrypt dict into the trailer to trigger the NoEncryptionRule
    let mut encrypt_dict = rust_pdfbox::cos::CosDictionary::new();
    encrypt_dict.insert(
        rust_pdfbox::cos::CosName::new(b"Filter".to_vec()), 
        rust_pdfbox::cos::CosObject::Name(rust_pdfbox::cos::CosName::new(b"Standard".to_vec()))
    );
    doc.trailer_mut().insert(
        rust_pdfbox::cos::CosName::new(b"Encrypt".to_vec()), 
        rust_pdfbox::cos::CosObject::Dictionary(encrypt_dict)
    );
    
    let validator = PreflightValidator::pdf_a1b();
    let result = validator.validate(&doc);
    
    assert!(!result.is_valid);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].rule_id, "1.0");
    assert!(result.errors[0].message.contains("must not be encrypted"));
}