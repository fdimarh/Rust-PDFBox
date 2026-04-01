//! Fixture generator — creates synthetic PDFs for comprehensive testing.
//!
//! This module provides utilities to generate PDFs with various characteristics
//! for testing text extraction, font handling, encryption, and edge cases.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Fixture generation
// ---------------------------------------------------------------------------

/// Parameters for generating a synthetic PDF.
#[derive(Debug, Clone)]
pub struct FixtureSpec {
    /// Output file path
    pub output: PathBuf,
    /// Number of pages
    pub pages: usize,
    /// Page width in points (default 612 for letter)
    pub page_width: f64,
    /// Page height in points (default 792 for letter)
    pub page_height: f64,
    /// Text to add per page (if Some, adds a page with text)
    pub text_content: Option<Vec<String>>,
    /// Font name to use (e.g., "Helvetica", "Times-Roman")
    pub font: String,
    /// Whether to encrypt with a password
    pub password: Option<String>,
    /// Whether to include multiple columns
    pub multi_column: bool,
    /// Add malformed objects (for robustness testing)
    pub with_corruption: bool,
}

impl Default for FixtureSpec {
    fn default() -> Self {
        Self {
            output: PathBuf::from("generated.pdf"),
            pages: 1,
            page_width: 612.0,
            page_height: 792.0,
            text_content: None,
            font: "Helvetica".to_string(),
            multi_column: false,
            password: None,
            with_corruption: false,
        }
    }
}

impl FixtureSpec {
    /// Creates a simple single-page fixture.
    pub fn simple() -> Self {
        Self::default()
    }

    /// Creates a multi-page fixture.
    pub fn multi_page(pages: usize) -> Self {
        Self {
            pages,
            ..Default::default()
        }
    }

    /// Creates a text-heavy fixture.
    pub fn text_heavy() -> Self {
        let text = vec![
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit.".to_string();
            10
        ];
        Self {
            text_content: Some(text),
            ..Default::default()
        }
    }

    /// Creates an encrypted fixture.
    pub fn encrypted(password: &str) -> Self {
        Self {
            password: Some(password.to_string()),
            ..Default::default()
        }
    }

    /// Creates a multi-column fixture.
    pub fn multi_column() -> Self {
        Self {
            multi_column: true,
            text_content: Some(vec!["Left column text".to_string(), "Right column text".to_string()]),
            ..Default::default()
        }
    }

    /// Creates a fixture with intentional corruption (for robustness).
    pub fn corrupted() -> Self {
        Self {
            with_corruption: true,
            ..Default::default()
        }
    }

    pub fn output_path(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.output = path.into();
        self
    }

    pub fn pages(&mut self, count: usize) -> &mut Self {
        self.pages = count;
        self
    }

    pub fn text(&mut self, lines: Vec<String>) -> &mut Self {
        self.text_content = Some(lines);
        self
    }

    pub fn font(&mut self, name: &str) -> &mut Self {
        self.font = name.to_string();
        self
    }

    pub fn password(&mut self, pwd: &str) -> &mut Self {
        self.password = Some(pwd.to_string());
        self
    }

    pub fn set_multi_column(&mut self, enabled: bool) -> &mut Self {
        self.multi_column = enabled;
        self
    }
}

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

/// Generates synthetic PDF fixtures for testing.
///
/// This uses rust-pdfbox itself to generate test PDFs, ensuring the generator
/// and library stay in sync. Real PDF generation is post-v1.
pub struct FixtureGenerator;

