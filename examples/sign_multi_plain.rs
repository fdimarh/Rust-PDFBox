//! Example: Multiple PAdES Signatures on a Plain (Non-Encrypted) PDF
//!
//! Sequential incremental signing on an unprotected PDF to verify
//! the multi-signature workflow works correctly before layering
//! encryption on top.
//!
//! # Usage
//! ```sh
//! cargo run --example sign_multi_plain
//! ```
//!
//! Output: signed_multi_plain.pdf

use rust_pdfbox::{
    signing::{sign_pdf, validate_pdf_full, PadesLevel, SignatureFormat, SignOptions},
    Document,
};
use std::{fs, path::PathBuf};

const INPUT: &str = "tests/signing_assets/sample.pdf";
const OUTPUT: &str = "signed_multi_plain.pdf";

fn asset(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("signing_assets");
    p.push(name);
    p
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║  rust-pdfbox · Multi-Signature (Plain / Unencrypted)        ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    let cert_chain = fs::read_to_string(asset("ca-chain.pem"))?;
    let private_key = fs::read_to_string(asset("user-key.pem"))?;
    let input_bytes = fs::read(INPUT)?;

    let signers = [
        ("Signature1", "Director Approval", "Jakarta"),
        ("Signature2", "Manager Approval", "Bandung"),
        ("Signature3", "Finance Approval", "Surabaya"),
    ];

    let mut current = input_bytes;

    for (i, (field_name, reason, location)) in signers.iter().enumerate() {
        println!("\n─── Signature #{}: {} ───", i + 1, field_name);

        let doc = Document::load_from_bytes(&current)?;
        println!("   Pages: {} ✅", doc.page_count());

        let opts = SignOptions {
            format: SignatureFormat::PAdES,
            pades_level: PadesLevel::B_LT,
            page: 1,
            rect: None,
            visible_signature: false,
            signer_name: reason.to_string(),
            contact_info: format!("{}@{}.com", field_name, reason.to_lowercase().replace(' ', "")),
            reason: reason.to_string(),
            location: location.to_string(),
            timestamp_url: Some("http://timestamp.digicert.com".into()),
            include_dss: true,
            include_crl: true,
            include_ocsp: false,
            field_name: field_name.to_string(),
            ..Default::default()
        };

        current = sign_pdf(&current, &cert_chain, &private_key, None, &opts)?;
        println!("   ✅ Signed ({} bytes)", current.len());

        // Validate all sigs so far
        let results = validate_pdf_full(&current, None)?;
        for r in &results {
            println!("   Sig #{} '{}': Digest {}  CMS {}  Chain {}",
                i + 1,
                r.field_name.as_deref().unwrap_or("?"),
                if r.digest_match { "✅" } else { "❌" },
                if r.cms_signature_valid { "✅" } else { "❌" },
                if r.certificate_chain_valid { "✅" } else { "❌" },
            );
        }
    }

    fs::write(OUTPUT, &current)?;

    // Final validation
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║  FINAL VALIDATION                                           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    let results = validate_pdf_full(&current, None)?;
    for (i, r) in results.iter().enumerate() {
        println!("\n--- Signature #{} ---", i + 1);
        println!("  Field Name       : {}", r.field_name.as_deref().unwrap_or("unnamed"));
        println!("  SubFilter        : {}", r.sub_filter.as_deref().unwrap_or("-"));
        println!("  Digest Match     : {}", if r.digest_match { "✅ VALID" } else { "❌ INVALID" });
        println!("  CMS Valid        : {}", if r.cms_signature_valid { "✅ VALID" } else { "❌ INVALID" });
        println!("  Cert Chain Valid : {}", if r.certificate_chain_valid { "✅ VALID" } else { "❌ INVALID" });
        println!("  Time Valid       : {}", if r.has_timestamp { "✅ VALID" } else { "❌ INVALID/MISSING" });
        println!("  LTV Enabled      : {}", if r.is_ltv_enabled { "✅ YES" } else { "❌ NO" });
        for w in &r.security_warnings {
            println!("  ⚠️  Warning: {}", w);
        }
    }

    let all_ok = results.iter().all(|r| r.digest_match && r.cms_signature_valid && r.certificate_chain_valid);
    println!("\n═══════════════════════════════════════════════════════════════");
    if all_ok {
        println!("🎉 ALL {} signatures VALID", results.len());
    } else {
        println!("⚠️  Some signatures failed — file: {}", OUTPUT);
    }
    println!("═══════════════════════════════════════════════════════════════");

    Ok(())
}
