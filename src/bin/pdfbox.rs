use clap::{Parser, Subcommand};
use rust_pdfbox::Document;
use std::path::PathBuf;

/// Rust PDFBox CLI (Equivalent to Apache PDFBox Tools)
#[derive(Parser, Debug)]
#[command(name = "rust-pdfbox")]
#[command(about = "Command line tools for PDF manipulation and extraction", version)]
struct Cli {
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::ExtractText { input, output } => {
            println!("Loading document: {}", input.display());
            let doc = Document::load(input)?;
            
            // This assumes `text` feature is enabled.
            #[cfg(feature = "text")]
            {
                let mut full_text = String::new();
                for page in doc.pages() {
                    // This assumes `extract_text` is adapted for page-level, 
                    // or we handle text via standard stream processing.
                    // For now, this is a placeholder CLI stub for text extraction.
                    full_text.push_str("... Page text extracted ...\n");
                }
                
                if let Some(out_path) = output {
                    std::fs::write(out_path, full_text)?;
                    println!("Text saved to {}", out_path.display());
                } else {
                    println!("{}", full_text);
                }
            }
            #[cfg(not(feature = "text"))]
            {
                println!("Error: 'text' feature not compiled in rust-pdfbox.");
            }
        }
        Commands::ExtractImages { input, output_dir } => {
            println!("Loading document: {}", input.display());
            #[allow(unused_variables)]
            let mut doc = Document::load(input)?;
            
            #[cfg(feature = "image-extract")]
            {
                std::fs::create_dir_all(output_dir)?;
                
                let images = rust_pdfbox::image_extract::export::export_images(&mut doc)?;
                println!("Found {} images.", images.len());
                
                for (idx, img) in images.iter().enumerate() {
                    let out_path = output_dir.join(format!("image_{:04}.png", idx + 1));
                    img.image.save(&out_path)?;
                    println!("Saved {}", out_path.display());
                }
            }
            #[cfg(not(feature = "image-extract"))]
            {
                println!("Error: 'image-extract' feature not compiled in rust-pdfbox.");
            }
        }
    }

    Ok(())
}