impl FixtureGenerator {
    /// Generate a PDF according to the spec.
    ///
    /// Returns the path to the generated file.
    /// Note: Full PDF generation is post-v1; this is a placeholder that
    /// documents the intended API.
    pub fn generate(spec: &FixtureSpec) -> std::io::Result<PathBuf> {
        // TODO: Implement once writer supports stream object creation
        // For now, this is a placeholder that documents the API.

        // In the meantime, fixtures are stored in tests/fixtures/
        // and used directly by tests.

        let expected_path = &spec.output;

        if !expected_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Fixture not found: {}. Fixtures must be created manually or via Java PDFBox.",
                    expected_path.display()
                ),
            ));
        }

        Ok(expected_path.clone())
    }

    /// Verify a fixture can be loaded with rust-pdfbox.
    pub fn verify_fixture(path: &Path) -> std::io::Result<()> {
        // TODO: Use Document::load_from_bytes once fully implemented
        if !path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Fixture not found: {}", path.display()),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Corpus metadata
// ---------------------------------------------------------------------------

/// Metadata about a fixture.
#[derive(Debug, Clone)]
pub struct FixtureMetadata {
    /// Relative path from tests/fixtures/
    pub relative_path: PathBuf,
    /// Tier: smoke, malformed, font_heavy, encrypted, large
    pub tier: String,
    /// Expected page count (for validation)
    pub expected_pages: Option<usize>,
    /// Known issues (if any)
    pub known_issues: Vec<String>,
}

/// Returns metadata for all known fixtures.
pub fn all_fixtures() -> Vec<FixtureMetadata> {
    vec![
        // Smoke tests — valid, simple PDFs
        FixtureMetadata {
            relative_path: PathBuf::from("smoke/minimal.pdf"),
            tier: "smoke".to_string(),
            expected_pages: Some(1),
            known_issues: vec![],
        },
        FixtureMetadata {
            relative_path: PathBuf::from("smoke/a4_letter.pdf"),
            tier: "smoke".to_string(),
            expected_pages: Some(1),
            known_issues: vec![],
        },
        // Malformed tests — should not crash
        FixtureMetadata {
            relative_path: PathBuf::from("malformed/truncated.pdf"),
            tier: "malformed".to_string(),
            expected_pages: None,
            known_issues: vec!["Truncated at xref".to_string()],
        },
        FixtureMetadata {
            relative_path: PathBuf::from("malformed/missing_header.pdf"),
            tier: "malformed".to_string(),
            expected_pages: None,
            known_issues: vec!["Missing %PDF header".to_string()],
        },
        // Font-heavy
        FixtureMetadata {
            relative_path: PathBuf::from("font_heavy/multi_font.pdf"),
            tier: "font_heavy".to_string(),
            expected_pages: Some(1),
            known_issues: vec![],
        },
        // Encrypted
        FixtureMetadata {
            relative_path: PathBuf::from("encrypted/user_password.pdf"),
            tier: "encrypted".to_string(),
            expected_pages: Some(1),
            known_issues: vec!["RC4-128 encrypted".to_string()],
        },
        // Large
        FixtureMetadata {
            relative_path: PathBuf::from("large/100_pages.pdf"),
            tier: "large".to_string(),
            expected_pages: Some(100),
            known_issues: vec![],
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_spec_defaults() {
        let spec = FixtureSpec::default();
        assert_eq!(spec.pages, 1);
        assert_eq!(spec.font, "Helvetica");
        assert!(!spec.multi_column);
    }

    #[test]
    fn fixture_spec_builder() {
        let spec = FixtureSpec::simple();
        // Test builder pattern
        assert_eq!(spec.pages, 1);
    }

    #[test]
    fn fixture_spec_multi_page() {
        let spec = FixtureSpec::multi_page(10);
        assert_eq!(spec.pages, 10);
    }

    #[test]
    fn fixture_spec_encrypted() {
        let spec = FixtureSpec::encrypted("mypassword");
        assert_eq!(spec.password, Some("mypassword".to_string()));
    }

    #[test]
    fn fixture_metadata_list() {
        let fixtures = all_fixtures();
        assert!(!fixtures.is_empty());

        let smoke_count = fixtures.iter().filter(|f| f.tier == "smoke").count();
        assert!(smoke_count > 0);
    }

    #[test]
    fn fixture_spec_text_heavy() {
        let spec = FixtureSpec::text_heavy();
        assert!(spec.text_content.is_some());
        assert_eq!(spec.text_content.as_ref().unwrap().len(), 10);
    }
}


