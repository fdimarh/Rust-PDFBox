use super::*;
use crate::cos::{CosObject, CosName, CosDictionary, CosStream, ObjectId};
use crate::Document;

// Build a Document with minimal trailer + catalog structure
fn doc_with_catalog() -> Document {
    let mut doc = Document::empty();
    let cat_id = ObjectId::new(1, 0);
    let trailer_cat_key = CosName::new(b"Root".to_vec());
    doc.objects.insert(cat_id, CosObject::Dictionary(CosDictionary::new()));
    doc.trailer_mut().insert(trailer_cat_key, CosObject::Reference(cat_id));
    doc
}

fn catalog_mut(doc: &mut Document) -> Option<&mut CosDictionary> {
    // Find the Root reference in trailer and return mutable dict
    let root_key = CosName::new(b"Root".to_vec());
    let root_ref = doc.trailer().get(&root_key)?.as_reference()?;
    let obj = doc.objects.get_mut(&root_ref)?;
    if let CosObject::Dictionary(d) = obj {
        Some(d)
    } else {
        None
    }
}

fn inject_dict(doc: &mut Document, dict: CosDictionary) {
    let id = ObjectId::new(999, 0);
    doc.objects.insert(id, CosObject::Dictionary(dict));
}

fn inject_stream_dict(doc: &mut Document, dict: CosDictionary) {
    let id = ObjectId::new(998, 0);
    doc.objects.insert(id, CosObject::Stream(CosStream { dictionary: dict, data: vec![] }));
}

// =======================================================================
// 1.0 NoEncryptionRule
// =======================================================================
#[test]
fn test_no_encryption_pass() {
    let doc = doc_with_catalog();
    let errs = NoEncryptionRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_no_encryption_fail() {
    let mut doc = doc_with_catalog();
    doc.trailer_mut().insert(CosName::new(b"Encrypt".to_vec()), CosObject::Dictionary(CosDictionary::new()));
    let errs = NoEncryptionRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "1.0");
}

// =======================================================================
// 2.0 NoLzwFilterRule
// =======================================================================
#[test]
fn test_no_lzw_filter_pass() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"Filter".to_vec()), CosObject::Name(CosName::new(b"FlateDecode".to_vec())));
    inject_stream_dict(&mut doc, dict);
    let errs = NoLzwFilterRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_no_lzw_filter_fail_name() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"Filter".to_vec()), CosObject::Name(CosName::new(b"LZWDecode".to_vec())));
    inject_stream_dict(&mut doc, dict);
    let errs = NoLzwFilterRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "2.0");
}

#[test]
fn test_no_lzw_filter_fail_array() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"Filter".to_vec()), CosObject::Array(vec![CosObject::Name(CosName::new(b"LZW".to_vec()))]));
    inject_stream_dict(&mut doc, dict);
    let errs = NoLzwFilterRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "2.0");
}

// =======================================================================
// 3.0 NoDeprecatedFiltersRule
// =======================================================================
#[test]
fn test_no_deprecated_filters_pass() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"Filter".to_vec()), CosObject::Name(CosName::new(b"FlateDecode".to_vec())));
    inject_stream_dict(&mut doc, dict);
    let errs = NoDeprecatedFiltersRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_no_deprecated_filters_fail_ascii85() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"Filter".to_vec()), CosObject::Name(CosName::new(b"ASCII85Decode".to_vec())));
    inject_stream_dict(&mut doc, dict);
    let errs = NoDeprecatedFiltersRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "3.0");
}

// =======================================================================
// 4.0 / 4.1 NoJavaScriptRule
// =======================================================================
#[test]
fn test_no_javascript_pass() {
    let doc = doc_with_catalog();
    let errs = NoJavaScriptRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_no_javascript_fail_js_entry() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"JavaScript".to_vec()), CosObject::Null);
    inject_dict(&mut doc, dict);
    let errs = NoJavaScriptRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "4.0");
}

#[test]
fn test_no_javascript_fail_action() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"S".to_vec()), CosObject::Name(CosName::new(b"JavaScript".to_vec())));
    inject_dict(&mut doc, dict);
    let errs = NoJavaScriptRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "4.1");
}

// =======================================================================
// 5.0 NoOpiRule
// =======================================================================
#[test]
fn test_no_opi_pass() {
    let doc = doc_with_catalog();
    let errs = NoOpiRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_no_opi_fail_dict() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"OPI".to_vec()), CosObject::Dictionary(CosDictionary::new()));
    inject_dict(&mut doc, dict);
    let errs = NoOpiRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "5.0");
}

#[test]
fn test_no_opi_fail_stream_dict() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"OPIversion".to_vec()), CosObject::Integer(1));
    inject_stream_dict(&mut doc, dict);
    let errs = NoOpiRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "5.0");
}

