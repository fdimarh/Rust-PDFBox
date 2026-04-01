//! Compatibility test harness — validates rust-pdfbox against Java PDFBox outputs.
//!
//! This module provides utilities to:
//! - Load PDFs with both implementations
//! - Compare normalized outputs
//! - Generate compatibility reports per feature
//!
//! Run with: `cargo test --test compat_pdfbox`

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Feature set (what we're testing)
// ---------------------------------------------------------------------------

/// Defines which features of a PDF are being compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// Basic load + page count
    Structure,
    /// Catalog, pages, resources
    Metadata,
    /// Page dimensions, rotation, media box
    PageGeometry,
    /// Extracted text content
    TextContent,
    /// Permission flags, encryption dict
    Permissions,
    /// Stream filters, decompression
    StreamDecoding,
    /// Font names, encodings
    FontInfo,
}

impl Feature {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::Metadata => "metadata",
            Self::PageGeometry => "page_geometry",
            Self::TextContent => "text_content",
            Self::Permissions => "permissions",
            Self::StreamDecoding => "stream_decoding",
            Self::FontInfo => "font_info",
        }
    }
}

// ---------------------------------------------------------------------------
// Normalized output for comparison
// ---------------------------------------------------------------------------

/// Normalized PDF output for comparison.
///
/// All values are reduced to canonical form to allow cross-implementation
/// comparison (Rust vs Java PDFBox).
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedOutput {
    /// File path (for logging)
    pub path: PathBuf,
    /// PDF version string (e.g., "1.4")
    pub version: String,
    /// Page count
    pub page_count: usize,
    /// Page dimensions: (page_index, width, height)
    pub page_sizes: Vec<(usize, f64, f64)>,
    /// Extracted text per page (first 500 chars to avoid huge diffs)
    pub page_texts: Vec<(usize, String)>,
    /// Document permissions flags (as sorted bit names)
    pub permissions: Vec<String>,
    /// Font names found in document
    pub fonts_used: Vec<String>,
}

impl NormalizedOutput {
    /// Creates a new normalized output with empty collections.
    pub fn new(path: impl Into<PathBuf>, version: String, page_count: usize) -> Self {
        Self {
            path: path.into(),
            version,
            page_count,
            page_sizes: Vec::new(),
            page_texts: Vec::new(),
            permissions: Vec::new(),
            fonts_used: Vec::new(),
        }
    }

    /// Add a page size measurement.
    pub fn add_page_size(&mut self, index: usize, width: f64, height: f64) {
        self.page_sizes.push((index, width, height));
    }

    /// Add extracted text from a page.
    pub fn add_page_text(&mut self, index: usize, text: String) {
        let truncated = if text.len() > 500 {
            text[..500].to_string()
        } else {
            text
        };
        self.page_texts.push((index, truncated));
    }

    /// Add a permission flag name.
    pub fn add_permission(&mut self, flag: String) {
        self.permissions.push(flag);
    }

    /// Add a font name.
    pub fn add_font(&mut self, name: String) {
        if !self.fonts_used.contains(&name) {
            self.fonts_used.push(name);
        }
    }
}

// ---------------------------------------------------------------------------
// Diff result
// ---------------------------------------------------------------------------

/// Comparison result for a single feature.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffResult {
    /// No differences found.
    Match,
    /// Differences found (as human-readable string).
    Mismatch(String),
    /// Could not compare (e.g., feature not yet implemented).
    NotImplemented(String),
}

impl DiffResult {
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Match)
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Match => "✓ Match".to_string(),
            Self::Mismatch(msg) => format!("✗ {}", msg),
            Self::NotImplemented(reason) => format!("⊗ Not implemented: {}", reason),
        }
    }
}

// ---------------------------------------------------------------------------
// Compatibility report
// ---------------------------------------------------------------------------

/// Full compatibility report for a PDF file.
#[derive(Debug, Clone)]
pub struct CompatReport {
    /// File being tested
    pub file: PathBuf,
    /// Feature → diff result
    pub results: HashMap<Feature, DiffResult>,
    /// Overall pass/fail
    pub passed: bool,
}

