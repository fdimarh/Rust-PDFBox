use clap::{Parser, Subcommand};
use rust_pdfbox::signing::{self, SignOptions, SignatureFormat, PadesLevel};
use rust_pdfbox::preflight::PreflightValidator;
use rust_pdfbox::Document;
use std::path::PathBuf;
use std::process;

/// Rust PDFBox CLI (Equivalent to Apache PDFBox Tools)
#[derive(Parser, Debug)]
#[command(name = "rust-pdfbox")]
#[command(about = "Command line tools for PDF manipulation and extraction", version)]
#[command(disable_help_flag = false)]
struct Cli {
    /// Enable verbose output
    #[arg(short = 'v', long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Extract text from a PDF document
    ExtractText {
        /// Input PDF file
        #[arg(required = true)]
        input: PathBuf,
        /// Output text file (if omitted, prints to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Extract images from a PDF document
    ExtractImages {
        /// Input PDF file
        #[arg(required = true)]
        input: PathBuf,
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,
    },

    /// Sign a PDF with a local private key
    Sign {
        /// Input PDF file
        #[arg(required = true)]
        input: PathBuf,
        /// Output signed PDF file
        #[arg(short, long, required = true)]
        output: PathBuf,
        /// PEM certificate chain file
        #[arg(short = 'c', long, required = true)]
        cert: PathBuf,
        /// PEM private key file
        #[arg(short = 'k', long, required = true)]
        key: PathBuf,
        /// Signature format: pkcs7 or pades
        #[arg(long, default_value = "pkcs7")]
        format: String,
        /// PAdES level: b-b, b-t, b-lt, b-lta
        #[arg(long, default_value = "b-b")]
        pades_level: String,
        /// Page number (1-based) for visible signature
        #[arg(long, default_value_t = 1)]
        page: u32,
        /// Visible signature rectangle: x1,y1,x2,y2
        #[arg(long)]
        rect: Option<String>,
        /// Signature appearance image (PNG/JPEG)
        #[arg(long)]
        image: Option<PathBuf>,
        /// Timestamp authority URL
        #[arg(long)]
        tsa_url: Option<String>,
        /// Password for encrypted PDF
        #[arg(long)]
        password: Option<String>,
    },

    /// Validate a signed PDF document
    Validate {
        /// Input signed PDF file
        #[arg(required = true)]
        input: PathBuf,
        /// Whether to check LTV
        #[arg(long)]
        check_ltv: bool,
    },

    /// Check PDF/A-1b compliance
    Preflight {
        /// Input PDF file
        #[arg(required = true)]
        input: PathBuf,
        /// Output JSON with validation errors
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Encrypt PDF with password protection
    Protect {
        /// Input PDF file
        #[arg(required = true)]
        input: PathBuf,
        /// Output encrypted PDF file
        #[arg(short, long, required = true)]
        output: PathBuf,
        /// User password (to open)
        #[arg(long)]
        user_password: Option<String>,
        /// Owner password (to change permissions)
        #[arg(long)]
        owner_password: Option<String>,
    },

    /// Display document info (pages, encryption, metadata)
    Info {
        /// Input PDF file
        #[arg(required = true)]
        input: PathBuf,
    },
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::ExtractText { input, output } => {
            if cli.verbose { eprintln!("ℹ Loading document: {}", input.display()); }
            let doc = Document::load(input)
                .map_err(|e| format!("Failed to open '{}': {e}", input.display()))?;

            #[cfg(feature = "text")]
            {
                let mut full_text = String::new();
                full_text.push_str("... Text extraction requires resolved CMap integration ...\n");

                if let Some(out_path) = output {
                    std::fs::write(out_path, full_text)
                        .map_err(|e| format!("Failed to write '{}': {e}", out_path.display()))?;
                    println!("✓ Text saved to {}", out_path.display());
                } else {
                    print!("{}", full_text);
                }
            }
            #[cfg(not(feature = "text"))]
            {
                eprintln!("✗ 'text' feature not compiled in rust-pdfbox.");
                process::exit(1);
            }
        }

        Commands::ExtractImages { input, output_dir } => {
            if cli.verbose { eprintln!("ℹ Loading document: {}", input.display()); }
            let mut doc = Document::load(input)
                .map_err(|e| format!("Failed to open '{}': {e}", input.display()))?;

            #[cfg(feature = "image-extract")]
            {
                std::fs::create_dir_all(output_dir)
                    .map_err(|e| format!("Cannot create output dir '{}': {e}", output_dir.display()))?;
                let images = rust_pdfbox::image_extract::export::export_images(&mut doc)
                    .map_err(|e| format!("Image extraction failed: {e}"))?;
                println!("✓ Found {} images.", images.len());

                for (idx, img) in images.iter().enumerate() {
                    let out_path = output_dir.join(format!("image_{:04}.png", idx + 1));
                    img.image.save(&out_path)
                        .map_err(|e| format!("Failed to save '{}': {e}", out_path.display()))?;
                    println!("  Saved {}", out_path.display());
                }
            }
            #[cfg(not(feature = "image-extract"))]
            {
                eprintln!("✗ 'image-extract' feature not compiled in rust-pdfbox.");
                process::exit(1);
            }
        }

        Commands::Sign {
            input,
            output,
            cert,
            key,
            format,
            pades_level,
            page,
            rect,
            image,
            tsa_url,
            password,
        } => {
            if cli.verbose { eprintln!("ℹ Loading PDF: {}", input.display()); }
            let pdf_bytes = std::fs::read(input)
                .map_err(|e| format!("Cannot read '{}': {e}", input.display()))?;

            let cert_pem = std::fs::read_to_string(cert)
                .map_err(|e| format!("Cannot read certificate '{}': {e}", cert.display()))?;
            let key_pem = std::fs::read_to_string(key)
                .map_err(|e| format!("Cannot read key '{}': {e}", key.display()))?;

            let sig_format = match format.as_str() {
                "pades" => SignatureFormat::PAdES,
                _ => SignatureFormat::Pkcs7,
            };

            let pades = match pades_level.as_str() {
                "b-t" => PadesLevel::B_T,
                "b-lt" => PadesLevel::B_LT,
                "b-lta" => PadesLevel::B_LTA,
                _ => PadesLevel::B_B,
            };

            let rect_parsed = rect.as_ref().and_then(|s| {
                let parts: Vec<f64> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                if parts.len() == 4 { Some([parts[0], parts[1], parts[2], parts[3]]) } else {
                    eprintln!("⚠ Warning: --rect expected 4 comma-separated values (x1,y1,x2,y2), got '{s}'");
                    None
                }
            });

            // Load signature appearance image if provided
            let sig_image: Option<Vec<u8>> = if let Some(img_path) = image {
                if cli.verbose { eprintln!("ℹ Loading signature image: {}", img_path.display()); }
                // Read as raw bytes — the signing pipeline may embed them.
                // For now, warn that --image is not yet integrated into sign_pdf.
                eprintln!("⚠ --image flag recognized but image embedding in signature appearance is not yet implemented; signing will proceed without visual.");
                None
            } else {
                None
            };
            let _ = sig_image; // suppress unused warning

            let opts = SignOptions {
                format: sig_format,
                pades_level: pades,
                page: *page,
                rect: rect_parsed,
                timestamp_url: tsa_url.clone(),
                ..SignOptions::default()
            };

            if cli.verbose {
                eprintln!("ℹ Signing PDF (format={:?}, page={page}, rect={rect_parsed:?}) ...", opts.format);
            }
            let signed = signing::sign_pdf(&pdf_bytes, &cert_pem, &key_pem, password.as_deref(), &opts)
                .map_err(|e| format!("Signing failed: {e}"))?;

            std::fs::write(output, &signed)
                .map_err(|e| format!("Cannot write '{}': {e}", output.display()))?;
            println!("✓ Signed PDF saved to {}", output.display());
        }

        Commands::Validate { input, check_ltv } => {
            if cli.verbose { eprintln!("ℹ Loading document: {}", input.display()); }
            let doc = Document::load(input)
                .map_err(|e| format!("Failed to open '{}': {e}", input.display()))?;

            #[cfg(feature = "signing")]
            {
                let result = rust_pdfbox::signing::validator::validate_signatures(&doc)
                    .map_err(|e| format!("Signature validation failed: {e}"))?;
                println!("✓ Signature count: {}", result.signatures.len());
                for (i, sig) in result.signatures.iter().enumerate() {
                    let status = if sig.is_valid { "✅ Valid" } else { "❌ Invalid" };
                    println!("  [{i}] Status: {status}");
                    println!("      Signed at: {:?}", sig.signed_at);
                    println!("      Signer: {}", sig.signer_name.as_deref().unwrap_or("(unknown)"));
                    println!("      Reason: {}", sig.reason.as_deref().unwrap_or("(not specified)"));
                    if *check_ltv {
                        let ltv = if sig.is_ltv_enabled { "✅ LTV enabled" } else { "❌ Not LTV" };
                        println!("      LTV: {ltv}");
                        println!("      DSS CRLs: {}, OCSPs: {}", sig.dss_crl_count, sig.dss_ocsp_count);
                    }
                }
                println!(
                    "{}",
                    if result.is_valid { "✅ All signatures valid" } else { "❌ Some signatures invalid" }
                );
                if !result.errors.is_empty() {
                    eprintln!("Errors:");
                    for e in &result.errors {
                        eprintln!("  - {e}");
                    }
                }
            }
            #[cfg(not(feature = "signing"))]
            {
                eprintln!("✗ 'signing' feature not compiled in rust-pdfbox.");
                process::exit(1);
            }
        }

        Commands::Preflight { input, output } => {
            if cli.verbose { eprintln!("ℹ Loading document: {}", input.display()); }
            let doc = Document::load(input)
                .map_err(|e| format!("Failed to open '{}': {e}", input.display()))?;
            let validator = PreflightValidator::pdf_a1b();
            let result = validator.validate(&doc);

            if result.is_valid {
                println!("✅ PDF/A-1b compliant — 0 errors");
            } else {
                println!("❌ PDF/A-1b: {} errors found", result.errors.len());
                for err in &result.errors {
                    println!("  [{:4}] {}", err.rule_id, err.message);
                }
            }

            if let Some(out_path) = output {
                let mut json = String::from("{\n  \"is_valid\": ");
                json.push_str(if result.is_valid { "true" } else { "false" });
                json.push_str(",\n  \"errors\": [\n");
                for (i, err) in result.errors.iter().enumerate() {
                    if i > 0 { json.push_str(",\n"); }
                    json.push_str(&format!(
                        "    {{\"rule_id\": {:?}, \"message\": {:?}}}",
                        err.rule_id, err.message
                    ));
                }
                json.push_str("\n  ]\n}\n");
                std::fs::write(out_path, &json)
                    .map_err(|e| format!("Failed to write '{}': {e}", out_path.display()))?;
                if cli.verbose { eprintln!("ℹ JSON output written to {}", out_path.display()); }
            }
        }

        Commands::Protect {
            input,
            output,
            user_password,
            owner_password,
        } => {
            if cli.verbose { eprintln!("ℹ Loading document: {}", input.display()); }
            let mut doc = Document::load(input)
                .map_err(|e| format!("Failed to open '{}': {e}", input.display()))?;

            let policy = rust_pdfbox::protection::StandardProtectionPolicy {
                user_password: Some(user_password.clone().unwrap_or_default()),
                owner_password: owner_password.clone().unwrap_or_else(|| {
                    format!("rust-pdfbox-{}", std::process::id())
                }),
                ..rust_pdfbox::protection::StandardProtectionPolicy::default()
            };

            #[cfg(feature = "crypto")]
            {
                doc.protect(&policy)
                    .map_err(|e| format!("Encryption failed: {e}"))?;
                doc.save(output)
                    .map_err(|e| format!("Cannot write '{}': {e}", output.display()))?;
                println!("✓ Encrypted PDF saved to {}", output.display());
            }
            #[cfg(not(feature = "crypto"))]
            {
                eprintln!("✗ 'crypto' feature not compiled in rust-pdfbox.");
                process::exit(1);
            }
        }

        Commands::Info { input } => {
            if cli.verbose { eprintln!("ℹ Loading document: {}", input.display()); }
            let doc = Document::load(input)
                .map_err(|e| format!("Failed to open '{}': {e}", input.display()))?;

            println!("📄 File: {}", input.display());
            println!("   Pages:   {}", doc.page_count());
            println!("   Objects: {}", doc.objects.len());

            #[cfg(feature = "text")]
            {
                let _ = doc.page_count(); // suppress unused
            }

            let trailer = doc.trailer();
            if let Some(info) = trailer.get(&rust_pdfbox::cos::CosName::new(b"Info".to_vec())) {
                if let Some(dict) = info.as_dictionary() {
                    let get_str = |key: &str| -> Option<String> {
                        let k = rust_pdfbox::cos::CosName::new(key.as_bytes().to_vec());
                        dict.get(&k)
                            .and_then(|o| o.as_string().map(|s| String::from_utf8_lossy(s).to_string()))
                    };
                    for field in ["Title", "Author", "Subject", "Keywords", "Creator", "Producer"] {
                        if let Some(v) = get_str(field) {
                            println!("   {}: {}", field, v);
                        }
                    }
                }
            }

            println!("   Encrypted: {}", doc.is_encrypted());

            #[cfg(feature = "signing")]
            {
                if let Ok(result) = rust_pdfbox::signing::validator::validate_signatures(&doc) {
                    if !result.signatures.is_empty() {
                        println!("   Signatures: {}", result.signatures.len());
                        for (i, sig) in result.signatures.iter().enumerate() {
                            println!("     [{i}] {}", sig.signer_name.as_deref().unwrap_or("(unknown)"));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("✗ Error: {e}");
        process::exit(1);
    }
}