// =======================================================================
// 6.0 / 6.1 MetadataRule
// =======================================================================
#[test]
fn test_metadata_pass() {
    let mut doc = doc_with_catalog();
    if let Some(cat) = catalog_mut(&mut doc) {
        cat.insert(CosName::new(b"Metadata".to_vec()), CosObject::Stream(CosStream {
            dictionary: CosDictionary::new(),
            data: b"<?xml ...>".to_vec(),
        }));
    }
    let errs = MetadataRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_metadata_fail_missing() {
    let doc = doc_with_catalog();
    let errs = MetadataRule.validate(&doc);
    assert!(!errs.is_empty());
    assert_eq!(errs[0].rule_id, "6.0");
}

#[test]
fn test_metadata_fail_not_stream() {
    let mut doc = doc_with_catalog();
    if let Some(cat) = catalog_mut(&mut doc) {
        cat.insert(CosName::new(b"Metadata".to_vec()), CosObject::Null);
    }
    let errs = MetadataRule.validate(&doc);
    assert!(errs.iter().any(|e| e.rule_id == "6.1"));
}

// =======================================================================
// 7.0 FontEmbeddingRule
// =======================================================================
#[test]
fn test_font_embedding_pass() {
    let mut doc = doc_with_catalog();
    let mut fd = CosDictionary::new();
    fd.insert(CosName::type_name(), CosObject::Name(CosName::new(b"FontDescriptor".to_vec())));
    fd.insert(CosName::new(b"FontFile2".to_vec()), CosObject::Stream(CosStream {
        dictionary: CosDictionary::new(),
        data: vec![0; 100],
    }));
    inject_dict(&mut doc, fd);
    let errs = FontEmbeddingRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_font_embedding_fail_unembedded() {
    let mut doc = doc_with_catalog();
    let mut fd = CosDictionary::new();
    fd.insert(CosName::type_name(), CosObject::Name(CosName::new(b"FontDescriptor".to_vec())));
    fd.insert(CosName::new(b"Flags".to_vec()), CosObject::Integer(0));
    inject_dict(&mut doc, fd);
    let errs = FontEmbeddingRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "7.0");
}

#[test]
fn test_font_embedding_symbolic_exempt() {
    let mut doc = doc_with_catalog();
    let mut fd = CosDictionary::new();
    fd.insert(CosName::type_name(), CosObject::Name(CosName::new(b"FontDescriptor".to_vec())));
    fd.insert(CosName::new(b"Flags".to_vec()), CosObject::Integer(4));
    inject_dict(&mut doc, fd);
    let errs = FontEmbeddingRule.validate(&doc);
    assert!(errs.is_empty(), "symbolic fonts are exempt");
}

// =======================================================================
// 8.0 / 8.1 NoTransparencyRule
// =======================================================================
#[test]
fn test_no_transparency_pass() {
    let doc = doc_with_catalog();
    let errs = NoTransparencyRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_no_transparency_fail_smask() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"SMask".to_vec()), CosObject::Null);
    inject_stream_dict(&mut doc, dict);
    let errs = NoTransparencyRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "8.0");
}

#[test]
fn test_no_transparency_fail_group() {
    let mut doc = doc_with_catalog();
    let mut group = CosDictionary::new();
    group.insert(CosName::new(b"S".to_vec()), CosObject::Name(CosName::new(b"Transparency".to_vec())));
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"Group".to_vec()), CosObject::Dictionary(group));
    inject_dict(&mut doc, dict);
    let errs = NoTransparencyRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "8.1");
}

// =======================================================================
// 9.0 AnnotationRule
// =======================================================================
#[test]
fn test_annotation_pass() {
    let mut doc = doc_with_catalog();
    let mut annot = CosDictionary::new();
    annot.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Annot".to_vec())));
    annot.insert(CosName::new(b"F".to_vec()), CosObject::Integer(4));
    inject_dict(&mut doc, annot);
    let errs = AnnotationRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_annotation_fail_no_print() {
    let mut doc = doc_with_catalog();
    let mut annot = CosDictionary::new();
    annot.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Annot".to_vec())));
    annot.insert(CosName::new(b"F".to_vec()), CosObject::Integer(0));
    inject_dict(&mut doc, annot);
    let errs = AnnotationRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "9.0");
}

