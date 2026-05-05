# rust-pdfbox

A pure-Rust port of [Apache PDFBox](https://pdfbox.apache.org/) — comprehensive PDF reading, writing, and digital signing.

> **Status:** v0.1.0 — core read/write pipeline complete, 536+ tests passing, post-v1 hardening done. Ongoing work toward full Java PDFBox feature parity.

---

## Highlights

- **Pure Rust** — no JVM dependency, minimal C footprint (optional features only)
- **Idiomatic API** — follows PDFBox's mental model (`Document`, `Page`, `CosObject`) but feels native to Rust
- **Feature-gated modules** — enable only what you need: `text`, `crypto`, `layout`
- **Robust parser** — handles malformed tokens, xref streams, object streams, lenient recovery
- **Digital signatures** — PKCS#7, PAdES B-B / B-T / B-LT / B-LTA with CRL, OCSP, DSS, and TSA support
- **Cross-validated** — test harness compares output against Java PDFBox reference snapshots

## Features

| Feature flag | What it enables |
|---|---|
| `text` *(default)* | Text extraction, font handling (CMap, Type1, TrueType, Type0/CID) |
| `crypto` *(default)* | Encryption / decryption (RC4, AES-CBC), permissions, StandardSecurityHandler |
| `layout` *(default)* | Advanced layout analysis — column detection, reading order heuristics |
| `full` | All of the above |

## Architecture

The crate mirrors the Java PDFBox package structure:

```
src/
  cos/        COS object model (dictionary, stream, name, object-id)
  parser/     Lexer, parser, xref (table + stream), object streams, malformed recovery
  pdmodel/    Page tree, page attributes, resources, rectangle
  content/    Content-stream tokenizer, operator model, graphics state
  font/       CMap, Type1, TrueType, Type0/CID font resolver
  text/       Text extraction with positional layout heuristics
  writer/     Full-rewrite serializer + incremental-append writer
  crypto/     RC4, AES-CBC, MD5, permissions, standard security handler
  signing/    CMS/PKCS#7 builder, PAdES profiles, CRL/OCSP/DSS/TSA embedding
  io/         Stream filters — FlateDecode, ASCIIHex, ASCII85, RunLength, LZW
  render/     (planned)
```

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rust-pdfbox = { path = "." }          # or publish / git reference
```

### Read PDF info

```rust
use rust_pdfbox::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = Document::load("sample.pdf")?;
    println!("Pages: {}", doc.page_count());
    println!("Objects: {}", doc.object_count());
    Ok(())
}
```

### Extract text

```rust
use rust_pdfbox::Document;
use rust_pdfbox::text::extract_text;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = Document::load("sample.pdf")?;
    for page in doc.pages()?.iter() {
        if let Some(stream) = page.contents_object().and_then(|o| o.as_stream()) {
            let text = extract_text(&stream.data, None);
            println!("{text}");
        }
    }
    Ok(())
}
```

### Digital signing

See [`examples/digital_sign.rs`](examples/digital_sign.rs) for a full example covering PKCS#7 and PAdES signing with visible/invisible signatures, CRL, OCSP, DSS, and TSA options.

## Examples

```bash
# Print PDF metadata and page sizes
cargo run --example read_info -- path/to/file.pdf

# Extract all text
cargo run --example extract_text -- path/to/file.pdf

# Digital signature (see source for key/cert setup)
cargo run --example digital_sign

# Inspect XFA packets (hybrid AcroForm/XFA PDFs)
cargo run --example read_xfa -- path/to/form.pdf

# Fill AcroForm field and inspect XFA packets in one run
cargo run --example fill_form_xfa_hybrid -- in.pdf out.pdf field_name "new value"
```

## Running Tests

```bash
# All tests (default features)
cargo test

# With all features
cargo test --all-features

# Parser regression suite only
cargo test --test parser_regression

# Cross-validation against Java PDFBox snapshots
cargo test --test cross_validate
```

## Benchmarks

```bash
cargo bench --bench bench_core
```

## Roadmap

The project is being ported phase-by-phase from Java PDFBox. The core pipeline (parse → model → text → write → crypto → sign) is complete. Upcoming work includes:

- **Forms** — AcroForm complete (fill/appearance/flatten/FDF/XFDF) + XFA read/inspect
- **Annotations** — markup, widget, link annotations
- **Bookmarks / Outlines** — document navigation tree
- **Page operations** — merge, split, rotate, crop
- **Content writing** — high-level page content stream builder
- **Image extraction** — inline + XObject image decode
- **Rendering** — page-to-image rasterisation
- **Advanced encryption** — AES-256, public-key security
- **Advanced filters** — JBIG2, JPX, CCITT
- **PDF/A validation** — preflight checks
- **XMP Metadata** — read/write XMP streams
- **CLI tools** — command-line utilities (like Java PDFBox CLI)

See [`docs/porting/FULL_PDFBOX_PARITY_PLAN.md`](docs/porting/FULL_PDFBOX_PARITY_PLAN.md) for the detailed phase breakdown.

## License

This project is an independent Rust implementation inspired by Apache PDFBox. It does not contain any Java source code from the Apache project.

---

*Built with 🦀 Rust — fast, safe, and zero-GC.*

