# rust-pdfbox

A comprehensive Rust PDF manipulation library, porting key capabilities from Apache PDFBox with idiomatic Rust design.

## Status

**1,133 tests — 100% passing ✅** — Overall ~95% completion across all modules

| Module | Tests | Progress | Status |
|--------|:-----:|:--------:|:------:|
| Parser | 159 | **95%** | 🟢 |
| Compress | 94 | **87%** | 🟢 |
| Crypto | 103 | **95%** | 🟢 |
| Forms (AcroForm) | 99 | **~85%** | 🟢 |
| Content Streams | 109 | **93%** | 🟢 |
| Signing | 67 | **87%** | 🟢 |
| Font | 61 | **82%** | 🟢 |
| PageOps | 73 | **82%** | 🟢 |
| COS Model | 56 | **96%** | 🟢 |
| Preflight (PDF/A) | 47 | **~80%** | 🟢 |
| Image Extract | 46 | **85%** | 🟢 |
| Metadata | 49 | **~80%** | 🟢 |
| Annotations | 36 | **88%** | 🟢 |
| Writer | 31 | **88%** | 🟢 |
| PdModel | 30 | **~82%** | 🟢 |
| Text Extraction | 25 | **82%** | 🟢 |
| Outline / Bookmarks | 23 | **~80%** | 🟢 |
| IO / Filters | 32 | **95%** | 🟢 |
| Security | 31 | **~85%** | 🟢 |
| Render | 12 | **90%** | 🟢 |
| CLI Tools | — | **0%** | ⬜ |

## Features

- **PDF Parsing:** Low-level support for parsing PDF documents, including objects, streams, cross-reference tables, and cross-reference streams (PDF 1.5+)
- **Incremental & Full-Rewrite Writing:** Support for creating incremental updates and full rewrites
- **Encryption & Decryption:** RC4, AES-128, AES-256 (Rev 2–6), password-protected PDFs
- **Digital Signatures:** Creating signature placeholders and embedding CMS/PAdES signatures
- **Text Extraction:** ToUnicode CMap, Type1, TrueType, Type0/CID font support with positional layout heuristics
- **Font Parsing:** FontDescriptor, Encoding, CMap, and font subsetting
- **Content Stream Editing:** Tokenizer, parser, serializer — find & replace text, XObject images, inline images
- **PdfEditor API:** High-level WASM-friendly interface for text, image, page, and form operations via `Vec<u8>` I/O
- **Interactive Forms (AcroForm):** Field creation, flatten, XFA, FDF/XFDF export/import
- **Annotations:** Markup, links, stamps, text notes
- **Page Operations:** Merge, split, rotate, extract, overlay, watermark
- **PDF Compression:** 8-pass pipeline — metadata stripping, stream re-compression, deduplication, image resampling, CMYK→sRGB conversion, font subsetting, version downgrade, linearization
- **Metadata:** DocInfo + XMP read/write/sync
- **Preflight:** PDF/A-1b validation with 14 rules
- **Rendering:** Per-glyph font outlines via `ab_glyph` with font program extraction
- **WASM-friendly:** `PdfEditor` API using `Vec<u8>` I/O (no `std::fs`)

## Usage

### Loading and inspecting a PDF

```rust,ignore
use rust_pdfbox::Document;

let doc = Document::load("input.pdf")?;
println!("Pages: {}", doc.page_count());
```

### Editing content streams

Use `PdfEditor` — a high-level WASM-friendly API for all content+page+form operations.

```rust,ignore
use rust_pdfbox::PdfEditor;

let mut editor = PdfEditor::load_from_bytes(&pdf_bytes)?;

// ── Text operations ──
editor.replace_text_on_page(0, "old text", "new text")?;
let results = editor.find_text_all_pages("search");
let text = editor.extract_text_from_page(0)?;

// ── Image operations ──
let images = editor.find_images_on_page(0)?;
editor.replace_image_xobject(0, "Im1", &new_data, 200, 100, "DeviceRGB", 8, None)?;
editor.rename_xobject_on_page(0, "Im1", "NewIm")?;

// ── Page operations ──
editor.merge_document(&other_doc)?;
editor.split(2)?; // split into 2-page chunks
let extracted = editor.extract_pages(&[0, 2])?;
editor.delete_page(1)?;
editor.reorder_pages(&[3, 0, 1, 2])?;
editor.rotate_page(0, 90)?;

// ── Form / AcroForm operations ──
editor.set_field_value("username", "new value")?;
let val = editor.get_field_value("username");
editor.flatten_fields()?;
let fdf = editor.export_fdf()?;
editor.import_fdf(&fdf_bytes)?;

// ── Save ──
let result = editor.save_to_bytes()?;
```

