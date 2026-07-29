use crate::Document;
use crate::cos::{CosObject, CosName};
use super::ValidationError;

/// Trait for individual PDF/A validation rules.
pub trait PreflightRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError>;
    /// Unique identifier for this rule (e.g. "1.0").
    fn id(&self) -> &'static str { "" }
}

// ── 1.0 Encryption Rule ────────────────────────────────────────────────

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
    fn id(&self) -> &'static str { "1.0" }
}

// ── 2.0 LZW Filter Rule ────────────────────────────────────────────────

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
    fn id(&self) -> &'static str { "2.0" }
}

// ── 3.0 ASCII85 / ASCIIHex Filter Rule ─────────────────────────────────
// ISO 19005-1:2005 §6.1.3 — these are deprecated in PDF/A-1b

pub struct NoDeprecatedFiltersRule;
impl PreflightRule for NoDeprecatedFiltersRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let filter_key = CosName::new(b"Filter".to_vec());
        let deprecated: &[&[u8]] = &[b"ASCII85Decode", b"ASCIIHexDecode"];

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Stream(stream) = obj {
                let has_dep = match stream.dictionary.get(&filter_key) {
                    Some(CosObject::Name(n)) => deprecated.iter().any(|d| n.as_bytes() == *d),
                    Some(CosObject::Array(arr)) => arr.iter().any(|f| {
                        if let CosObject::Name(n) = f {
                            deprecated.iter().any(|d| n.as_bytes() == *d)
                        } else { false }
                    }),
                    _ => false,
                };
                if has_dep {
                    errors.push(ValidationError {
                        rule_id: "3.0",
                        message: format!("PDF/A-1b discourages deprecated filters (Obj {} {}).", id.object_number, id.generation),
                    });
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "3.0" }
}

// ── 4.0 JavaScript Rule ────────────────────────────────────────────────

pub struct NoJavaScriptRule;
impl PreflightRule for NoJavaScriptRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let js_key = CosName::new(b"JavaScript".to_vec());
        let js_short = CosName::new(b"JS".to_vec());
        let action_key = CosName::new(b"S".to_vec());

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                if dict.contains_key(&js_key) || dict.contains_key(&js_short) {
                    errors.push(ValidationError {
                        rule_id: "4.0",
                        message: format!("PDF/A-1b forbids JavaScript. Found JS entry in Obj {} {}", id.object_number, id.generation),
                    });
                }

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
    fn id(&self) -> &'static str { "4.0" }
}

// ── 5.0 OPI Rule ───────────────────────────────────────────────────────

pub struct NoOpiRule;
impl PreflightRule for NoOpiRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let opi_keys = [
            CosName::new(b"OPI".to_vec()),
            CosName::new(b"OPIversion".to_vec()),
        ];

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                for key in &opi_keys {
                    if dict.contains_key(key) {
                        errors.push(ValidationError {
                            rule_id: "5.0",
                            message: format!("PDF/A-1b forbids OPI references (Obj {} {}).", id.object_number, id.generation),
                        });
                    }
                }
            }
            if let CosObject::Stream(stream) = obj {
                let dict = &stream.dictionary;
                for key in &opi_keys {
                    if dict.contains_key(key) {
                        errors.push(ValidationError {
                            rule_id: "5.0",
                            message: format!("PDF/A-1b forbids OPI references (Obj {} {}).", id.object_number, id.generation),
                        });
                    }
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "5.0" }
}

// ── 6.0 Metadata Rule ─────────────────────────────────────────────────
// ISO 19005-1:2005 §6.7.3 — XMP Metadata is required

pub struct MetadataRule;
impl PreflightRule for MetadataRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let catalog = match doc.catalog() {
            Some(c) => c,
            None => {
                errors.push(ValidationError {
                    rule_id: "6.0",
                    message: "Missing document catalog.".to_string(),
                });
                return errors;
            }
        };

        if !catalog.contains_key(&CosName::new(b"Metadata".to_vec())) {
            errors.push(ValidationError {
                rule_id: "6.0",
                message: "PDF/A-1b requires XMP Metadata in the document catalog.".to_string(),
            });
        }

        if let Some(md_obj) = catalog.get(&CosName::new(b"Metadata".to_vec())) {
            let resolved = doc.objects.resolve(md_obj);
            match resolved {
                Some(CosObject::Stream(_)) => {} // OK
                _ => {
                    errors.push(ValidationError {
                        rule_id: "6.1",
                        message: "Catalog /Metadata must be a stream.".to_string(),
                    });
                }
            }
        }

        errors
    }
    fn id(&self) -> &'static str { "6.0" }
}

