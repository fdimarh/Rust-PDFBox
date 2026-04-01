//! Example: print PDF metadata and page count.
//!
//! Usage:
//!   cargo run --example read_info -- path/to/file.pdf

use rust_pdfbox::Document;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: read_info <path/to/file.pdf>");
        std::process::exit(1);
    });

    let doc = match Document::load(&path) {
        Ok(d) => d,
        Err(e) => { eprintln!("Error loading {path}: {e}"); std::process::exit(2); }
    };

    println!("File          : {path}");
    println!("Source length : {} bytes", doc.source_len());
    println!("Objects       : {}", doc.object_count());
    println!("Pages         : {}", doc.page_count());

    if let Some(cat_ref) = doc.catalog_ref() {
        println!("Catalog ref   : {} {} R", cat_ref.object_number, cat_ref.generation);
    }

    // Print /Info metadata if available
    let trailer = doc.trailer();
    if let Some(info_ref) = trailer.get(&rust_pdfbox::cos::CosName::info()) {
        if let Some(info_id) = info_ref.as_reference() {
            if let Some(info_obj) = doc.objects.get(&info_id) {
                if let Some(dict) = info_obj.as_dictionary() {
                    println!("\n--- Document Info ---");
                    for key in &[b"Title".as_slice(), b"Author", b"Subject",
                                  b"Creator", b"Producer", b"CreationDate"] {
                        let name = rust_pdfbox::cos::CosName::new(key.to_vec());
                        if let Some(val) = dict.get(&name) {
                            if let Some(s) = val.as_string_lossy() {
                                println!("{:15}: {}", String::from_utf8_lossy(key), s);
                            }
                        }
                    }
                }
            }
        }
    }

    // Iterate pages and print their media boxes
    if let Ok(pages) = doc.pages() {
        println!("\n--- Pages ---");
        for (i, page) in pages.iter().enumerate() {
            let mb = page.media_box()
                .map(|r| format!("{:.0}×{:.0}", r.width(), r.height()))
                .unwrap_or_else(|| "no media box".into());
            let rot = page.rotation();
            let rot_str = if rot != 0 { format!(" rot={rot}°") } else { String::new() };
            println!("  Page {:3}: {}{}", i + 1, mb, rot_str);
        }
    }
}