### Encrypting a PDF

```rust,ignore
use rust_pdfbox::Document;
use rust_pdfbox::protection::StandardProtectionPolicy;
use rust_pdfbox::crypto::Permissions;

let mut doc = Document::load("unencrypted.pdf")?;
let policy = StandardProtectionPolicy::new(
    "owner-password",
    "user-password",
    Permissions::all_allowed(),
);
doc.protect(&policy)?;
let mut file = std::fs::File::create("encrypted.pdf")?;
doc.save_encrypted(&mut file)?;
```

### Compressing a PDF

```rust,ignore
use rust_pdfbox::{Document, compress::{compress, CompressionMode}};

let mut doc = Document::load("large.pdf")?;
let report = compress(&mut doc, CompressionMode::Recommended.into())?;
println!("{}", report.summary());
```

### Merging pages

```rust,ignore
use rust_pdfbox::pageops::merge::PdfMerger;
use rust_pdfbox::Document;

let mut merger = PdfMerger::new();
merger.append(Document::load("a.pdf")?)?;
merger.append(Document::load("b.pdf")?)?;
let merged = merger.finish();
merged.save("merged.pdf")?;
```

### Extracting text with layout

```rust,ignore
use rust_pdfbox::text::extract_text_with_layout;
use rust_pdfbox::text::LayoutConfig;

let text = extract_text_with_layout(&doc, &LayoutConfig::default())?;
```

## Feature Flags

| Feature | Unlocks |
|---------|---------|
| `text` | Font + text extraction (on by default) |
| `crypto` | Encryption/decryption (on by default) |
| `layout` | Positional text layout heuristics (on by default) |
| `forms` | Interactive AcroForm support (on by default) |
| `pageops` | Merge, split, rotate, extract (on by default) |
| `outline` | Document outline/bookmarks (on by default) |
| `annotations` | Annotation support (on by default) |
| `image-extract` | Image extraction (on by default) |
| `metadata` | DocInfo + XMP (on by default) |
| `compress` | Compression pipeline (on by default) |
| `compress-images` | Image resampling + re-encoding |
| `compress-mozjpeg` | libjpeg-turbo encoder |
| `compress-color` | CMYK→sRGB via lcms2 |
| `compress-fonts` | Font subsetting via subsetter |
| `compress-full` | All compression features |

## Project Structure

```
src/
  cos/          — COS object model (Object, Name, Dictionary, Stream)
  parser/       — Lexer, parser, xref (table + stream), malformed recovery
  io/           — Stream filters (FlateDecode, ASCIIHex, ASCII85, RunLength, LZW)
  pdmodel/      — Page, page tree, resources, PDMetadata
  content/      — Stream tokenizer, operator, editor (PdfEditor API)
  font/         — CMap, descriptor, encoding, simple fonts, Type0/CID
  text/         — Text extraction + layout heuristics
  writer/       — Full-rewrite + incremental append writer
  crypto/       — RC4, AES, MD5, permissions, security handler
  render/       — Glyph outline rendering via ab_glyph
  forms/        — AcroForm fields, widget, flatten, XFA, FDF/XFDF
  annotations/  — Markup, link, stamp, text annotations
  pageops/      — Merge, split, rotate, extract, overlay, watermark
  outline/      — Document outline / bookmarks
  image_extract/ — Extract embedded images to PNG/JPEG/TIFF
  preflight/    — PDF/A-1b validation
  metadata/     — DocInfo + XMP metadata
  compress/     — 8-pass compression pipeline
  signing/      — Digital signatures (CMS/PAdES)
```