// =======================================================================
// 10.0 / 10.1 / 10.2 OutputIntentRule
// =======================================================================
#[test]
fn test_output_intent_pass() {
    let mut doc = doc_with_catalog();
    let mut oi = CosDictionary::new();
    oi.insert(CosName::new(b"DestOutputProfile".to_vec()), CosObject::Stream(CosStream {
        dictionary: CosDictionary::new(),
        data: vec![0; 10],
    }));
    if let Some(cat) = catalog_mut(&mut doc) {
        cat.insert(CosName::new(b"OutputIntents".to_vec()), CosObject::Array(vec![CosObject::Dictionary(oi)]));
    }
    let errs = OutputIntentRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_output_intent_fail_missing() {
    let doc = doc_with_catalog();
    let errs = OutputIntentRule.validate(&doc);
    assert!(!errs.is_empty());
    assert_eq!(errs[0].rule_id, "10.0");
}

#[test]
fn test_output_intent_fail_missing_profile() {
    let mut doc = doc_with_catalog();
    let mut oi = CosDictionary::new();
    oi.insert(CosName::new(b"S".to_vec()), CosObject::Name(CosName::new(b"GTS_PDFA1".to_vec())));
    if let Some(cat) = catalog_mut(&mut doc) {
        cat.insert(CosName::new(b"OutputIntents".to_vec()), CosObject::Array(vec![CosObject::Dictionary(oi)]));
    }
    let errs = OutputIntentRule.validate(&doc);
    assert!(errs.iter().any(|e| e.rule_id == "10.1"));
}

// =======================================================================
// 11.0 NoLaunchActionsRule
// =======================================================================
#[test]
fn test_no_launch_actions_pass() {
    let doc = doc_with_catalog();
    let errs = NoLaunchActionsRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_no_launch_actions_fail() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"S".to_vec()), CosObject::Name(CosName::new(b"Launch".to_vec())));
    inject_dict(&mut doc, dict);
    let errs = NoLaunchActionsRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "11.0");
}

// =======================================================================
// 12.0 ColorSpaceRule
// =======================================================================
#[test]
fn test_color_space_pass() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"ColorSpace".to_vec()), CosObject::Name(CosName::new(b"DeviceRGB".to_vec())));
    inject_dict(&mut doc, dict);
    let errs = ColorSpaceRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_color_space_fail_calrgb() {
    let mut doc = doc_with_catalog();
    let mut dict = CosDictionary::new();
    dict.insert(CosName::new(b"ColorSpace".to_vec()), CosObject::Name(CosName::new(b"CalRGB".to_vec())));
    inject_dict(&mut doc, dict);
    let errs = ColorSpaceRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "12.0");
}

// =======================================================================
// 13.0 / 13.1 PageRule
// =======================================================================
#[test]
fn test_page_rule_pass() {
    let doc = doc_with_catalog();
    let errs = PageRule.validate(&doc);
    assert!(errs.is_empty());
}

#[test]
fn test_page_rule_fail_openaction() {
    let mut doc = doc_with_catalog();
    if let Some(cat) = catalog_mut(&mut doc) {
        cat.insert(CosName::new(b"OpenAction".to_vec()), CosObject::Dictionary(CosDictionary::new()));
    }
    let errs = PageRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "13.0");
}

#[test]
fn test_page_rule_fail_additional_actions() {
    let mut doc = doc_with_catalog();
    let mut page_dict = CosDictionary::new();
    page_dict.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Page".to_vec())));
    page_dict.insert(CosName::new(b"AA".to_vec()), CosObject::Dictionary(CosDictionary::new()));
    inject_dict(&mut doc, page_dict);
    let errs = PageRule.validate(&doc);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].rule_id, "13.1");
}

// =======================================================================
// 14.0 EmbeddedFileRule tests
// =======================================================================
#[test]
fn test_embedded_file_stub() {
    let doc = doc_with_catalog();
    let errs = EmbeddedFileRule.validate(&doc);
    assert!(errs.is_empty(), "stub: no errors yet");
}

#[test]
fn test_embedded_file_detects_non_pdf() {
    let mut doc = doc_with_catalog();
    let mut d = CosDictionary::new();
    d.insert(CosName::type_name(), CosObject::Name(CosName::new(b"EmbeddedFile".to_vec())));
    d.insert(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"text/plain".to_vec())));
    inject_stream_dict(&mut doc, d);
    let errs = EmbeddedFileRule.validate(&doc);
    assert!(errs.iter().any(|e| e.rule_id == "14.1"));
}

#[test]
fn test_embedded_file_passes_pdf() {
    let mut doc = doc_with_catalog();
    let mut d = CosDictionary::new();
    d.insert(CosName::type_name(), CosObject::Name(CosName::new(b"EmbeddedFile".to_vec())));
    d.insert(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"application/pdf".to_vec())));
    inject_stream_dict(&mut doc, d);
    let errs = EmbeddedFileRule.validate(&doc);
    assert!(errs.is_empty(), "PDF embedded file should pass: {:?}", errs);
}

// =======================================================================
// PreflightValidator integration
// =======================================================================
#[test]
fn test_validate_all_rules_loaded() {
    let validator = crate::preflight::PreflightValidator::pdf_a1b();
    let doc = doc_with_catalog();
    let result = validator.validate(&doc);
    assert!(!result.is_valid, "doc with catalog but no Metadata should fail");
    assert!(!result.errors.is_empty(), "should produce errors");
}
