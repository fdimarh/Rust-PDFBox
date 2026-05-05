#[cfg(not(feature = "forms"))]
fn main() {
    eprintln!("Enable the `forms` feature to use this example.");
}

#[cfg(feature = "forms")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rust_pdfbox::forms::set_field_value;
    use rust_pdfbox::Document;

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "Usage: {} <input.pdf> <output.pdf> <field_name> <value>",
            args.first().map(String::as_str).unwrap_or("fill_form_xfa_hybrid")
        );
        std::process::exit(2);
    }

    let input_path = &args[1];
    let output_path = &args[2];
    let field_name = &args[3];
    let field_value = &args[4];

    let mut doc = Document::load(input_path)?;

    let field_id = {
        let form = doc.acro_form().ok_or("Document has no AcroForm")?;
        let field = form
            .get_field(field_name)
            .ok_or("AcroForm field not found")?;

        println!("Hybrid form: {}", form.is_hybrid_xfa());
        println!("Has XFA: {}", form.has_xfa());

        field.id()
    };

    set_field_value(&mut doc, field_id, field_value);

    if let Some(xfa) = doc.xfa_form() {
        println!("XFA packets: {}", xfa.packets().len());
        for packet in xfa.packets() {
            let packet_name = packet.name().unwrap_or("<unnamed>");
            println!("- {packet_name}: {} bytes", packet.xml().len());
        }

        if let Some(datasets) = xfa.datasets_xml() {
            println!("datasets packet size: {} bytes", datasets.len());
        }
    } else {
        println!("No XFA payload found; updated AcroForm only.");
    }

    doc.save(output_path)?;
    println!("Saved: {output_path}");

    Ok(())
}

