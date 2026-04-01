//! Example: extract and print all text from a PDF.
//!
//! Usage:
//!   cargo run --example extract_text -- path/to/file.pdf

use rust_pdfbox::Document;
use rust_pdfbox::text::extract_text;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: extract_text <path/to/file.pdf>");
        std::process::exit(1);
    });

    let doc = match Document::load(&path) {
        Ok(d) => d,
        Err(e) => { eprintln!("Error loading {path}: {e}"); std::process::exit(2); }
    };

    let pages = match doc.pages() {
        Ok(p) => p,
        Err(e) => { eprintln!("Cannot access pages: {e}"); std::process::exit(3); }
    };

    let page_count = pages.count();
    eprintln!("Extracting text from {page_count} page(s)...");

    for (i, page) in pages.iter().enumerate() {
        // Get the raw content stream bytes for this page
        let content_bytes = match page.contents_object() {
            Some(obj) => match obj.as_stream() {
                Some(s) => s.data.clone(),
                None => continue,
            },
            None => continue,
        };

        let text = extract_text(&content_bytes, None);
        if !text.trim().is_empty() {
            println!("=== Page {} ===", i + 1);
            println!("{}", text);
        }
    }
}

