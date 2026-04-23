use rust_pdfbox::Document;

fn main() {
    let files = [
        "/Users/fdimarh/Downloads/compressed rust-pdfbox/out_less.pdf",
        "/Users/fdimarh/Downloads/compressed rust-pdfbox/out_recommended.pdf",
        "/Users/fdimarh/Downloads/compressed rust-pdfbox/out_extreme.pdf",
    ];

    for path in &files {
        let basename = path.rsplit('/').next().unwrap_or(path);
        print!("{}: ", basename);

        let bytes = std::fs::read(path).unwrap();

        match Document::load_from_bytes(&bytes) {
            Ok(doc) => {
                let pages = doc.page_count();
                println!("OK — {} pages, {} objects", pages, doc.objects.len());
            }
            Err(e) => {
                eprintln!("STRICT FAIL: {}", e);
                let (doc, report) = Document::load_lenient(&bytes);
                let pages = doc.page_count();
                let catalog = doc.catalog().is_some();
                println!(
                    "  lenient: {} pages, {} objects, catalog={}, skipped={}, warnings={}",
                    pages, doc.objects.len(), catalog, report.objects_skipped, report.warnings.len()
                );
                for w in report.warnings.iter().take(5) {
                    println!("    warn: {}", w);
                }
            }
        }
    }
}

