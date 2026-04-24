use rust_pdfbox::cos::{CosDictionary, CosName, CosObject};
use rust_pdfbox::parser::xref::XRefEntry;
use rust_pdfbox::pdmodel::{DocumentBuilder, PageSize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
struct VariantConfig {
    file_stem: &'static str,
    text_value: Option<&'static str>,
    checkbox_on: bool,
    radio_on: bool,
    combo_value: &'static str,
    list_value: &'static str,
}

const VARIANTS: &[VariantConfig] = &[
    VariantConfig {
        file_stem: "00_base",
        text_value: None,
        checkbox_on: false,
        radio_on: false,
        combo_value: "Option A",
        list_value: "Option A",
    },
    VariantConfig {
        file_stem: "01_text_filled",
        text_value: Some("Hello from rust-pdfbox"),
        checkbox_on: false,
        radio_on: false,
        combo_value: "Option A",
        list_value: "Option A",
    },
    VariantConfig {
        file_stem: "02_checkbox_on",
        text_value: None,
        checkbox_on: true,
        radio_on: false,
        combo_value: "Option A",
        list_value: "Option A",
    },
    VariantConfig {
        file_stem: "03_radio_on",
        text_value: None,
        checkbox_on: false,
        radio_on: true,
        combo_value: "Option A",
        list_value: "Option A",
    },
    VariantConfig {
        file_stem: "04_combo_option_b",
        text_value: None,
        checkbox_on: false,
        radio_on: false,
        combo_value: "Option B",
        list_value: "Option A",
    },
    VariantConfig {
        file_stem: "05_list_option_c",
        text_value: None,
        checkbox_on: false,
        radio_on: false,
        combo_value: "Option A",
        list_value: "Option C",
    },
    VariantConfig {
        file_stem: "06_all_active",
        text_value: Some("All active state"),
        checkbox_on: true,
        radio_on: true,
        combo_value: "Option C",
        list_value: "Option B",
    },
];

fn ensure_parent_dir(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn write_form(output_path: &Path, variant: VariantConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

    let acro_form_id = doc.allocate_object_id();
    let parent_page_id = doc.page_object_ids().next().unwrap();

    let mut fields_array = Vec::new();
    let mut counter = 0;

    let mut add_field = |ft: &str, ff: i64, name: &str| {
        let field_id = doc.allocate_object_id();
        let mut field_dict = CosDictionary::new();
        field_dict.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Annot".to_vec())));
        field_dict.insert(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Widget".to_vec())));
        field_dict.insert(CosName::new(b"FT".to_vec()), CosObject::Name(CosName::new(ft.as_bytes().to_vec())));
        field_dict.insert(CosName::new(b"T".to_vec()), CosObject::String(name.as_bytes().to_vec()));
        field_dict.insert(CosName::new(b"F".to_vec()), CosObject::Integer(4));
        if ff != 0 {
            field_dict.insert(CosName::new(b"Ff".to_vec()), CosObject::Integer(ff));
        }
        field_dict.insert(CosName::new(b"P".to_vec()), CosObject::Reference(parent_page_id));

        let y_offset = 750.0 - (counter as f64 * 50.0);
        counter += 1;
        field_dict.insert(
            CosName::new(b"Rect".to_vec()),
            CosObject::Array(vec![
                CosObject::Real(100.0),
                CosObject::Real(y_offset - 20.0),
                CosObject::Real(300.0),
                CosObject::Real(y_offset),
            ]),
        );

        if ft == "Tx" || ft == "Ch" {
            field_dict.insert(
                CosName::new(b"DA".to_vec()),
                CosObject::String(b"/Helv 10 Tf 0 g".to_vec()),
            );
        }

        if name == "TextField" {
            if let Some(v) = variant.text_value {
                field_dict.insert(CosName::new(b"V".to_vec()), CosObject::String(v.as_bytes().to_vec()));
            }
        }

        if name == "CheckBox" {
            let state = if variant.checkbox_on { b"Yes" } else { b"Off" };
            field_dict.insert(CosName::new(b"V".to_vec()), CosObject::Name(CosName::new(state.to_vec())));
            field_dict.insert(CosName::new(b"AS".to_vec()), CosObject::Name(CosName::new(state.to_vec())));
        }

        if name == "RadioButton" {
            let state: &[u8] = if variant.radio_on { b"On" } else { b"Off" };
            field_dict.insert(CosName::new(b"V".to_vec()), CosObject::Name(CosName::new(state.to_vec())));
            field_dict.insert(CosName::new(b"AS".to_vec()), CosObject::Name(CosName::new(state.to_vec())));
        }

        if name == "ComboBox" || name == "ListBox" {
            let options = vec![
                CosObject::String(b"Option A".to_vec()),
                CosObject::String(b"Option B".to_vec()),
                CosObject::String(b"Option C".to_vec()),
            ];
            field_dict.insert(CosName::new(b"Opt".to_vec()), CosObject::Array(options));
            let selected = if name == "ComboBox" {
                variant.combo_value
            } else {
                variant.list_value
            };
            field_dict.insert(
                CosName::new(b"V".to_vec()),
                CosObject::String(selected.as_bytes().to_vec()),
            );
        }

        doc.insert_object(field_id, CosObject::Dictionary(field_dict));
        doc.xref.insert_if_absent(field_id, XRefEntry::InUse { offset: 0, generation: 0 });
        fields_array.push(CosObject::Reference(field_id));
    };

    add_field("Tx", 0, "TextField");
    add_field("Btn", 0, "CheckBox");
    add_field("Btn", 0x8000, "RadioButton");
    add_field("Ch", 0x20000, "ComboBox");
    add_field("Ch", 0, "ListBox");
    add_field("Btn", 0x10000, "PushButton");
    add_field("Sig", 0, "Signature");

    let mut acro_form = CosDictionary::new();
    acro_form.insert(CosName::new(b"Fields".to_vec()), CosObject::Array(fields_array.clone()));
    acro_form.insert(CosName::new(b"NeedAppearances".to_vec()), CosObject::Bool(true));
    acro_form.insert(
        CosName::new(b"DA".to_vec()),
        CosObject::String(b"/Helv 10 Tf 0 g".to_vec()),
    );

    let mut font = CosDictionary::new();
    font.insert(CosName::new(b"Type".to_vec()), CosObject::Name(CosName::new(b"Font".to_vec())));
    font.insert(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Type1".to_vec())));
    font.insert(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(b"Helvetica".to_vec())));
    let mut dr_font = CosDictionary::new();
    dr_font.insert(CosName::new(b"Helv".to_vec()), CosObject::Dictionary(font));
    let mut dr = CosDictionary::new();
    dr.insert(CosName::new(b"Font".to_vec()), CosObject::Dictionary(dr_font));
    acro_form.insert(CosName::new(b"DR".to_vec()), CosObject::Dictionary(dr));

    doc.insert_object(acro_form_id, CosObject::Dictionary(acro_form));
    doc.xref.insert_if_absent(acro_form_id, XRefEntry::InUse { offset: 0, generation: 0 });

    let catalog_id = doc.catalog_id().unwrap();
    doc.mutate_object(catalog_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.insert(CosName::new(b"AcroForm".to_vec()), CosObject::Reference(acro_form_id));
        }
    });

    doc.mutate_object(parent_page_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.insert(CosName::new(b"Annots".to_vec()), CosObject::Array(fields_array.clone()));
        }
    });

    ensure_parent_dir(output_path)?;
    doc.save(output_path)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.get(1).map(|s| s.as_str()) == Some("--all-modes") {
        let output_dir = args
            .get(2)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("examples_output/all_fields_modes"));

        for variant in VARIANTS {
            let output_path = output_dir.join(format!("{}.pdf", variant.file_stem));
            write_form(&output_path, *variant)?;
            println!("Generated {}", output_path.display());
        }

        println!("Done: generated {} mode files", VARIANTS.len());
        return Ok(());
    }

    let output_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("all_fields_form.pdf"));

    write_form(&output_path, VARIANTS[0])?;
    println!("Saved all-fields form to {}", output_path.display());

    Ok(())
}
