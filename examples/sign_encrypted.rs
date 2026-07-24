//! Example: Signing an Already-Encrypted (Password-Protected) PDF
//!
//! This example demonstrates the complete workflow:
//! 1. Load a password-protected PDF
//! 2. Unlock it in memory with the user password
//! 3. Apply a PAdES-B-LT digital signature via Incremental Update
//! 4. Verify the result is both decryptable and cryptographically valid
//!
//! # Prerequisites
//! You need an encrypted PDF file named `encrypted_input.pdf` in the current directory.
//! Use external tools (like Adobe Acrobat, qpdf, or pdftk) to encrypt `sample.pdf` first,
//! or add a `--generate` feature to this example in the future.
//!
//! Expected password for the encrypted PDF: "admin123"
//!
//! # Usage
//! ```sh
//! cargo run --example sign_encrypted
//! ```

use rust_pdfbox::{
    signing::{sign_pdf, validate_pdf_full, PadesLevel, SignatureFormat, SignOptions},
    Document,
};
use std::{fs, path::PathBuf, process};

const USER_PASSWORD: &str = "admin123";
const ENCRYPTED_INPUT: &str = "encrypted_input.pdf";
const SIGNED_OUTPUT: &str = "signed_encrypted_output.pdf";

fn asset(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("signing_assets");
    p.push(name);
    p
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║  rust-pdfbox · Encrypted PDF + PAdES Digital Signature Demo  ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    // ── Step 1: Load encrypted PDF ───────────────────────────────────
    println!("\n📄 Step 1: Loading encrypted PDF...");
    println!("   File: {}", ENCRYPTED_INPUT);
    
    let encrypted_bytes = fs::read(ENCRYPTED_INPUT).unwrap_or_else(|_| {
        eprintln!("❌ Encrypted file not found at: {}", ENCRYPTED_INPUT);
        eprintln!("   Please provide an encrypted PDF named '{}'", ENCRYPTED_INPUT);
        eprintln!("   (User password expected: {})", USER_PASSWORD);
        process::exit(1);
    });

    let mut doc = Document::load_from_bytes(&encrypted_bytes)?;
    println!("   ✅ PDF loaded ({} pages)", doc.page_count());

    // ── Step 2: Unlock with password ─────────────────────────────────
    println!("\n🔓 Step 2: Decrypting with user password...");
    doc.decrypt(USER_PASSWORD)?;
    println!("   ✅ Document decrypted in memory");

    // ── Step 3: Load signing credentials ─────────────────────────────
    println!("\n🔐 Step 3: Loading signing credentials...");
    let cert_chain = fs::read_to_string(asset("ca-chain.pem"))?;
    let private_key = fs::read_to_string(asset("user-key.pem"))?;
    
    let cert_count = cert_chain.matches("-----BEGIN CERTIFICATE-----").count();
    println!("   Certs: {} certificate(s)", cert_count);
    println!("   Key:   {}", asset("user-key.pem").display());

    // ── Step 4: Sign the unlocked document ───────────────────────────
    println!("\n✍️  Step 4: Applying PAdES-B-LT signature...");
    println!("   Output: {}", SIGNED_OUTPUT);

    let sign_opts = SignOptions {
        format: SignatureFormat::PAdES,
        pades_level: PadesLevel::B_LT,
        page: 1,
        rect: None, // Invisible signature
        visible_signature: false,
        signer_name: "Admin User".into(),
        contact_info: "admin@example.com".into(),
        reason: "Document approval".into(),
        location: "Jakarta, Indonesia".into(),
        timestamp_url: Some("http://timestamp.digicert.com".into()),
        include_dss: true,
        include_crl: true,
        include_ocsp: false,
        field_name: "Signature1".into(),
        ..Default::default()
    };

    let signed_bytes = sign_pdf(
        &encrypted_bytes,
        &cert_chain,
        &private_key,
        Some(USER_PASSWORD), // ← CRITICAL: Unlock the encrypted PDF before signing
        &sign_opts,
    )?;

    fs::write(SIGNED_OUTPUT, &signed_bytes)?;
    println!("   ✅ Signed & encrypted PDF written ({} bytes)", signed_bytes.len());

    // ── Step 5: Validate result ──────────────────────────────────────
    println!("\n🔬 Step 5: Verifying the signed + encrypted PDF...");
    
    // Verify it opens with password
    let mut verify_doc = Document::load_from_bytes(&signed_bytes)?;
    verify_doc.decrypt(USER_PASSWORD)?;
    println!("   ✅ Final PDF opens correctly with password");

    // Full cryptographic validation
    let results = validate_pdf_full(&signed_bytes)?;
    
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║  VALIDATION RESULTS                                           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    
    for (i, r) in results.iter().enumerate() {
        println!("\n--- Signature #{} ---", i + 1);
        println!("  Field Name       : {}", r.field_name.as_deref().unwrap_or("unnamed"));
        println!("  SubFilter        : {}", r.sub_filter.as_deref().unwrap_or("-"));
        println!("  Digest Match     : {}", if r.digest_match { "✅ VALID" } else { "❌ INVALID" });
        println!("  CMS Valid        : {}", if r.cms_signature_valid { "✅ VALID" } else { "❌ INVALID" });
        println!("  Cert Chain Valid : {}", if r.certificate_chain_valid { "✅ VALID" } else { "❌ INVALID" });
        println!("  Time Valid       : {}", if r.has_timestamp { "✅ VALID" } else { "❌ INVALID/MISSING" });
        println!("  LTV Enabled      : {}", if r.is_ltv_enabled { "✅ YES" } else { "❌ NO" });
        
        if !r.errors.is_empty() {
            for e in &r.errors { println!("  ❌ Error: {}", e); }
        }
        for w in &r.security_warnings {
            println!("  ⚠️  Warning: {}", w);
        }
    }

    let all_ok = results
        .iter()
        .all(|r| r.digest_match && r.cms_signature_valid && r.certificate_chain_valid);
    
    println!("\n═══════════════════════════════════════════════════════════════");
    if all_ok {
        println!("🎉 SUCCESS: Encrypted PDF signed and validated correctly!");
        println!("   File: {}", SIGNED_OUTPUT);
        println!("   Open with Adobe/Foxit using password: {}", USER_PASSWORD);
    } else {
        println!("💥 FAILURE: One or more signatures failed validation");
        process::exit(1);
    }
    println!("═══════════════════════════════════════════════════════════════\n");

    Ok(())
}