// ── 7.0 Font Rule ──────────────────────────────────────────────────────
// ISO 19005-1:2005 §6.3 — all fonts must be embedded

pub struct FontEmbeddingRule;
impl PreflightRule for FontEmbeddingRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                let type_name = dict.get(&CosName::type_name())
                    .and_then(|o| o.as_name())
                    .map(|n| n.as_bytes());

                if type_name == Some(b"FontDescriptor") {
                    let has_fontfile = dict.contains_key(&CosName::new(b"FontFile".to_vec()))
                        || dict.contains_key(&CosName::new(b"FontFile2".to_vec()))
                        || dict.contains_key(&CosName::new(b"FontFile3".to_vec()));

                    if !has_fontfile {
                        let is_symbolic = dict.get(&CosName::new(b"Flags".to_vec()))
                            .and_then(|o| o.as_integer())
                            .map(|f| (f & 4) != 0)
                            .unwrap_or(false);

                        if !is_symbolic {
                            errors.push(ValidationError {
                                rule_id: "7.0",
                                message: format!("PDF/A-1b requires embedded fonts. FontDescriptor {} {} has no FontFile.", id.object_number, id.generation),
                            });
                        }
                    }
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "7.0" }
}

// ── 8.0 Transparency Rule ───────────────────────────────────────────────
// ISO 19005-1:2005 §6.2 — transparency is forbidden

pub struct NoTransparencyRule;
impl PreflightRule for NoTransparencyRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let forbidden = [
            CosName::new(b"SMask".to_vec()),
            CosName::new(b"SMaskInData".to_vec()),
            CosName::new(b"ca".to_vec()),
            CosName::new(b"CA".to_vec()),
            CosName::new(b"BM".to_vec()),
        ];

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Stream(stream) = obj {
                for key in &forbidden {
                    if stream.dictionary.contains_key(key) {
                        errors.push(ValidationError {
                            rule_id: "8.0",
                            message: format!("PDF/A-1b forbids transparency (key {:?} in Obj {} {}).",
                                String::from_utf8_lossy(key.as_bytes()), id.object_number, id.generation),
                        });
                    }
                }
            }
            if let CosObject::Dictionary(dict) = obj {
                if dict.contains_key(&CosName::new(b"Group".to_vec())) {
                    if let Some(CosObject::Dictionary(g)) = dict.get(&CosName::new(b"Group".to_vec())) {
                        let s = g.get(&CosName::new(b"S".to_vec()))
                            .and_then(|o| o.as_name())
                            .map(|n| n.as_bytes());
                        if s == Some(b"Transparency") {
                            errors.push(ValidationError {
                                rule_id: "8.1",
                                message: format!("PDF/A-1b forbids transparency groups (Obj {} {}).", id.object_number, id.generation),
                            });
                        }
                    }
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "8.0" }
}

// ── 9.0 Annotation Rule ────────────────────────────────────────────────
// ISO 19005-1:2005 §6.8 — annotations must have /F print flag set

pub struct AnnotationRule;
impl PreflightRule for AnnotationRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let type_key = CosName::type_name();
        let f_key = CosName::new(b"F".to_vec());

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                let is_annot = dict.get(&type_key)
                    .and_then(|o| o.as_name())
                    .map(|n| n.as_bytes() == b"Annot")
                    .unwrap_or(false);

                if is_annot {
                    let flags = dict.get(&f_key).and_then(|o| o.as_integer()).unwrap_or(0);
                    if (flags & 4) == 0 {
                        errors.push(ValidationError {
                            rule_id: "9.0",
                            message: format!("PDF/A-1b requires annotation print flag (Obj {} {}).", id.object_number, id.generation),
                        });
                    }
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "9.0" }
}

// ── 10.0 Output Intent Rule ────────────────────────────────────────────
// ISO 19005-1:2005 §6.2.8 — must specify /OutputIntents

pub struct OutputIntentRule;
impl PreflightRule for OutputIntentRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let catalog = match doc.catalog() {
            Some(c) => c,
            None => return errors,
        };

        let output_intents = catalog.get(&CosName::new(b"OutputIntents".to_vec()));
        match output_intents {
            Some(CosObject::Array(arr)) if !arr.is_empty() => {
                for (i, intent) in arr.iter().enumerate() {
                    if let Some(d) = intent.as_dictionary() {
                        if !d.contains_key(&CosName::new(b"DestOutputProfile".to_vec())) {
                            errors.push(ValidationError {
                                rule_id: "10.1",
                                message: format!("OutputIntent[{}] missing /DestOutputProfile.", i),
                            });
                        }
                    }
                }
            }
            Some(_) => {
                errors.push(ValidationError {
                    rule_id: "10.2",
                    message: "OutputIntents must be an array.".to_string(),
                });
            }
            None => {
                errors.push(ValidationError {
                    rule_id: "10.0",
                    message: "PDF/A-1b requires /OutputIntents in the catalog.".to_string(),
                });
            }
        }

        errors
    }
    fn id(&self) -> &'static str { "10.0" }
}

