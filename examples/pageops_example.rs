use std::env;
use rust_pdfbox::Document;
use rust_pdfbox::pageops::{PdfSplitter, PdfMerger};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage:");
        eprintln!("  {} split <input.pdf> <pages_per_file>", args[0]);
        eprintln!("  {} merge <output.pdf> <input1.pdf> <input2.pdf>", args[0]);
        std::process::exit(1);
    }

    let cmd = &args[1];

    if cmd == "split" {
        let input_path = &args[2];
        let pages_per_file: usize = args[3].parse()?;

        println!("Loading {} for splitting...", input_path);
        let mut doc = Document::load(input_path)?;

        let mut splitter = PdfSplitter::new(&mut doc);
        let docs = splitter.split(pages_per_file)?;

        for (i, part) in docs.iter().enumerate() {
            let out_path = format!("{}_part{}.pdf", input_path, i + 1);
            part.save(&out_path)?;
            println!("Saved {}", out_path);
        }
    } else if cmd == "merge" {
        let output_path = &args[2];
        let input1_path = &args[3];
        let input2_path = &args[4];

        println!("Merging {} and {}...", input1_path, input2_path);
        let doc1 = Document::load(input1_path)?;
        let doc2 = Document::load(input2_path)?;

        let mut merger = PdfMerger::new();
        merger.append(&doc1)?;
        merger.append(&doc2)?;

        let merged = merger.finish();
        merged.save(output_path)?;

        println!("Saved merged document to {}", output_path);
    }

    Ok(())
}

