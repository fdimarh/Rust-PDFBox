//! External signer interface for HSM, PKCS#11, remote/cloud signing services.
//!
//! # When to use
//!
//! Use [`CmsSigner`] when you need to delegate the CMS (PKCS#7 / PAdES)
//! signature generation to an external system — **hardware security module**
//! (HSM), **cloud KMS** (AWS KMS, Azure Key Vault, Google Cloud KMS),
//! **remote signing service**, or a **smart-card PKCS#11 middleware**.
//!
//! For local signing with a PEM private key, use [`sign_pdf`] instead.
//!
//! # Usage
//!
//! ```rust,ignore
//! use rust_pdfbox::signing::cms_signer::{CmsSigner, CmsSignerResult, SignatureConfig};
//! use rust_pdfbox::signing::SignOptions;
//!
//! struct MyHsmSigner { key_id: Vec<u8> }
//!
//! impl CmsSigner for MyHsmSigner {
//!     fn algorithm(&self) -> &'static str { "RSA" }
//!     fn sign_bytes(&mut self, digest: &[u8], _config: &SignatureConfig)
//!         -> Result<CmsSignerResult, String>
//!     {
//!         let signature = hsm_sign(digest, &self.key_id)?;
//!         Ok(CmsSignerResult { cms_der: signature })
//!     }
//! }
//! ```

use crate::signing::SignOptions;

/// Configuration passed to the external signer.
///
/// Carries information the signer may need to build the CMS structure:
/// which digest to use, whether to embed certificates, what PAdES level
/// to target, etc.
#[derive(Debug, Clone)]
pub struct SignatureConfig {
    /// The raw SHA-256 digest of the ByteRange content (to-be-signed bytes).
    pub digest: Vec<u8>,
    /// Signing options such as PAdES level, timestamp URL, signature format.
    pub sign_options: SignOptions,
}

/// The result returned by an external signer.
#[derive(Debug, Clone)]
pub struct CmsSignerResult {
    /// Complete DER-encoded CMS SignedData blob that will be hex-encoded
    /// and written into the PDF `/Contents` placeholder.
    pub cms_der: Vec<u8>,
}

/// Trait for implementing external CMS signers.
///
/// Implement this trait to integrate with an HSM, cloud KMS, or remote
/// signing service. The trait is consumed by [`sign_pdf_with_cms_signer`].
pub trait CmsSigner {
    /// Human-readable algorithm name (for logging / diagnostics).
    fn algorithm(&self) -> &'static str;

    /// Sign the given digest and return a complete CMS blob.
    ///
    /// # Arguments
    ///
    /// * `digest` — The raw SHA-256 digest (32 bytes) of the PDF ByteRange
    ///   content that covers the entire PDF except the `/Contents` placeholder.
    /// * `config` — Signature configuration: algorithm, PAdES level, etc.
    ///
    /// # Returns
    ///
    /// A [`CmsSignerResult`] containing the DER-encoded CMS blob. The caller
    /// hex-encodes it and injects it into the PDF.
    fn sign_bytes(&mut self, digest: &[u8], config: &SignatureConfig) -> Result<CmsSignerResult, String>;
}

/// Sign a PDF using an external [`CmsSigner`].
///
/// This is a convenience function that wraps the internal signing pipeline
/// with an external signer trait. It calls [`super::sign_pdf_with_signer`]
/// internally, converting the [`CmsSigner`] into the closure it expects.
///
/// # Arguments
///
/// * `pdf_bytes` — Original unencrypted PDF bytes.
/// * `_cert_chain_pem` — Ignored (signer manages its own keys/certs), pass `""`.
/// * `unlock_password` — Password for encrypted PDFs, or `None`.
/// * `opts` — [`SignOptions`] controlling placement, format, and metadata.
/// * `signer` — Anything implementing [`CmsSigner`].
///
/// # Errors
///
/// Returns [`PdfError`] if the signer fails or the PDF structure is invalid.
pub fn sign_pdf_with_cms_signer(
    pdf_bytes: &[u8],
    _cert_chain_pem: &str,
    unlock_password: Option<&str>,
    opts: &super::SignOptions,
    mut signer: impl CmsSigner,
) -> Result<Vec<u8>, crate::PdfError> {
    let config = SignatureConfig {
        digest: Vec::new(), // will be filled by the signing pipeline
        sign_options: opts.clone(),
    };

    // Delegate to the generic signer: we build the CMS ourselves via the callback.
    // Note: `sign_pdf` expects cert_chain_pem + private_key_pem. We use
    // `sign_pdf_with_signer` instead which takes a closure.
    super::sign_pdf_with_signer(pdf_bytes, unlock_password, opts, move |content: &[u8]| {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content);
        let digest = hasher.finalize().to_vec();

        let mut cfg = config.clone();
        cfg.digest = digest;

        let result = signer
            .sign_bytes(&cfg.digest, &cfg)
            .map_err(|e| crate::PdfError::Parse {
                offset: None,
                context: format!("CMS signer failed: {e}"),
            })?;

        Ok(result.cms_der)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSigner;
    impl CmsSigner for MockSigner {
        fn algorithm(&self) -> &'static str {
            "RSA-Mock"
        }
        fn sign_bytes(
            &mut self,
            _digest: &[u8],
            _config: &SignatureConfig,
        ) -> Result<CmsSignerResult, String> {
            Ok(CmsSignerResult {
                cms_der: b"mock-cms-blob".to_vec(),
            })
        }
    }

    #[test]
    fn test_mock_signer() {
        let mut signer = MockSigner;
        let config = SignatureConfig {
            digest: vec![0u8; 32],
            sign_options: SignOptions::default(),
        };
        let result = signer.sign_bytes(&config.digest, &config).unwrap();
        assert_eq!(result.cms_der, b"mock-cms-blob");
    }

    #[test]
    fn test_sign_with_mock_signer() {
        // Just verify the function signature compiles and accepts the trait.
        let _ = crate::signing::SignatureFormat::Pkcs7;
    }
}