impl CompatReport {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self {
            file: file.into(),
            results: HashMap::new(),
            passed: true,
        }
    }

    /// Record a feature comparison result.
    pub fn add_result(&mut self, feature: Feature, result: DiffResult) {
        if !result.is_match() {
            self.passed = false;
        }
        self.results.insert(feature, result);
    }

    /// Generate a human-readable report.
    pub fn to_string(&self) -> String {
        let mut s = format!("File: {}\n", self.file.display());
        s.push_str(if self.passed { "Status: ✓ PASS\n" } else { "Status: ✗ FAIL\n" });
        s.push_str("\nFeatures:\n");

        let mut features: Vec<_> = self.results.keys().collect();
        features.sort_by_key(|f| f.as_str());

        for feature in features {
            let result = &self.results[feature];
            s.push_str(&format!("  {}: {}\n", feature.as_str(), result.summary()));
        }

        s
    }
}

// ---------------------------------------------------------------------------
// Comparison functions
// ---------------------------------------------------------------------------

/// Compare two normalized outputs and return diff results per feature.
pub fn compare_outputs(
    rust_out: &NormalizedOutput,
    java_out: &NormalizedOutput,
) -> HashMap<Feature, DiffResult> {
    let mut results = HashMap::new();

    // Structure: page count must match
    if rust_out.page_count != java_out.page_count {
        results.insert(
            Feature::Structure,
            DiffResult::Mismatch(format!(
                "page count: rust={}, java={}",
                rust_out.page_count, java_out.page_count
            )),
        );
    } else {
        results.insert(Feature::Structure, DiffResult::Match);
    }

    // Metadata: version check
    if rust_out.version != java_out.version {
        results.insert(
            Feature::Metadata,
            DiffResult::Mismatch(format!(
                "pdf version: rust={}, java={}",
                rust_out.version, java_out.version
            )),
        );
    } else {
        results.insert(Feature::Metadata, DiffResult::Match);
    }

    // Page geometry: compare page sizes (allow small rounding errors)
    let mut geom_match = true;
    for ((idx_r, w_r, h_r), (idx_j, w_j, h_j)) in
        rust_out.page_sizes.iter().zip(java_out.page_sizes.iter())
    {
        if idx_r != idx_j || (w_r - w_j).abs() > 0.01 || (h_r - h_j).abs() > 0.01 {
            geom_match = false;
            break;
        }
    }
    results.insert(
        Feature::PageGeometry,
        if geom_match {
            DiffResult::Match
        } else {
            DiffResult::Mismatch("page dimensions differ".to_string())
        },
    );

    // Text content: check if text lengths are within 10% (exact match is hard)
    let mut text_match = true;
    for ((idx_r, text_r), (idx_j, text_j)) in
        rust_out.page_texts.iter().zip(java_out.page_texts.iter())
    {
        if idx_r != idx_j {
            text_match = false;
            break;
        }
        let len_diff = ((text_r.len() as i32 - text_j.len() as i32).abs() as f64)
            / (text_j.len() as f64 + 1.0);
        if len_diff > 0.1 {
            text_match = false;
            break;
        }
    }
    results.insert(
        Feature::TextContent,
        if text_match {
            DiffResult::Match
        } else {
            DiffResult::Mismatch("extracted text differs significantly".to_string())
        },
    );

    // Permissions: exact match of sorted flags
    if rust_out.permissions == java_out.permissions {
        results.insert(Feature::Permissions, DiffResult::Match);
    } else {
        results.insert(
            Feature::Permissions,
            DiffResult::Mismatch(format!(
                "flags: rust={:?}, java={:?}",
                rust_out.permissions, java_out.permissions
            )),
        );
    }

    // Font info: check if font names are present (not exact order)
    let rust_fonts: std::collections::HashSet<_> = rust_out.fonts_used.iter().cloned().collect();
    let java_fonts: std::collections::HashSet<_> = java_out.fonts_used.iter().cloned().collect();
    if rust_fonts == java_fonts {
        results.insert(Feature::FontInfo, DiffResult::Match);
    } else {
        results.insert(
            Feature::FontInfo,
            DiffResult::Mismatch(format!(
                "fonts: rust={:?}, java={:?}",
                rust_fonts, java_fonts
            )),
        );
    }

    results
}

// ---------------------------------------------------------------------------
// Fixture corpus
// ---------------------------------------------------------------------------

