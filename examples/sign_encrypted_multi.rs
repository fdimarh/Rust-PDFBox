//! Example: Multiple PAdES Signatures on an Encrypted PDF
//!
//! Demonstrates sequential incremental signing: each signature is applied
//! as a separate incremental update to the already-signed PDF.
//!
//! # Usage
//! ```sh
//! cargo run --example sign_encrypted_multi
//! ```
//!
//! Output: signed_encrypted_multi.pdf  (open with password: admin123)

use rust_pdfbox::{
    Document,
    signing::{PadesLevel, SignOptions, SignatureFormat, sign_pdf, validate_pdf_full},
};
use std::{fs, path::PathBuf, process};

const USER_PASSWORD: &str = "admin123";
const ENCRYPTED_INPUT: &str = "encrypted_input.pdf";
const OUTPUT: &str = "signed_encrypted_multi.pdf";

fn asset(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("signing_assets");
    p.push(name);
    p
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║  rust-pdfbox · Multi-Signature on Encrypted PDF             ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    let cert_chain = fs::read_to_string(asset("ca-chain.pem"))?;
    let private_key = fs::read_to_string(asset("user-key.pem"))?;
    let encrypted_bytes = fs::read(ENCRYPTED_INPUT).unwrap_or_else(|_| {
        eprintln!("❌ Encrypted file not found at: {}", ENCRYPTED_INPUT);
        process::exit(1);
    });

    let fields = [
        ("Signature1", "Director Approval", "Jakarta"),
        ("Signature2", "Manager Approval", "Bandung"),
    ];

    let mut current_bytes = encrypted_bytes;
    let mut all_results = Vec::new();

    for (i, (field_name, reason, location)) in fields.iter().enumerate() {
        println!("\n─── Signature #{}: {} ───", i + 1, field_name);

        // Check page count & decryption
        let mut check_doc = Document::load_from_bytes(&current_bytes)?;
        check_doc.decrypt(USER_PASSWORD)?;
        println!("   Pages: {} ✅ decrypt OK", check_doc.page_count());

        let sign_opts = SignOptions {
            format: SignatureFormat::PAdES,
            pades_level: PadesLevel::B_LT,
            page: 1,
            rect: None,
            visible_signature: false,
            signer_name: reason.to_string(),
            contact_info: format!("{}@company.com", field_name),
            reason: reason.to_string(),
            location: location.to_string(),
            timestamp_url: Some("http://timestamp.digicert.com".into()),
            include_dss: true,
            include_crl: true,
            include_ocsp: false,
            field_name: field_name.to_string(),
            ..Default::default()
        };

        current_bytes = sign_pdf(
            &current_bytes,
            &cert_chain,
            &private_key,
            Some(USER_PASSWORD),
            &sign_opts,
        )?;
        println!("   ✅ Signed ({} bytes)", current_bytes.len());

        // Validate
        let results = validate_pdf_full(&current_bytes, Some("admin123"))?;
        all_results.push(results);
    }

    fs::write(OUTPUT, &current_bytes)?;

    // Summary
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║  FINAL VALIDATION                                           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    let results = validate_pdf_full(&current_bytes, Some("admin123"))?;
    for (i, r) in results.iter().enumerate() {
        println!("\n--- Signature #{} ---", i + 1);
        println!(
            "  Field Name       : {}",
            r.field_name.as_deref().unwrap_or("unnamed")
        );
        println!(
            "  SubFilter        : {}",
            r.sub_filter.as_deref().unwrap_or("-")
        );
        println!(
            "  Digest Match     : {}",
            if r.digest_match {
                "✅ VALID"
            } else {
                "❌ INVALID"
            }
        );
        println!(
            "  CMS Valid        : {}",
            if r.cms_signature_valid {
                "✅ VALID"
            } else {
                "❌ INVALID"
            }
        );
        println!(
            "  Cert Chain Valid : {}",
            if r.certificate_chain_valid {
                "✅ VALID"
            } else {
                "❌ INVALID"
            }
        );
        println!(
            "  Time Valid       : {}",
            if r.has_timestamp {
                "✅ VALID"
            } else {
                "❌ INVALID/MISSING"
            }
        );
        println!(
            "  LTV Enabled      : {}",
            if r.is_ltv_enabled {
                "✅ YES"
            } else {
                "❌ NO"
            }
        );
        for w in &r.security_warnings {
            println!("  ⚠️  Warning: {}", w);
        }
    }

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("🎉 File: {}  (password: {})", OUTPUT, USER_PASSWORD);
    println!("═══════════════════════════════════════════════════════════════\n");

    Ok(())
}
