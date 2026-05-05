#[cfg(not(feature = "image-extract"))]
fn main() {
    eprintln!("Enable the `image-extract` feature to use this example.");
}

#[cfg(feature = "image-extract")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rust_pdfbox::image_extract::ImageExportFormat;
    use rust_pdfbox::Document;

    let mut args = std::env::args();
    let _bin = args.next();

    let input = match args.next() {
        Some(v) => v,
        None => {
            eprintln!("Usage: cargo run --features image-extract --example extract_images -- <input.pdf> <output_dir> [page_index]");
            std::process::exit(2);
        }
    };

    let output_dir = match args.next() {
        Some(v) => v,
        None => {
            eprintln!("Usage: cargo run --features image-extract --example extract_images -- <input.pdf> <output_dir> [page_index]");
            std::process::exit(2);
        }
    };

    let page_filter = args
        .next()
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|e| format!("invalid page_index: {e}"))?;

    std::fs::create_dir_all(&output_dir)?;

    let doc = Document::load(&input)?;
    let page_count = doc.page_count();

    let mut exported = 0usize;
    for page_index in 0..page_count {
        if page_filter.is_some() && page_filter != Some(page_index) {
            continue;
        }

        let images = doc.extract_images(page_index)?;
        for (idx, img) in images.iter().enumerate() {
            let prefer_jpeg = img
                .filter_names()
                .iter()
                .any(|f| matches!(f.as_str(), "DCTDecode" | "DCT"));

            let (format, ext) = if prefer_jpeg {
                (ImageExportFormat::Jpeg, "jpg")
            } else {
                (ImageExportFormat::Png, "png")
            };

            let out_path = std::path::Path::new(&output_dir)
                .join(format!("page_{:03}_img_{:03}.{ext}", page_index + 1, idx + 1));

            if let Err(err) = img.save_as(&out_path, format) {
                eprintln!(
                    "skip page {} image {} ({}x{}): {}",
                    page_index + 1,
                    idx + 1,
                    img.width(),
                    img.height(),
                    err
                );
                continue;
            }

            println!(
                "saved {} ({}x{}, {:?}, filters={:?})",
                out_path.display(),
                img.width(),
                img.height(),
                img.color_space(),
                img.filter_names()
            );
            exported += 1;
        }
    }

    println!("done: exported {exported} images");
    Ok(())
}