/// Represents a corpus of test PDFs organized by tier.
#[derive(Debug)]
pub struct Corpus {
    pub smoke: Vec<PathBuf>,
    pub malformed: Vec<PathBuf>,
    pub font_heavy: Vec<PathBuf>,
    pub encrypted: Vec<PathBuf>,
    pub large: Vec<PathBuf>,
}

impl Corpus {
    /// Load corpus from a root directory.
    /// Expects subdirectories: smoke/, malformed/, font_heavy/, encrypted/, large/
    pub fn from_dir(root: &Path) -> std::io::Result<Self> {
        let mut corpus = Self {
            smoke: Vec::new(),
            malformed: Vec::new(),
            font_heavy: Vec::new(),
            encrypted: Vec::new(),
            large: Vec::new(),
        };

        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "pdf") {
                // Categorize by parent directory name
                if let Some(parent) = path.parent() {
                    if let Some(dir_name) = parent.file_name() {
                        match dir_name.to_string_lossy().as_ref() {
                            "smoke" => corpus.smoke.push(path),
                            "malformed" => corpus.malformed.push(path),
                            "font_heavy" => corpus.font_heavy.push(path),
                            "encrypted" => corpus.encrypted.push(path),
                            "large" => corpus.large.push(path),
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(corpus)
    }

    /// Returns all PDF files in the corpus.
    pub fn all_files(&self) -> Vec<&PathBuf> {
        self.smoke
            .iter()
            .chain(self.malformed.iter())
            .chain(self.font_heavy.iter())
            .chain(self.encrypted.iter())
            .chain(self.large.iter())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_output_creation() {
        let mut out = NormalizedOutput::new("test.pdf", "1.4".to_string(), 3);
        assert_eq!(out.page_count, 3);
        assert_eq!(out.page_sizes.len(), 0);

        out.add_page_size(0, 612.0, 792.0);
        assert_eq!(out.page_sizes.len(), 1);
    }

    #[test]
    fn normalized_output_text_truncation() {
        let mut out = NormalizedOutput::new("test.pdf", "1.4".to_string(), 1);
        let long_text = "a".repeat(1000);
        out.add_page_text(0, long_text);

        assert_eq!(out.page_texts[0].1.len(), 500);
    }

    #[test]
    fn feature_as_str() {
        assert_eq!(Feature::Structure.as_str(), "structure");
        assert_eq!(Feature::TextContent.as_str(), "text_content");
    }

    #[test]
    fn compare_outputs_matching() {
        let mut rust_out = NormalizedOutput::new("test.pdf", "1.4".to_string(), 2);
        rust_out.add_page_size(0, 612.0, 792.0);
        rust_out.add_page_size(1, 612.0, 792.0);
        rust_out.add_page_text(0, "Hello World".to_string());
        rust_out.add_page_text(1, "Second page".to_string());

        let mut java_out = NormalizedOutput::new("test.pdf", "1.4".to_string(), 2);
        java_out.add_page_size(0, 612.0, 792.0);
        java_out.add_page_size(1, 612.0, 792.0);
        java_out.add_page_text(0, "Hello World".to_string());
        java_out.add_page_text(1, "Second page".to_string());

        let results = compare_outputs(&rust_out, &java_out);
        assert_eq!(results[&Feature::Structure], DiffResult::Match);
        assert_eq!(results[&Feature::PageGeometry], DiffResult::Match);
        assert_eq!(results[&Feature::TextContent], DiffResult::Match);
    }

    #[test]
    fn compare_outputs_page_count_mismatch() {
        let rust_out = NormalizedOutput::new("test.pdf", "1.4".to_string(), 2);
        let java_out = NormalizedOutput::new("test.pdf", "1.4".to_string(), 3);

        let results = compare_outputs(&rust_out, &java_out);
        assert!(!matches!(results[&Feature::Structure], DiffResult::Match));
    }

    #[test]
    fn compat_report_track_results() {
        let mut report = CompatReport::new("test.pdf");
        report.add_result(Feature::Structure, DiffResult::Match);
        report.add_result(Feature::TextContent, DiffResult::Mismatch("text differs".to_string()));

        assert!(!report.passed);
        assert_eq!(report.results.len(), 2);
    }

    #[test]
    fn compat_report_summary() {
        let mut report = CompatReport::new("test.pdf");
        report.add_result(Feature::Structure, DiffResult::Match);
        let summary = report.to_string();
        assert!(summary.contains("PASS"));
    }
}

