//! Example: PDF Digital Signature using rust-pdfbox
//!
//! Demonstrates how to sign and verify a PDF document using the
//! `rust_pdfbox::signing` module — no external PDF library dependency.
//!
//! # Required assets (from tests/signing_assets/)
//!
//! | File                        | Purpose                              |
//! |-----------------------------|--------------------------------------|
//! | `sample.pdf`                | Input PDF to sign                    |
//! | `keystore-local-chain.pem`  | Certificate chain (signer cert first)|
//! | `keystore-local-key.pem`    | PKCS#8 PEM private key               |
//! | `sig1.png`                  | (optional) Visible signature image   |
//!
//! # Usage
//!
//! ```sh
//! # Invisible signature (default)
//! cargo run --example digital_sign
//!
//! # Visible signature with custom rectangle
//! cargo run --example digital_sign -- --rect 50,700,250,750 --reason "I approve"
//!
//! # Custom key / cert paths
//! cargo run --example digital_sign -- \
//!     --input  path/to/input.pdf \
//!     --cert   path/to/chain.pem \
//!     --key    path/to/key.pem   \
//!     --output signed.pdf
//! ```

use rust_pdfbox::signing::{sign_pdf, verify_pdf, SignOptions};
use std::{env, fs, path::PathBuf, process};

// ---------------------------------------------------------------------------
// Helper: resolve an asset path relative to the project root
// ---------------------------------------------------------------------------

fn asset(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("signing_assets");
    p.push(name);
    p
}


// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

struct Args {
    input:   PathBuf,
    output:  PathBuf,
    cert:    PathBuf,
    key:     PathBuf,
    rect:    Option<[f64; 4]>,
    reason:  String,
    contact: String,
    page:    u32,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            input:   asset("sample.pdf"),
            output:  PathBuf::from("signed_output.pdf"),
            cert:    asset("ca-chain.pem"),
            key:     asset("user-key.pem"),
            rect:    None, // invisible by default
            reason:  "Approved via rust-pdfbox digital signature".into(),
            contact: "signer@example.com".into(),
            page:    1,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let cli: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < cli.len() {
        match cli[i].as_str() {
            "--input"   | "-i" => { i += 1; a.input  = PathBuf::from(&cli[i]); }
            "--output"  | "-o" => { i += 1; a.output = PathBuf::from(&cli[i]); }
            "--cert"    | "-c" => { i += 1; a.cert   = PathBuf::from(&cli[i]); }
            "--key"     | "-k" => { i += 1; a.key    = PathBuf::from(&cli[i]); }
            "--reason"  | "-r" => { i += 1; a.reason  = cli[i].clone(); }
            "--contact"        => { i += 1; a.contact = cli[i].clone(); }
            "--page"    | "-p" => { i += 1; a.page = cli[i].parse().unwrap_or(1); }
            "--rect" => {
                i += 1;
                let parts: Vec<f64> = cli[i].split(',')
                    .map(|s| s.trim().parse().unwrap_or(0.0)).collect();
                if parts.len() == 4 {
                    a.rect = Some([parts[0], parts[1], parts[2], parts[3]]);
                }
            }
            "--help" | "-h" => {
                println!("Usage: digital_sign [--input PDF] [--output PDF] [--cert PEM] [--key PEM]");
                println!("                    [--rect x1,y1,x2,y2] [--reason TEXT] [--page N]");
                process::exit(0);
            }
            other => { eprintln!("Unknown argument: {other}"); process::exit(1); }
        }
        i += 1;
    }
    a
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args = parse_args();

    println!("══════════════════════════════════════════════════════");
    println!("  rust-pdfbox  ·  Digital Signature Example");
    println!("══════════════════════════════════════════════════════");

    // ── 1. Load input PDF ─────────────────────────────────────────────────
    let pdf_bytes = fs::read(&args.input).unwrap_or_else(|e| {
        eprintln!("Cannot read input PDF {:?}: {e}", args.input);
        process::exit(1);
    });
    println!("  Input  : {:?}  ({} bytes)", args.input, pdf_bytes.len());

    // ── 2. Parse the document info ────────────────────────────────────────
    let doc = rust_pdfbox::Document::load_from_bytes(&pdf_bytes).unwrap_or_else(|e| {
        eprintln!("Failed to parse PDF: {e}");
        process::exit(1);
    });
    println!("  Pages  : {}", doc.page_count());
    println!("  Version: {} bytes source", doc.source_len());

