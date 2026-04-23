use rust_pdfbox::Document;
use rust_pdfbox::forms::set_field_value;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: {} <input.pdf> <output.pdf> <field_name> <value>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];
    let field_name = &args[3];
    let field_value = &args[4];

    println!("Loading {}", input_path);
    let mut doc = Document::load(input_path)?;

    // Retrieve field ID
    let field_id = {
        let acro_form = doc.acro_form().expect("Document has no AcroForm");
        let field = acro_form.get_field(field_name).expect("Field not found");
        field.id()
    };

    println!("Found field '{}', setting value to '{}'", field_name, field_value);
    set_field_value(&mut doc, field_id, field_value);

    println!("Saving to {}", output_path);
    doc.save(output_path)?;

    println!("Done!");
    Ok(())
}