// ── 11.0 Action Rule ───────────────────────────────────────────────────
// ISO 19005-1:2005 §6.6 — no launch or hide actions

pub struct NoLaunchActionsRule;
impl PreflightRule for NoLaunchActionsRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let s_key = CosName::new(b"S".to_vec());
        let forbidden: &[&[u8]] = &[b"Launch", b"Sound", b"Movie", b"Hide", b"ResetForm", b"ImportData"];

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                if let Some(CosObject::Name(action_type)) = dict.get(&s_key) {
                    if forbidden.iter().any(|f| action_type.as_bytes() == *f) {
                        errors.push(ValidationError {
                            rule_id: "11.0",
                            message: format!("PDF/A-1b forbids {} actions (Obj {} {}).",
                                String::from_utf8_lossy(action_type.as_bytes()), id.object_number, id.generation),
                        });
                    }
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "11.0" }
}

// ── 12.0 Color Space Rule ──────────────────────────────────────────────
// ISO 19005-1:2005 §6.2 — only CMYK, Gray, RGB, or ICC-based

pub struct ColorSpaceRule;
impl PreflightRule for ColorSpaceRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let cs_key = CosName::new(b"ColorSpace".to_vec());

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                if let Some(cs) = dict.get(&cs_key) {
                    self.check_cs(cs, id, &mut errors);
                }
            }
            if let CosObject::Stream(stream) = obj {
                if let Some(cs) = stream.dictionary.get(&cs_key) {
                    self.check_cs(cs, id, &mut errors);
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "12.0" }
}

impl ColorSpaceRule {
    fn check_cs(&self, obj: &CosObject, id: &crate::cos::ObjectId, errors: &mut Vec<ValidationError>) {
        let names = match obj {
            CosObject::Name(n) => vec![n.as_bytes()],
            CosObject::Array(arr) => arr.iter().filter_map(|o| o.as_name().map(|n| n.as_bytes())).collect(),
            _ => return,
        };
        for name in &names {
            if *name == b"CalRGB" || *name == b"CalGray" || *name == b"Lab" {
                errors.push(ValidationError {
                    rule_id: "12.0",
                    message: format!("PDF/A-1b forbids {} color space (Obj {} {}).",
                        String::from_utf8_lossy(name), id.object_number, id.generation),
                });
            }
        }
    }
}

// ── 13.0 Page Rule ─────────────────────────────────────────────────────
// Ensure page resources don't contain Actions

pub struct PageRule;
impl PreflightRule for PageRule {
    fn validate(&self, doc: &Document) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let aa_key = CosName::new(b"AA".to_vec());
        let open_action = CosName::new(b"OpenAction".to_vec());

        let catalog = match doc.catalog() {
            Some(c) => c,
            None => return errors,
        };

        if catalog.contains_key(&open_action) {
            errors.push(ValidationError {
                rule_id: "13.0",
                message: "PDF/A-1b forbids /OpenAction in the catalog.".to_string(),
            });
        }

        for (id, obj) in doc.objects.iter() {
            if let CosObject::Dictionary(dict) = obj {
                let is_page = dict.get(&CosName::type_name())
                    .and_then(|o| o.as_name())
                    .map(|n| n.as_bytes() == b"Page")
                    .unwrap_or(false);
                if is_page && dict.contains_key(&aa_key) {
                    errors.push(ValidationError {
                        rule_id: "13.1",
                        message: format!("PDF/A-1b forbids page-level additional actions (Page {} {}).", id.object_number, id.generation),
                    });
                }
            }
        }
        errors
    }
    fn id(&self) -> &'static str { "13.0" }
}

// ── 14.0 Embedded File Rule ────────────────────────────────────────────
// ISO 19005-1:2005 §6.2.11 — embedded files must be PDF/A

pub struct EmbeddedFileRule;
impl PreflightRule for EmbeddedFileRule {
    fn validate(&self, _doc: &Document) -> Vec<ValidationError> {
        // Placeholder: complex spec — check for EF / embedded files
        Vec::new()
    }
    fn id(&self) -> &'static str { "14.0" }
}

// =======================================================================
// Tests
// =======================================================================
#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
