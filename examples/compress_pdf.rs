//! `compress_pdf` — CLI example for the PDF compression pipeline.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example compress_pdf --features compress -- \
//!     input.pdf output.pdf [--mode extreme|recommended|less]
//! ```
//!
//! # Options
//!
//! | Flag | Default | Description |
//! |---|---|---|
//! | `--mode extreme` | | JPEG q=40, DPI 96, all passes |
//! | `--mode recommended` | ✓ | JPEG q=72, DPI 150, all passes |
//! | `--mode less` | | Stream re-compress + dedup only |
//! | `--no-images` | | Disable image re-encoding |
//! | `--no-fonts` | | Disable font subsetting |
//! | `--no-cmyk` | | Disable CMYK → sRGB conversion |
//! | `--no-linearize` | | Disable linearization |
//! | `--jpeg-quality N` | mode default | Override JPEG quality (1–100) |
//! | `--max-dpi N` | mode default | Override max DPI threshold |

use rust_pdfbox::{
    Document,
    compress::{CompressOptions, CompressionMode},
};
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: compress_pdf <input.pdf> <output.pdf> [options]");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --mode extreme|recommended|less   Compression profile (default: recommended)");
        eprintln!("  --no-images                       Skip image re-encoding");
        eprintln!("  --no-fonts                        Skip font subsetting");
        eprintln!("  --no-cmyk                         Skip CMYK → sRGB conversion");
        eprintln!("  --no-linearize                    Skip linearization");
        eprintln!("  --jpeg-quality N                  Override JPEG quality (1–100)");
        eprintln!("  --max-dpi N                       Override max DPI threshold");
        process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    // Parse options.
    let mut mode = CompressionMode::Recommended;
    let mut no_images = false;
    let mut no_fonts = false;
    let mut no_cmyk = false;
    let mut no_linearize = false;
    let mut jpeg_quality: Option<u8> = None;
    let mut max_dpi: Option<u32> = None;
    let mut force_zopfli = false;

    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                i += 1;
                if i < args.len() {
                    mode = match args[i].as_str() {
                        "extreme"     => CompressionMode::Extreme,
                        "recommended" => CompressionMode::Recommended,
                        "less"        => CompressionMode::Less,
                        other => {
                            eprintln!("Unknown mode '{}'; valid: extreme, recommended, less", other);
                            process::exit(1);
                        }
                    };
                }
            }
            "--no-images"    => no_images = true,
            "--no-fonts"     => no_fonts = true,
            "--no-cmyk"      => no_cmyk = true,
            "--no-linearize" => no_linearize = true,
            "--zopfli"       => force_zopfli = true,
            "--jpeg-quality" => {
                i += 1;
                if i < args.len() {
                    jpeg_quality = args[i].parse::<u8>().ok();
                }
            }
            "--max-dpi" => {
                i += 1;
                if i < args.len() {
                    max_dpi = args[i].parse::<u32>().ok();
                }
            }
            flag => {
                eprintln!("Unknown option '{}'", flag);
                process::exit(1);
            }
        }
        i += 1;
    }

    // Build options from profile, then apply overrides.
    let mut opts = CompressOptions::for_mode(mode);
    if no_images    { opts.optimize_images = false; opts.optimize_png_images = false; }
    if no_fonts     { opts.subset_fonts = false; opts.font_remove_unused = false; }
    if no_cmyk      { opts.convert_cmyk_to_srgb = false; }
    if no_linearize { opts.linearize = false; }
    if force_zopfli { opts.use_zopfli = true; }
    if let Some(q)  = jpeg_quality { opts.image_jpeg_quality = q; }
    if let Some(d)  = max_dpi      { opts.image_max_dpi = d; }

    // Load — try strict first, fall back to lenient for real-world PDFs
    // that have non-standard xref entries, missing objects, etc.
    print!("Loading {}... ", input_path);
    let raw = match std::fs::read(input_path) {
        Ok(b) => b,
        Err(e) => { eprintln!("cannot read file: {}", e); process::exit(1); }
    };
    let mut doc = match Document::load_from_bytes(&raw) {
        Ok(d) => { println!("ok ({} bytes, strict)", d.source_len()); d }
        Err(strict_err) => {
            eprintln!("strict parse: {} — retrying lenient…", strict_err);
            let (d, report) = Document::load_lenient(&raw);
            if report.objects_skipped > 0 {
                eprintln!("  lenient: {} object(s) skipped", report.objects_skipped);
                for w in report.warnings.iter().take(5) {
                    eprintln!("  warn: {}", w);
                }
            }
            println!("ok ({} bytes, lenient)", d.source_len());
            d
        }
    };

    // Compress.
    let mode_label = match mode {
        CompressionMode::Extreme     => "extreme",
        CompressionMode::Recommended => "recommended",
        CompressionMode::Less        => "less",
        CompressionMode::Custom      => "custom",
    };
    println!("Compressing (mode={}) ...", mode_label);

    let report = match doc.compress(opts) {
        Ok(r) => r,
        Err(e) => { eprintln!("Compression error: {}", e); process::exit(1); }
    };

    // Save.
    print!("Saving {}... ", output_path);
    match doc.save(output_path) {
        Ok(()) => println!("ok"),
        Err(e) => { eprintln!("save error: {}", e); process::exit(1); }
    }

    // Report.
    println!();
    println!("── Compression report ──────────────────────────────────────────");
    println!("  Input size           : {:>10} bytes", report.bytes_before);
    println!("  Output size          : {:>10} bytes", report.bytes_after);
    println!("  Reduction            : {:>9.1}%", report.reduction_pct);
    println!();
    println!("  Streams recompressed : {:>6}", report.streams_recompressed);
    println!("  Images resampled     : {:>6}", report.images_resampled);
    println!("  Images PNG optimized : {:>6}", report.images_png_optimized);
    println!("  Images →CCITT G4     : {:>6}", report.images_ccitt_encoded);
    println!("  Images CMYK→sRGB     : {:>6}", report.images_cmyk_converted);
    println!("  Images →gray         : {:>6}", report.images_grayscale_converted);
    println!("  Fonts subsetted      : {:>6}", report.fonts_subsetted);
    println!("  Fonts removed        : {:>6}", report.fonts_removed);
    println!("  Objects deduped      : {:>6}", report.objects_deduped);
    println!("  Objects removed      : {:>6}", report.objects_removed);
    println!("  Errors skipped       : {:>6}", report.decode_errors_skipped);
    println!();
    println!("  Saved (streams)      : {:>10} bytes", report.bytes_saved_streams);
    println!("  Saved (images)       : {:>10} bytes", report.bytes_saved_images);
    println!("  Saved (fonts)        : {:>10} bytes", report.bytes_saved_fonts);
    println!("  Saved (dedup)        : {:>10} bytes", report.bytes_saved_dedup);
    println!("  Saved (metadata)     : {:>10} bytes", report.bytes_saved_metadata);
    println!("────────────────────────────────────────────────────────────────");
}