    // ── 3. Load certificate chain ─────────────────────────────────────────
    let cert_chain_pem = fs::read_to_string(&args.cert).unwrap_or_else(|e| {
        eprintln!("Cannot read cert {:?}: {e}", args.cert);
        process::exit(1);
    });
    // Quick count of certs for display
    let cert_count = cert_chain_pem.matches("-----BEGIN CERTIFICATE-----").count();
    if cert_count == 0 {
        eprintln!("No certificates found in {:?}", args.cert);
        process::exit(1);
    }
    println!("  Certs  : {} certificate(s) loaded", cert_count);

    // ── 4. Load private key ───────────────────────────────────────────────
    let key_pem = fs::read_to_string(&args.key).unwrap_or_else(|e| {
        eprintln!("Cannot read key {:?}: {e}", args.key);
        process::exit(1);
    });
    println!("  Key    : {:?}", args.key);

    // ── 5. Build signature options ────────────────────────────────────────
    let opts = SignOptions {
        page:         args.page,
        rect:         args.rect,
        reason:       args.reason.clone(),
        contact_info: args.contact.clone(),
        location:     "rust-pdfbox".into(),
        reserved_size: 16_384,
        field_name:   "Signature1".into(),
        ..Default::default()
    };

    println!("  Reason : {}", opts.reason);
    println!("  Page   : {}", opts.page);
    match opts.rect {
        Some(r) => println!("  Rect   : [{} {} {} {}] (visible)", r[0], r[1], r[2], r[3]),
        None    => println!("  Rect   : invisible (no annotation)"),
    }
    println!();

    // ── 6. Sign ───────────────────────────────────────────────────────────
    println!("  Signing …");
    let signed = sign_pdf(&pdf_bytes, &cert_chain_pem, &key_pem, &opts)
        .unwrap_or_else(|e| {
            eprintln!("Signing failed: {e}");
            process::exit(1);
        });
    println!("  ✅ Signed PDF: {} bytes", signed.len());

    // ── 7. Write output ───────────────────────────────────────────────────
    fs::write(&args.output, &signed).unwrap_or_else(|e| {
        eprintln!("Cannot write {:?}: {e}", args.output);
        process::exit(1);
    });
    println!("  Output : {:?}", args.output);
    println!();

    // ── 8. Verify the freshly-signed PDF ─────────────────────────────────
    println!("  Verifying …");
    match verify_pdf(&signed) {
        Ok(results) if results.is_empty() => {
            println!("  ⚠  No signature fields found in output PDF.");
        }
        Ok(results) => {
            for (i, r) in results.iter().enumerate() {
                let valid_icon = if r.is_valid() { "✅" } else { "❌" };
                println!("  Signature [{}] {}: field='{}'", i + 1, valid_icon, r.field_name);
                println!("    Status         : {}", r.status);
                println!("    Filter         : {}", r.filter.as_deref().unwrap_or("-"));
                println!("    SubFilter      : {}", r.sub_filter.as_deref().unwrap_or("-"));
                println!("    Reason         : {}", r.reason.as_deref().unwrap_or("-"));
                println!("    Contact        : {}", r.contact_info.as_deref().unwrap_or("-"));
                println!("    Signing time   : {}", r.signing_time.as_deref().unwrap_or("-"));
                println!("    ByteRange      : {:?}", r.byte_range);
                println!("    Covers file    : {}", r.byte_range_covers_whole_file);
                println!("    Digest valid   : {}", r.digest_valid);
                println!("    CMS sig valid  : {}", r.cms_signature_valid);
                println!("    Chain valid    : {}", r.certificate_chain_valid);
                println!("    Has timestamp  : {}", r.has_timestamp);
                println!("    Has DSS        : {}", r.has_dss);
                println!("    LTV enabled    : {}", r.is_ltv_enabled);
                for warn in &r.chain_warnings {
                    println!("    ⚠  Chain warn  : {warn}");
                }
                println!("    Certificates   : {}", r.certificates.len());
                for (ci, cert) in r.certificates.iter().enumerate() {
                    println!("      [{}] Subject  : {}", ci, cert.subject);
                    println!("          Issuer   : {}", cert.issuer);
                    println!("          Serial   : {}", cert.serial);
                    println!("          Valid    : {} → {}",
                        cert.not_before.as_deref().unwrap_or("?"),
                        cert.not_after.as_deref().unwrap_or("?"));
                    if cert.is_self_signed { println!("          ⚠  Self-signed"); }
                    if cert.is_expired     { println!("          ❌ Expired"); }
                }
                for err in &r.errors {
                    println!("    ❌ Error       : {err}");
                }
            }
            let all_ok = results.iter().all(|r| r.digest_valid && r.cms_signature_valid);
            println!();
            if all_ok {
                println!("  ✅ All signatures cryptographically verified.");
            } else {
                println!("  ❌ One or more signatures failed verification.");
                process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("  Verification error: {e}");
            process::exit(2);
        }
    }
    println!("══════════════════════════════════════════════════════");
}

