//! Example: PDF Digital Signature using rust-pdfbox
//!
//! Full feature parity with `rust_pdf_signing`'s `sign_doc` example.
//!
//! # Required assets (tests/signing_assets/)
//!
//! | File                   | Purpose                               |
//! |------------------------|---------------------------------------|
//! | `sample.pdf`           | Default input PDF                     |
//! | `ca-chain.pem`         | Certificate chain (signer cert first) |
//! | `user-key.pem`         | PKCS#8 PEM private key                |
//! | `sig1.png`             | (optional) Visible signature image    |
//!
//! # Usage
//!
//! ```sh
//! cargo run --example digital_sign
//!
//! # PAdES B-T (signature timestamp)
//! cargo run --example digital_sign -- -f pades -l b-t
//!
//! # PAdES B-LT (timestamp + DSS)
//! cargo run --example digital_sign -- -f pades -l b-lt
//!
//! # Visible signature with rectangle
//! cargo run --example digital_sign -- --rect 50,700,250,750 --reason "I approve"
//!
//! # Invisible PKCS7 with custom cert/key
//! cargo run --example digital_sign -- -f pkcs7 --invisible -c chain.pem -k key.pem
//! ```

use rust_pdfbox::signing::{
    sign_pdf, validate_pdf_full, PadesLevel, SignatureAnchorMode, SignatureFormat, SignOptions,
};
use std::{env, fs, path::PathBuf, process};

// ---------------------------------------------------------------------------
// Asset helper
// ---------------------------------------------------------------------------

fn asset(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("signing_assets");
    p.push(name);
    p
}

// ---------------------------------------------------------------------------
// CLI usage
// ---------------------------------------------------------------------------

