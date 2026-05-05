#[cfg(not(feature = "forms"))]
fn main() {
    eprintln!("Enable the `forms` feature to use this example.");
}

#[cfg(feature = "forms")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rust_pdfbox::Document;

    let mut args = std::env::args();
    let _bin = args.next();
    let input = match args.next() {
        Some(v) => v,
        None => {
            eprintln!("Usage: cargo run --example read_xfa -- <input.pdf>");
            std::process::exit(2);
        }
    };

    let doc = Document::load(&input)?;

    if !doc.has_xfa_form() {
        println!("No XFA payload found in {input}");
        return Ok(());
    }

    let xfa = doc.xfa_form().expect("checked above");
    println!("XFA packets: {}", xfa.packets().len());

    for packet in xfa.packets() {
        let name = packet.name().unwrap_or("<unnamed>");
        println!("- {name}: {} bytes", packet.xml().len());
    }

    if let Some(datasets) = xfa.datasets_xml() {
        println!("datasets packet size: {} bytes", datasets.len());
    }

    Ok(())
}