fn usage() {
    eprintln!(
        "Usage: digital_sign [input.pdf] [options]

Options:
  -o, --output <path>           Output file path           (default: signed_output.pdf)
  -c, --cert <path>             Certificate chain PEM      (default: tests/signing_assets/ca-chain.pem)
  -k, --key <path>              Private key PEM            (default: tests/signing_assets/user-key.pem)
  -f, --format <pkcs7|pades>    Signature format           (default: pkcs7)
  -l, --level <b-b|b-t|b-lt|b-lta>
                                PAdES conformance level    (default: b-b, only for pades)
  -p, --page <num>              Page number (1-based)      (default: 1)
  -r, --rect <x1,y1,x2,y2>     Signature rectangle        (default: invisible)
      --invisible               Force invisible signature
      --tag <text>              Anchor signature to text tag on page
      --width <num>             Signature width  (required with --tag)
      --height <num>            Signature height (required with --tag)
      --tag-mode <front|overlay> Tag placement mode        (default: front)
      --name <name>             Signer display name
      --contact <email>         Signer contact / email
      --reason <text>           Signing reason             (default: Digital Signature)
      --location <text>         Signing location
      --tsa <url>               RFC 3161 TSA URL           (default: http://timestamp.digicert.com)
      --no-tsa                  Disable timestamp (for B-B or offline signing)
      --dss                     Append DSS dictionary (CRL/OCSP/Certs)
      --crl                     Include CRL in CMS signed attributes
      --no-crl                  Exclude CRL from CMS signed attributes
      --ocsp                    Include OCSP in CMS signed attributes
      --reserved <bytes>        Reserve N bytes for CMS blob  (default: 32768)
      --field <name>            AcroForm field name        (default: Signature1)
  -h, --help                    Show this help

PAdES Levels:
  b-b    Basic — ESS-signingCertV2 only, no timestamp
  b-t    Timestamp — adds RFC 3161 signature timestamp (TSA required)
  b-lt   Long-Term — timestamp + DSS dictionary with CRL/OCSP/Certs
  b-lta  Archival — B-LT + document-level timestamp

Examples:
  digital_sign
  digital_sign input.pdf -f pades -l b-lt
  digital_sign input.pdf --rect 50,700,250,750 --reason \"Approved\"
  digital_sign input.pdf --tag \"#SIGN\" --width 180 --height 64 --tag-mode front
  digital_sign input.pdf -f pades -l b-lta --invisible
  digital_sign input.pdf -f pkcs7 --dss --crl --ocsp"
    );
}

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

struct Args {
    input:         PathBuf,
    output:        PathBuf,
    cert:          PathBuf,
    key:           PathBuf,
    format:        SignatureFormat,
    pades_level:   PadesLevel,
    page:          u32,
    rect:          Option<[f64; 4]>,
    visible:       bool,
    anchor_tag:    Option<String>,
    anchor_width:  Option<f64>,
    anchor_height: Option<f64>,
    anchor_mode:   SignatureAnchorMode,
    signer_name:   String,
    contact:       String,
    reason:        String,
    location:      String,
    tsa_url:       Option<String>,
    include_dss:   bool,
    include_crl:   Option<bool>,  // None = use default per format
    include_ocsp:  bool,
    reserved_size: usize,
    field_name:    String,
    image_path:    Option<PathBuf>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            input:         asset("sample.pdf"),
            output:        PathBuf::from("signed_output.pdf"),
            cert:          asset("ca-chain.pem"),
            key:           asset("user-key.pem"),
            format:        SignatureFormat::Pkcs7,
            pades_level:   PadesLevel::B_B,
            page:          1,
            rect:          None,
            visible:       false,  // invisible by default (no rect)
            anchor_tag:    None,
            anchor_width:  None,
            anchor_height: None,
            anchor_mode:   SignatureAnchorMode::InFront,
            signer_name:   String::new(),
            contact:       "signer@example.com".into(),
            reason:        "Approved via rust-pdfbox digital signature".into(),
            location:      String::new(),
            tsa_url:       Some("http://timestamp.digicert.com".into()),
            include_dss:   false,
            include_crl:   None,
            include_ocsp:  false,
            reserved_size: 32_768,
            field_name:    "Signature1".into(),
            image_path:    None,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let cli: Vec<String> = env::args().skip(1).collect();

    if cli.iter().any(|s| s == "-h" || s == "--help") {
        usage();
        process::exit(0);
    }

    let mut i = 0;
    // First positional arg = input PDF (not starting with -)
    if !cli.is_empty() && !cli[0].starts_with('-') {
        a.input = PathBuf::from(&cli[0]);
        i = 1;
    }

    while i < cli.len() {
        match cli[i].as_str() {
            "-o" | "--output"  => { i += 1; a.output = PathBuf::from(&cli[i]); }
            "-c" | "--cert"    => { i += 1; a.cert   = PathBuf::from(&cli[i]); }
            "-k" | "--key"     => { i += 1; a.key    = PathBuf::from(&cli[i]); }
            "-p" | "--page"    => { i += 1; a.page   = cli[i].parse().unwrap_or(1); }
            "--name"           => { i += 1; a.signer_name = cli[i].clone(); }
            "--contact"        => { i += 1; a.contact = cli[i].clone(); }
            "--reason" | "-r"  => { i += 1; a.reason  = cli[i].clone(); }
            "--location"       => { i += 1; a.location = cli[i].clone(); }
            "--tsa"            => { i += 1; a.tsa_url = Some(cli[i].clone()); }
            "--no-tsa"         => { a.tsa_url = None; }
            "--dss"            => { a.include_dss  = true; }
            "--crl"            => { a.include_crl  = Some(true); }
            "--no-crl"         => { a.include_crl  = Some(false); }
            "--ocsp"           => { a.include_ocsp = true; }
            "--invisible"      => { a.visible = false; }
            "--field"          => { i += 1; a.field_name = cli[i].clone(); }
            "--reserved"       => { i += 1; a.reserved_size = cli[i].parse().unwrap_or(32_768); }
            "--image"          => { i += 1; a.image_path = Some(PathBuf::from(&cli[i])); }
            "--tag"            => { i += 1; a.anchor_tag    = Some(cli[i].clone()); a.visible = true; }
            "--width"          => { i += 1; a.anchor_width  = cli[i].parse().ok(); }
            "--height"         => { i += 1; a.anchor_height = cli[i].parse().ok(); }
            "--tag-mode" => {
                i += 1;
                a.anchor_mode = match cli[i].to_lowercase().as_str() {
                    "front" | "in-front" | "in_front" => SignatureAnchorMode::InFront,
                    "overlay" | "over"                => SignatureAnchorMode::Overlay,
                    other => { eprintln!("Unknown --tag-mode: {other}"); process::exit(1); }
                };
            }
            "-f" | "--format" => {
                i += 1;
                a.format = match cli[i].to_lowercase().as_str() {
                    "pkcs7" | "p7"        => SignatureFormat::Pkcs7,
                    "pades" | "cades"     => SignatureFormat::PAdES,
                    other => { eprintln!("Unknown --format: {other}"); process::exit(1); }
                };
            }
            "-l" | "--level" => {
                i += 1;
                a.pades_level = match cli[i].to_lowercase().as_str() {
                    "b-b" | "bb"   => PadesLevel::B_B,
                    "b-t" | "bt"   => PadesLevel::B_T,
                    "b-lt" | "blt" => PadesLevel::B_LT,
                    "b-lta"|"blta" => PadesLevel::B_LTA,
                    other => { eprintln!("Unknown --level: {other}"); process::exit(1); }
                };
            }
            "--rect" => {
                i += 1;
                let parts: Vec<f64> = cli[i].split(',')
                    .map(|s| s.trim().parse().unwrap_or(0.0)).collect();
                if parts.len() == 4 {
                    a.rect    = Some([parts[0], parts[1], parts[2], parts[3]]);
                    a.visible = true;
                } else {
                    eprintln!("--rect requires 4 comma-separated values: x1,y1,x2,y2");
                    process::exit(1);
                }
            }
            other => { eprintln!("Unknown option: {other}"); usage(); process::exit(1); }
        }
        i += 1;
    }

    // Validate anchor-tag constraints
    if a.anchor_tag.is_some() && (a.anchor_width.is_none() || a.anchor_height.is_none()) {
        eprintln!("Error: --tag requires both --width and --height");
        process::exit(1);
    }

    a
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args = parse_args();

    let format_label = match args.format {
        SignatureFormat::Pkcs7 => "PKCS7".to_string(),
        SignatureFormat::PAdES => format!("PAdES {}", match args.pades_level {
            PadesLevel::B_B  => "B-B",
            PadesLevel::B_T  => "B-T",
            PadesLevel::B_LT => "B-LT",
            PadesLevel::B_LTA=> "B-LTA",
        }),
    };

    println!("══════════════════════════════════════════════════════");
    println!("  rust-pdfbox  ·  Digital Signature Example");
    println!("══════════════════════════════════════════════════════");

    // ── 1. Load input PDF ──
    let pdf_bytes = fs::read(&args.input).unwrap_or_else(|e| {
        eprintln!("Cannot read input PDF {:?}: {e}", args.input); process::exit(1);
    });
    println!("  Input    : {:?}  ({} bytes)", args.input, pdf_bytes.len());

    let doc = rust_pdfbox::Document::load_from_bytes(&pdf_bytes).unwrap_or_else(|e| {
        eprintln!("Failed to parse PDF: {e}"); process::exit(1);
    });
    println!("  Pages    : {}", doc.page_count());

    // ── 2. Load cert chain ──
    let cert_pem = fs::read_to_string(&args.cert).unwrap_or_else(|e| {
        eprintln!("Cannot read cert {:?}: {e}", args.cert); process::exit(1);
    });
    let cert_count = cert_pem.matches("-----BEGIN CERTIFICATE-----").count();
    if cert_count == 0 {
        eprintln!("No certificates found in {:?}", args.cert); process::exit(1);
    }
    println!("  Certs    : {cert_count} certificate(s)");

    // ── 3. Load private key ──
    let key_pem = fs::read_to_string(&args.key).unwrap_or_else(|e| {
        eprintln!("Cannot read key {:?}: {e}", args.key); process::exit(1);
    });
    println!("  Key      : {:?}", args.key);

    // ── 4. Build SignOptions ──
    // Resolve CRL / OCSP defaults per format (mirrors rust_pdf_signing defaults)
    let include_crl = args.include_crl.unwrap_or(matches!(args.format, SignatureFormat::Pkcs7));
    let include_dss = args.include_dss
        || matches!(args.pades_level, PadesLevel::B_LT | PadesLevel::B_LTA);

    let opts = SignOptions {
        format:        args.format.clone(),
        pades_level:   args.pades_level.clone(),
        timestamp_url: args.tsa_url.clone(),
        include_crl,
        include_ocsp:  args.include_ocsp,
        include_dss,
        page:          args.page,
        rect:          args.rect,
        visible_signature: args.visible,
        anchor_tag:    args.anchor_tag.clone(),
        anchor_width:  args.anchor_width,
        anchor_height: args.anchor_height,
        anchor_mode:   args.anchor_mode.clone(),
        signer_name:   args.signer_name.clone(),
        contact_info:  args.contact.clone(),
        reason:        args.reason.clone(),
        location:      args.location.clone(),
        reserved_size: args.reserved_size,
        field_name:    args.field_name.clone(),
        image_path:    args.image_path.clone(),
    };

    // ── 5. Print summary ──
    println!("  Format   : {format_label}");
    println!("  Page     : {}", opts.page);
    if opts.visible_signature {
        match opts.rect {
            Some(r) => println!("  Rect     : [{} {} {} {}]", r[0], r[1], r[2], r[3]),
            None    => println!("  Rect     : default"),
        }
        if let Some(ref tag) = opts.anchor_tag {
            println!("  Anchor   : tag={tag:?} w={:?} h={:?} mode={:?}",
                opts.anchor_width, opts.anchor_height, opts.anchor_mode);
        }
    } else {
        println!("  Visible  : false (invisible signature)");
    }
    if !opts.reason.is_empty()       { println!("  Reason   : {}", opts.reason); }
    if !opts.signer_name.is_empty()  { println!("  Name     : {}", opts.signer_name); }
    if !opts.contact_info.is_empty() { println!("  Contact  : {}", opts.contact_info); }
    if !opts.location.is_empty()     { println!("  Location : {}", opts.location); }
    match &opts.timestamp_url {
        Some(u) => println!("  TSA      : {u}"),
        None    => println!("  TSA      : disabled"),
    }
    println!("  CRL      : {}", opts.include_crl);
    println!("  OCSP     : {}", opts.include_ocsp);
    println!("  DSS      : {}", opts.include_dss);
    println!("  Reserved : {} bytes", opts.reserved_size);
    println!();

    // ── 6. Sign ──
    println!("  Signing …");
    let signed = sign_pdf(&pdf_bytes, &cert_pem, &key_pem, &opts)
        .unwrap_or_else(|e| {
            eprintln!("Signing failed: {e}"); process::exit(1);
        });
    println!("  ✅ Signed PDF: {} bytes", signed.len());

    // ── 7. Write output ──
    fs::write(&args.output, &signed).unwrap_or_else(|e| {
        eprintln!("Cannot write {:?}: {e}", args.output); process::exit(1);
    });
    println!("  Output   : {:?}", args.output);
    println!();

    // ── 8. Verify ──
    println!("  Verifying …");
    match validate_pdf_full(&signed) {
        Ok(results) if results.is_empty() => {
            println!("  ⚠  No signature fields found in output PDF.");
        }
        Ok(results) => {
            for (i, r) in results.iter().enumerate() {
                let icon = if r.is_valid() { "✅" } else { "❌" };
                let ts_label = if r.is_document_timestamp { " [DocTimestamp]" } else { "" };
                println!("  Signature [{}] {}{}: field='{}'",
                    i + 1, icon, ts_label,
                    r.field_name.as_deref().unwrap_or("unnamed"));
                println!("    Filter         : {}", r.filter.as_deref().unwrap_or("-"));
                println!("    SubFilter      : {}", r.sub_filter.as_deref().unwrap_or("-"));
                println!("    Reason         : {}", r.reason.as_deref().unwrap_or("-"));
                println!("    Contact        : {}", r.contact_info.as_deref().unwrap_or("-"));
                println!("    Signing time   : {}", r.signing_time.as_deref().unwrap_or("-"));
                println!("    ByteRange      : {:?}", r.byte_range);
                println!("    Covers file    : {}", r.byte_range_covers_whole_file);
                // ── cryptographic ──
                println!("    Digest match   : {}", r.digest_match);
                println!("    CMS sig valid  : {}", r.cms_signature_valid);
                // ── chain ──
                println!("    Chain valid    : {}", r.certificate_chain_valid);
                println!("    Chain trusted  : {}", r.certificate_chain_trusted);
                for w in &r.chain_warnings {
                    println!("    ⚠  Chain       : {w}");
                }
                // ── LTV ──
                println!("    Has timestamp  : {}", r.has_timestamp);
                println!("    Has DSS        : {}", r.has_dss);
                if r.has_dss {
                    println!("      DSS CRLs     : {}", r.dss_crl_count);
                    println!("      DSS OCSPs    : {}", r.dss_ocsp_count);
                    println!("      DSS Certs    : {}", r.dss_cert_count);
                    println!("      Has VRI      : {}", r.has_vri);
                }
                println!("    CMS revoc data : {}", r.has_cms_revocation_data);
                println!("    LTV enabled    : {}", r.is_ltv_enabled);
                // ── modification detection ──
                println!("    No unauth mods : {}", r.no_unauthorized_modifications);
                if !r.modification_notes.is_empty() {
                    println!("    Modification notes:");
                    for note in &r.modification_notes {
                        println!("      - {note}");
                    }
                }
                // ── security attack defences ──
                println!("    ByteRange valid: {}", r.byte_range_valid);
                println!("    Not wrapped    : {}", r.signature_not_wrapped);
                if let Some(level) = r.certification_level {
                    println!("    Cert level     : {level}");
                    println!("    Cert perm ok   : {}", r.certification_permission_ok);
                }
                for sw in &r.security_warnings {
                    println!("    🔒 Security    : {sw}");
                }
                // ── certificates ──
                println!("    Certificates   : {}", r.certificates.len());
                for (ci, cert) in r.certificates.iter().enumerate() {
                    println!("      [{ci}] Subject  : {}", cert.subject);
                    println!("           Issuer   : {}", cert.issuer);
                    println!("           Serial   : {}", cert.serial_number);
                    let nb = cert.not_before.map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string()).unwrap_or_else(|| "?".into());
                    let na = cert.not_after.map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string()).unwrap_or_else(|| "?".into());
                    println!("           Valid    : {nb} → {na}");
                    if cert.is_self_signed { println!("           ⚠  Self-signed"); }
                    if cert.is_expired     { println!("           ❌ Expired"); }
                }
                for err in &r.errors {
                    println!("    ❌ Error       : {err}");
                }
            }
            let all_ok = results.iter().all(|r| r.digest_match && r.cms_signature_valid);
            println!();
            if all_ok {
                println!("  ✅ All signatures cryptographically verified.");
            } else {
                println!("  ❌ One or more signatures failed verification.");
                process::exit(2);
            }
        }
        Err(e) => { eprintln!("  Verification error: {e}"); process::exit(2); }
    }
    println!("══════════════════════════════════════════════════════");
}

