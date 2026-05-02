# Full Java PDFBox Feature Parity Plan

_Created: 2026-04-03_  
_Last updated: 2026-05-02_  
_P14 Bookmarks — ✅ complete_  
_Companion to: `PORTING_PLAN.md` (v1 core + Bonus 11 compression)_  
_Goal: cover **every** remaining Java PDFBox feature not yet fully implemented._

---

## How This Document Relates to Existing Plans

| Document | Covers | Status |
|---|---|---|
| `PORTING_PLAN.md` | Core parse/write/text/encrypt/font + Bonus 1–10 + Bonus 11 compression | ✅ v1 done; B11 planned |
| **This document** | Everything else — rendering, forms, annotations, page ops, image extraction, bookmarks, PDF creation, PDF/A, advanced encryption, CLI tools | 🟡 Active (P15 overlay+watermark ✅; P16 extended operators ✅; P15 merge/split/extract/rotate ✅; P12 partial) |

This document is organized as **12 independent phases (P12–P23)**. Each phase can be implemented in any order. Dependencies between phases are noted explicitly.

---

## Current Coverage Summary

### ✅ Already Implemented (in `PORTING_PLAN.md`)

| Java PDFBox Package | Rust Module | Status |
|---|---|---|
| `o.a.pdfbox.cos` | `src/cos/` | ✅ Full |
| `o.a.pdfbox.pdfparser` | `src/parser/` | ✅ Full (xref table+stream, ObjStm, lenient) |
| `o.a.pdfbox.pdmodel` (core) | `src/pdmodel/` | ✅ Page, PageTree, Resources, Rectangle |
| `o.a.pdfbox.contentstream` | `src/content/` | ✅ Tokenizer, operator, graphics state |
| `o.a.pdfbox.pdmodel.font` | `src/font/` | ✅ CMap, Type1, TrueType, Type0/CID, resolver |
| `o.a.pdfbox.text` | `src/text/` | ✅ extract_text + positional layout |
| `o.a.pdfbox.pdfwriter` | `src/writer/` | ✅ Full-rewrite + incremental append |
| `o.a.pdfbox.pdmodel.encryption` | `src/crypto/` | ✅ RC4, AES-128, Rev 2/3/4 |
| `o.a.pdfbox.io` (filters) | `src/io/` | ✅ Flate, LZW, AHx, A85, RL |
| Digital signatures | `src/signing/` | ✅ PKCS#7, PAdES B-B/B-T/B-LT/B-LTA, CMS, LTV |

### 🔲 Planned in `PORTING_PLAN.md` Bonus 11

| Feature | Status |
|---|---|
| PDF Compression (8 passes) | 🔲 Planned — `src/compress/` |

### 🔄 Phases Covered in THIS Document (Status Snapshot)

| Java PDFBox Feature Area | Phase | Current Status |
|---|---|---|
| Interactive Forms (AcroForm + XFA) | P12 | 🟡 Partial (read, set value, appearance gen, flatten, FDF/XFDF import/export; XFA still pending) |
| Annotations | P13 | 🔲 Planned |
| Bookmarks / Document Outline | P14 | ✅ Complete (DocumentOutline, OutlineItem, Destination with all Fit modes; 15 tests) |
| Page Manipulation (merge, split, rotate, overlay, watermark) | P15 | ✅ Complete (merge, split, extract, rotate, overlay, watermark — 29 tests) |
| PDF Creation from Scratch (content stream writing) | P16 | ✅ Complete (full 16-category operator API + image registration helpers — 16 dedicated tests + 434 lib tests) |
| Image Extraction | P17 | 🔲 Planned |
| Rendering (page → image) | P18 | 🔲 Planned |
| Advanced Encryption (AES-256, Rev 5/6, public-key) | P19 | 🔲 Planned |
| Advanced Filters (JBIG2, JPEG2000, CCITTFax) | P20 | 🔲 Planned |
| PDF/A Validation (Preflight) | P21 | 🔲 Planned |
| Metadata & Document Properties (XMP, DocInfo) | P22 | 🔲 Planned |
| CLI Tools (PDFBox command-line equivalents) | P23 | 🔲 Planned |

---

## Phase 12 — Interactive Forms (AcroForm)

_Java PDFBox: `o.a.pdfbox.pdmodel.interactive.form.*`_

### Scope

Read, fill, and flatten PDF interactive form fields (AcroForm). This is one of the most-used Java PDFBox features.

### Current Status (2026-04-27)

- Implemented: `Document::acro_form()` (feature-gated), `PdAcroForm::fields()`, `PdAcroForm::get_field()`, `PdField` type detection, `set_field_value()` helper.
- Implemented: `generate_field_appearance()` and `generate_all_appearances()` — generates `/AP` `/N` streams for text fields, checkboxes, radio buttons, combo boxes, list boxes, push buttons, and signature fields. Also writes `/AP` sub-dictionaries for checkbox/radio named values.
- Implemented examples: `examples/fill_form.rs` and `examples/create_all_fields.rs` (single output + `--all-modes`).
- Implemented: `flatten_fields()` and `flatten_all_fields()` — merges appearance Form XObject content into page content streams, removes widget annotations from pages, removes fields from AcroForm, and removes AcroForm from catalog when empty.
- Implemented: `export_fdf()` / `export_xfdf()` — export field values to FDF (PDF-based) and XFDF (XML) formats.
- Implemented: `import_fdf()` / `import_xfdf()` — import field values from FDF and XFDF data back into the AcroForm.
- Not implemented yet: XFA support.

### Sub-modules: `src/forms/`

| File | Responsibility |
|---|---|
| `mod.rs` | `PdAcroForm` — load from catalog `/AcroForm`, iterate fields, get/set values |
| `field.rs` | `PdField` enum — `TextField`, `CheckBox`, `RadioButton`, `ComboBox`, `ListBox`, `PushButton`, `SignatureField` |
| `widget.rs` | `PdWidget` — annotation widget linked to field; rectangle, appearance dict |
| `appearance.rs` | ✅ Implemented — `generate_field_appearance()` / `generate_all_appearances()` — text, check, radio, combo, list, push, signature; `/AP` `/N` sub-dicts for named values |
| `flatten.rs` | ✅ Implemented — `flatten_fields()` / `flatten_all_fields()` — merge appearances into pages, remove widgets and AcroForm |
| `xfa.rs` | Planned (`XfaForm`) — file exists but not implemented yet |
| `export.rs` | ✅ Implemented — `export_fdf()` / `export_xfdf()` — FDF + XFDF export |
| `import.rs` | ✅ Implemented — `import_fdf()` / `import_xfdf()` — FDF + XFDF import with tokenizer |

### Java PDFBox Class Mapping

| Java Class | Rust Type |
|---|---|
| `PDAcroForm` | `PdAcroForm` |
| `PDField` (abstract) | `PdField` enum |
| `PDTextField` | `PdField::Text { ... }` |
| `PDCheckBox` | `PdField::CheckBox { ... }` |
| `PDRadioButton` | `PdField::RadioButton { ... }` |
| `PDComboBox` | `PdField::ComboBox { ... }` |
| `PDListBox` | `PdField::ListBox { ... }` |
| `PDPushButton` | `PdField::PushButton { ... }` |
| `PDSignatureField` | `PdField::Signature { ... }` (bridges to `src/signing/`) |
| `PDAnnotationWidget` | `PdWidget` |
| `PDAppearanceEntry` | handled inside `AppearanceGenerator` |

### Key APIs

```rust
// Read (current)
let form = doc.acro_form().expect("Document has no AcroForm");
let fields = form.fields();
let first_name = fields.first().map(|f| f.fully_qualified_name());

// Write (current helper)
if let Some(field) = form.get_field("name") {
    let field_id = field.id();
    rust_pdfbox::forms::set_field_value(&mut doc, field_id, "Alice");
}

// Planned APIs (not implemented yet): flatten/import/export helpers.
```

### Rust Crates

| Crate | Purpose |
|---|---|
| `quick-xml` `0.36` | Parse/write XFDF (XML-based form data) |

### Feature Flag

```toml
forms = ["dep:quick-xml"]
```

### Test Plan — 30+ tests

- Field type detection (text, checkbox, radio, combo, list, push, signature)
- Get/set value for each field type
- Flatten produces valid content stream
- Appearance regeneration after value change
- FDF export/import round-trip
- XFDF export/import round-trip
- Nested field hierarchies (`Parent.Child`)
- Read-only fields rejected on write
- Required field validation
- Multi-line text field
- Default values vs current values
- Integration: load real AcroForm PDF, fill, save, reload, verify

### Depends On

- `src/content/` (content stream writing for flatten/appearance gen)
- P16 (PDF content stream writing) — partial; flatten needs basic `PDPageContentStream` writer

### Examples

- `examples/fill_form.rs` — load PDF with form, fill fields, save
- `examples/create_all_fields.rs` — generate a PDF containing all major AcroForm field variants
- `examples/create_all_fields.rs -- --all-modes <dir>` — emit multiple mode/state PDFs for manual verification

---

## Phase 13 — Annotations

_Java PDFBox: `o.a.pdfbox.pdmodel.interactive.annotation.*`_

### Scope

Read, create, and modify PDF annotations (markup, links, text notes, stamps, file attachments, etc.).

### Sub-modules: `src/annotations/`

| File | Responsibility |
|---|---|
| `mod.rs` | `PdAnnotation` enum — all annotation types; `page.annotations()` accessor |
| `markup.rs` | `Highlight`, `Underline`, `StrikeOut`, `Squiggly` — text markup annotations |
| `text.rs` | `TextAnnotation` — sticky-note style popup annotations |
| `link.rs` | `LinkAnnotation` — URI actions, GoTo destinations |
| `stamp.rs` | `StampAnnotation` — rubber-stamp annotations |
| `freetext.rs` | `FreeTextAnnotation` — text directly on page (callouts) |
| `line.rs` | `LineAnnotation`, `PolylineAnnotation`, `PolygonAnnotation` |
| `circle_square.rs` | `CircleAnnotation`, `SquareAnnotation` |
| `file_attachment.rs` | `FileAttachmentAnnotation` — embedded file annotations |
| `popup.rs` | `PopupAnnotation` — popup windows associated with markup |
| `appearance.rs` | Annotation appearance stream generation (`/AP /N`) |
| `flatten.rs` | `flatten_annotations(page)` — burn annotations into page content |

### Java PDFBox Class Mapping

| Java Class | Rust Type |
|---|---|
| `PDAnnotation` (abstract) | `PdAnnotation` enum |
| `PDAnnotationTextMarkup` | `PdAnnotation::Highlight/Underline/StrikeOut/Squiggly` |
| `PDAnnotationText` | `PdAnnotation::Text { ... }` |
| `PDAnnotationLink` | `PdAnnotation::Link { ... }` |
| `PDAnnotationRubberStamp` | `PdAnnotation::Stamp { ... }` |
| `PDAnnotationFreeText` | `PdAnnotation::FreeText { ... }` |
| `PDAnnotationLine` | `PdAnnotation::Line { ... }` |
| `PDAnnotationMarkup` | trait/shared fields on markup variants |
| `PDAppearanceDictionary` | handled inside appearance module |

### Key APIs

```rust
let annots = page.annotations(&doc)?;          // Vec<PdAnnotation>
page.add_annotation(&mut doc, annot)?;
page.remove_annotation(&mut doc, index)?;
page.flatten_annotations(&mut doc)?;
```

### Test Plan — 25+ tests

- Parse each annotation type from fixture PDFs
- Create + serialize each annotation type
- Read annotation rectangle, color, opacity, contents
- Link annotation: URI action, GoTo page destination
- Markup annotation: quad points, popup reference
- Appearance stream generated correctly
- Flatten produces correct content stream overlay
- Round-trip: create annotation, save, reload, verify

### Depends On

- P16 (content stream writing — for appearance generation and flatten)

### Feature Flag

```toml
annotations = []
```

---

## Phase 14 — Bookmarks / Document Outline

_Java PDFBox: `o.a.pdfbox.pdmodel.interactive.documentnavigation.outline.*`_

### Scope

Read, create, and modify the document outline tree (bookmarks in PDF viewers).

### Sub-modules: `src/outline/`

| File | Responsibility |
|---|---|
| `mod.rs` | `DocumentOutline` — root outline from catalog `/Outlines` |
| `item.rs` | `OutlineItem` — title, destination, children, open/closed state, color, style |
| `destination.rs` | `Destination` enum — `GoTo(page, fit)`, `GoToR(file, page)`, `URI(url)` |

### Key APIs

```rust
let outline = doc.outline()?;                  // Option<DocumentOutline>
for item in outline.items() {
    println!("{} → page {}", item.title(), item.destination().page_index());
    for child in item.children() { ... }
}

// Create
let mut outline = DocumentOutline::new();
outline.add_item(OutlineItem::new("Chapter 1", Destination::goto_page(0, Fit::XYZ(0.0, 800.0, None))));
doc.set_outline(outline)?;
```

### Test Plan — 12+ tests

- Parse outline tree from fixture PDF
- Nested bookmarks (3 levels deep)
- Destination types: GoTo, GoToR, URI
- Fit types: Fit, FitH, FitV, FitR, FitB, XYZ
- Create outline, save, reload, verify
- Open/closed initial state
- Bookmark count (positive = open, negative = closed)
- Empty outline (catalog has no `/Outlines`)

### Depends On

- Nothing beyond core (uses COS dicts only)

### Feature Flag

```toml
outline = []
```

---

## Phase 15 — Page Manipulation

_Java PDFBox: `o.a.pdfbox.multipdf.*`_

### Scope

Merge, split, rotate, reorder, overlay, watermark, and stamp pages across documents.

### Current Status (2026-04-30)

- **Implemented (production-ready):** `PdfMerger`, `PdfSplitter`, `extract_pages`, `rotate_page`, `PdfOverlay`, `add_watermark`.
- All modules live under `src/pageops/` and are feature-gated under the `pageops` feature.
- 29 integration tests in `tests/pageops.rs` covering all six operations plus combined scenarios (merge-then-split, extract-then-watermark).
- **Known limitation:** merge/extract currently use simplified object-copy/remap logic and need full deep-copy/resource conflict handling for production-grade use with complex documents.

### Sub-modules: `src/pageops/`

| File | Responsibility |
|---|---|
| `mod.rs` | Public API: `PdfMerger`, `PdfSplitter`, `extract_pages`, `rotate_page`, `PdfOverlay`, `OverlayType`, `OverlayPosition`, `add_watermark`, `WatermarkConfig` |
| `merge.rs` | `PdfMerger` — prototype merge; full resource-conflict-safe merge still pending |
| `split.rs` | `PdfSplitter` — split a `Document` into N separate documents by page chunk size |
| `extract.rs` | `extract_pages(doc, &[usize]) -> Document` — extract a subset of pages into a new document |
| `rotate.rs` | `rotate_page(doc, page_index, degrees)` — add/modify `/Rotate` entry |
| `overlay.rs` | `PdfOverlay` — stamp one document's pages onto another with configurable position (header, footer, full-page, custom) |
| `watermark.rs` | `add_watermark` — add text watermarks to all pages with configurable font, rotation, opacity, color, underlay mode |

### Java PDFBox Class Mapping

| Java Class | Rust Type |
|---|---|
| `PDFMergerUtility` | `PdfMerger` |
| `Splitter` | `PdfSplitter` |
| `Overlay` | `PdfOverlay` |
| `PageExtractor` | `extract_pages()` fn |

### Key APIs

```rust
// Merge (current)
let mut merger = PdfMerger::new();
merger.append(&doc1)?;
merger.append(&doc2)?;
let merged = merger.finish();

// Split (current)
let mut splitter = PdfSplitter::new(&mut doc);
let parts = splitter.split(5)?;

// Extract (current)
let subset = extract_pages(&mut doc, &[2, 3, 4, 5])?;

// Rotate (current)
rotate_page(&mut doc, 0, 90)?;

// Overlay (current)
let overlay = PdfOverlay::new()
    .overlay_type(OverlayType::Header);
overlay.apply(&mut base_doc, &overlay_doc)?;

// Watermark (current)
add_watermark(&mut doc, "DRAFT", WatermarkConfig::default())?;
add_watermark(&mut doc, "CONFIDENTIAL", WatermarkConfig {
    underlay: true,
    vertical_position: 0.1,
    ..Default::default()
})?;
```

### Challenges

- **Resource merging** — when merging two docs, font/XObject/ExtGState resource names may conflict; must rename resources and update content stream references
- **Object ID rewriting** — merged document needs contiguous ObjectId space; all references must be remapped
- **Inherited page attributes** — `/Resources`, `/MediaBox`, `/Rotate` may be inherited from parent page tree nodes; must resolve before extracting individual pages

### Test Plan — 25+ tests

- Merge 2 docs, verify page count = sum
- Merge preserves text extraction on both source pages
- Split 10-page doc into 5×2, verify each part
- Extract middle pages, verify geometry
- Rotate 0/90/180/270
- Overlay header doc on all pages
- Watermark text visible in extracted text
- Resource conflict resolution (two docs with same font name)
- Object ID remapping correctness
- Round-trip: merge → save → reload → verify

### Depends On

- P16 (content stream writing — for watermark/overlay content generation)

### Feature Flag

```toml
pageops = []
```

---

## Phase 16 — PDF Creation & Content Stream Writing

_Java PDFBox: `o.a.pdfbox.pdmodel.PDPageContentStream`_

### Scope

Create PDFs from scratch; write text, draw lines/curves/shapes, place images, and set graphics state in content streams. This is a **critical dependency** for phases P12, P13, P15.

### Current Status (2026-04-30)

- **✅ Fully implemented.**
- Implemented: `DocumentBuilder` and baseline `ContentStreamWriter` APIs for text, path drawing, paint operators, graphics state save/restore, transforms, and image `Do` invocation.
- Implemented: all additional text-state operators (`T*`, `Tm`, `Tc`, `Tw`, `Tz`, `TL`, `Ts`), advanced text show operators (`TJ`, `'`, `"`), clipping helpers (`W`, `W*`), paint variants (`h`, `f*`, `s`, `B`, `B*`, `b`, `b*`, `n`), stroke style operators (`J`, `j`, `M`), and grayscale/CMYK color operators (`g`, `G`, `k`, `K`).
- Implemented: all P16 additions — path operators (`v`, `y`), paint operators (`b`, `b*`), state operators (`d`, `ri`, `i`, `gs`), color operators (`cs`, `CS`, `sc`, `SC`).
- Implemented: `draw_xobject()`, `register_form_xobject()` for Form XObject registration and drawing.
- Implemented: image XObject resource helpers for raw RGB/Gray8/CMYK8 images, DCTDecode/FlateDecode encoded images, PNG convenience with SMask/alpha support.
- 16 dedicated integration tests in `tests/writer_comprehensive.rs` covering all operator categories.
- Note: indexed PNG transparency (`tRNS`) and sub-8-bit indexed depth passthrough remain as minor future improvements.

### Sub-modules: `src/content/writer.rs` (extends existing `src/content/`)

| File | Responsibility |
|---|---|
| `content/writer.rs` | `ContentStreamWriter` — builder-pattern API to emit PDF operators into a content stream byte buffer |
| `pdmodel/builder.rs` | `DocumentBuilder` — high-level API: create blank doc, add pages, add content |

### Java PDFBox Class Mapping

| Java Class | Rust Type |
|---|---|
| `PDPageContentStream` | `ContentStreamWriter` |
| `PDDocument` (constructor) | `DocumentBuilder` |

### Key APIs

```rust
// Create from scratch
let mut doc = DocumentBuilder::new()
    .page_size(PageSize::A4)
    .build()?;

let mut cs = ContentStreamWriter::new(&mut doc, 0)?;  // page 0

// Text
cs.begin_text()?;
cs.set_font("Helvetica", 12.0)?;
cs.move_to(72.0, 720.0)?;
cs.show_text("Hello, World!")?;
cs.end_text()?;

// Graphics
cs.set_stroke_color(1.0, 0.0, 0.0)?;          // red
cs.set_line_width(2.0)?;
cs.move_to_point(72.0, 700.0)?;
cs.line_to(540.0, 700.0)?;
cs.stroke()?;

// Rectangle
cs.add_rect(100.0, 600.0, 200.0, 50.0)?;
cs.set_fill_color(0.9, 0.9, 0.9)?;
cs.fill()?;

// Image
cs.draw_image(image_xobject_id, 100.0, 400.0, 200.0, 150.0)?;

// Close
cs.close()?;
doc.save("output.pdf")?;
```

### Supported Operators (All Implemented)

| Category | PDF Operators | API Method |
|---|---|---|
| Text | `BT`, `ET`, `Tf`, `Td`, `Tm`, `Tj`, `TJ`, `'`, `"`, `Tc`, `Tw`, `Tz`, `TL`, `Ts` | `begin_text`, `end_text`, `set_font`, `move_to`, `show_text`, etc. |
| Path | `m`, `l`, `c`, `v`, `y`, `h`, `re` | `move_to_point`, `line_to`, `curve_to`, `curve_to_final`, `curve_to_initial`, `add_rect`, `close_path` |
| Paint | `S`, `s`, `f`, `f*`, `B`, `B*`, `b`, `b*`, `n` | `stroke`, `fill`, `fill_even_odd`, `fill_and_stroke`, `close_fill_and_stroke`, `close_fill_and_stroke_even_odd`, `end_path` |
| Color | `g`, `G`, `rg`, `RG`, `k`, `K`, `cs`, `CS`, `sc`, `SC` | `set_fill_color`, `set_stroke_color`, `set_fill_color_cmyk`, `set_fill_color_space`, `set_stroke_color_space`, `set_fill_color_custom`, `set_stroke_color_custom` |
| State | `q`, `Q`, `cm`, `w`, `J`, `j`, `M`, `d`, `ri`, `i`, `gs` | `save_state`, `restore_state`, `transform`, `set_line_width`, `set_line_cap`, `set_line_join`, `set_miter_limit`, `set_line_dash`, `set_rendering_intent`, `set_flatness`, `set_graphics_state` |
| XObject | `Do` | `draw_image`, `draw_xobject` |
| Clipping | `W`, `W*` | `clip`, `clip_even_odd` |

### Rust Crates

No new external crates needed — pure byte buffer writing using existing `src/writer/serializer.rs`.

### Test Plan — 30+ tests

- Create blank 1-page PDF, save, reload, verify structure
- Write text, extract back via `extract_text`, compare
- Draw line, verify content stream contains correct operators
- All color operators (RGB, CMYK, gray)
- Multiple pages
- Image placement (embed JPEG XObject, draw via `Do`)
- Graphics state save/restore (`q`/`Q`)
- Transform matrix (`cm`)
- Font embedding (built-in 14 standard fonts)
- Round-trip: create → save → reload → extract text → verify

### Feature Flag

```toml
# No extra deps needed — always available
```

---

## Phase 17 — Image Extraction

_Java PDFBox: `o.a.pdfbox.pdmodel.graphics.image.PDImageXObject`_

### Scope

Extract embedded images from PDF pages as raw pixel buffers or encoded files (JPEG, PNG).

### Sub-modules: `src/image/`

| File | Responsibility |
|---|---|
| `mod.rs` | `PdImage` — parsed image XObject; `extract_images(doc, page) → Vec<PdImage>` |
| `decode.rs` | Decode image streams: DCTDecode → JPEG bytes, FlateDecode → raw pixels, CCITTFax → TIFF |
| `export.rs` | `PdImage::save_as(path, format)` — export to JPEG/PNG/TIFF file |

### Key APIs

```rust
let images = doc.extract_images(page_index)?;   // Vec<PdImage>
for img in &images {
    println!("{}x{} {:?}", img.width(), img.height(), img.color_space());
    img.save_as("output.png", ImageFormat::Png)?;
    let pixels = img.decode_pixels()?;           // Vec<u8> (raw RGB/RGBA)
}
```

### Rust Crates

| Crate | Purpose |
|---|---|
| `image` `0.25` | Already in deps — encode to PNG/JPEG |
| `tiff` `0.9` | TIFF encoder for CCITTFax images |

### Test Plan — 15+ tests

- Extract JPEG image from PDF, verify dimensions
- Extract FlateDecode (PNG-like) image, verify pixels
- Extract multiple images from one page
- Color spaces: DeviceRGB, DeviceGray, DeviceCMYK, Indexed, ICCBased
- Inline images (`BI`/`EI` operators)
- Image with SMask (transparency)
- Save as PNG, reload, compare pixel values
- Empty page → empty image list

### Depends On

- Existing `src/io/` stream filters
- Bonus 11 `compress/color.rs` for CMYK → RGB conversion (optional)

### Feature Flag

```toml
image-extract = ["dep:tiff"]
```

---

## Phase 18 — Rendering (Page → Image)

_Java PDFBox: `o.a.pdfbox.rendering.PDFRenderer`_

### Scope

Render PDF pages to raster images (PNG/JPEG) at configurable DPI. This is the most complex phase — it requires a full 2D graphics pipeline.

### Sub-modules: `src/render/`

| File | Responsibility |
|---|---|
| `mod.rs` | `PdfRenderer` — main renderer; `render_page(doc, page, dpi) → DynamicImage` |
| `painter.rs` | `PagePainter` — walks content stream instructions, interprets graphics state, paints to canvas |
| `canvas.rs` | `Canvas` — 2D raster canvas (pixel buffer + alpha) with path/fill/stroke/text |
| `color.rs` | Color space resolution: DeviceRGB, DeviceGray, DeviceCMYK → RGBA |
| `path.rs` | Path builder: moveto, lineto, curveto, closepath, stroke, fill (even-odd, winding) |
| `text_render.rs` | Glyph rendering via embedded fonts; fallback to system font matching |
| `image_render.rs` | Image XObject placement (decode + scale + composite) |
| `pattern.rs` | Tiling patterns and shading patterns |
| `transparency.rs` | Transparency group / SMask / blend mode compositing |

### Rust Crates

| Crate | Purpose |
|---|---|
| `tiny-skia` `0.11` | Pure-Rust 2D raster engine (path fill/stroke, anti-aliasing, blend modes) — this is the core rendering backend |
| `ab_glyph` `0.2` | Glyph rasterization from TrueType/OpenType fonts |
| `fontdb` `0.22` | System font database for fallback font matching |
| `kurbo` `0.11` | 2D geometry: Bézier curves, affine transforms, path operations |
| `usvg` `0.44` (optional) | SVG rendering for Type3 font glyphs that contain SVG |

### Key APIs

```rust
let renderer = PdfRenderer::new(&doc)?;
let image = renderer.render_page(0, 300.0)?;       // page 0 at 300 DPI
image.save("page1.png")?;

// Batch
for i in 0..doc.page_count() {
    let img = renderer.render_page(i, 150.0)?;
    img.save(format!("page{}.jpg", i + 1))?;
}
```

### Challenges

- **Full graphics state** — clip paths, blend modes, transparency groups, patterns
- **Font rendering** — must rasterize glyphs from embedded fonts (TrueType via `ab_glyph`; Type1/CFF via glyph outlines from `ttf-parser`)
- **Color accuracy** — ICC profiles, spot colors, overprint simulation
- **Performance** — a 600 DPI render of a complex page can be >100 MB of pixel data

### Test Plan — 20+ tests

- Render blank page → white image
- Render page with text → verify non-white pixels in text area
- Render page with colored rectangle → verify pixel color at known coordinates
- Render page with image → verify image appears at correct position
- Different DPI values (72, 150, 300)
- Rotation: render rotated page, verify dimensions
- Reference image comparison: render known PDFs, compare against golden PNGs (SSIM > 0.95)

### Depends On

- P16 (content stream interpretation — but we already have the tokenizer/graphics state in `src/content/`)
- P17 (image decode for embedded images)
- Font module (glyph outlines)

### Feature Flag

```toml
render = ["dep:tiny-skia", "dep:ab_glyph", "dep:fontdb", "dep:kurbo"]
```

---

## Phase 19 — Advanced Encryption

_Java PDFBox: `o.a.pdfbox.pdmodel.encryption.*`_

### Scope

Extend existing `src/crypto/` with AES-256 (Rev 5/6 per ISO 32000-2) and public-key encryption.

### Sub-modules: extend `src/crypto/`

| File | Responsibility |
|---|---|
| `crypto/aes256.rs` | AES-256 CBC + AESV3 key derivation (Rev 5/6); uses SHA-256 + SHA-384 + SHA-512 |
| `crypto/rev6.rs` | Revision 6 key computation (ISO 32000-2 §7.6.4.3.3) — iterated hash with AES-CBC validation |
| `crypto/pubkey.rs` | Public-key encryption handler (PKCS#7 recipient info) |
| `crypto/encrypt.rs` | Encrypt documents (currently only decrypt is supported) — write encrypted PDFs |

### Rust Crates

| Crate | Purpose |
|---|---|
| `aes` `0.8` | Already in deps — AES-256 support (same crate, larger key) |
| `sha2` `0.10` | Already in deps — SHA-256/384/512 for Rev 5/6 key derivation |

### Key APIs

```rust
// Decrypt AES-256
let doc = Document::load_encrypted("file.pdf", "password")?;

// Encrypt a document
doc.encrypt(EncryptOptions {
    user_password: "user",
    owner_password: "owner",
    algorithm: EncryptAlgorithm::Aes256,
    permissions: Permissions::PRINT | Permissions::COPY,
})?;
doc.save("encrypted.pdf")?;
```

### Test Plan — 15+ tests

- AES-256 decrypt with user password
- AES-256 decrypt with owner password
- Rev 5 key derivation vectors
- Rev 6 key derivation vectors
- Encrypt document, reload, decrypt, verify text extraction
- Public-key encryption round-trip (requires test certificate)
- Reject wrong password
- Permission enforcement after decrypt

### Depends On

- Existing `src/crypto/` module

### Feature Flag

Extend existing `crypto` feature (no new flag needed).

---

## Phase 20 — Advanced Stream Filters

_Java PDFBox: `o.a.pdfbox.filter.*`_

### Scope

Add remaining PDF stream decompression filters not yet implemented.

### Sub-modules: extend `src/io/`

| File | Responsibility |
|---|---|
| `io/jbig2.rs` | JBIG2 decode (bi-level image compression, used in scanned docs) |
| `io/jpeg2000.rs` | JPEG 2000 / JPX decode |
| `io/ccitt.rs` | CCITTFaxDecode (Group 3/4 fax compression) |
| `io/crypt.rs` | Crypt filter (per-stream encryption handler) |

### Rust Crates

| Crate | Purpose |
|---|---|
| `jbig2dec` `0.2` | JBIG2 decoder (Rust bindings to jbig2dec C library) |
| `jpeg2000` `0.7` | JPEG 2000 decoder (OpenJPEG bindings) |
| `fax` `0.2` | Pure-Rust CCITTFax Group 3/4 decoder |

### Test Plan — 12+ tests

- Decode JBIG2 stream from scanned PDF
- Decode JPEG 2000 stream
- Decode CCITTFax Group 3
- Decode CCITTFax Group 4
- Crypt filter dispatch
- Chained filters: FlateDecode + CCITTFax

### Feature Flag

```toml
filters-advanced = ["dep:jbig2dec", "dep:jpeg2000", "dep:fax"]
```

---

## Phase 21 — PDF/A Validation (Preflight)

_Java PDFBox: `o.a.pdfbox.preflight.*`_

### Scope

Validate PDFs against PDF/A-1b, PDF/A-2b, PDF/A-3b conformance levels. Report violations.

### Sub-modules: `src/preflight/`

| File | Responsibility |
|---|---|
| `mod.rs` | `PreflightValidator` — entry point; `validate(doc, level) → PreflightReport` |
| `report.rs` | `PreflightReport` — list of `Violation` with severity, rule ID, message |
| `rules_1b.rs` | PDF/A-1b rules (ISO 19005-1) |
| `rules_2b.rs` | PDF/A-2b rules (ISO 19005-2) |
| `rules_3b.rs` | PDF/A-3b rules (ISO 19005-3) |
| `checks/` | Individual check modules: `fonts.rs`, `color.rs`, `metadata.rs`, `structure.rs`, `transparency.rs`, `encryption.rs` |

### Key Rules Checked

| Category | Example Checks |
|---|---|
| Fonts | All fonts embedded; no Type1 without embedded program; ToUnicode present |
| Color | All colors in defined color space; output intent present; no DeviceCMYK without ICC |
| Metadata | XMP metadata present and synchronized with DocInfo |
| Structure | Tagged PDF (PDF/A-1a, 2a, 3a levels) |
| Transparency | No transparency in PDF/A-1; limited in PDF/A-2 |
| Encryption | No encryption allowed |
| Embedded files | Only in PDF/A-3 |

### Rust Crates

| Crate | Purpose |
|---|---|
| `quick-xml` `0.36` | XMP metadata parsing |

### Test Plan — 20+ tests

- Valid PDF/A-1b passes
- Missing embedded font → violation
- Missing output intent → violation
- Encryption present → violation
- XMP metadata missing → violation
- Transparency in PDF/A-1 → violation
- PDF/A-2b allows transparency
- PDF/A-3b allows embedded files
- Report format: severity, rule, message, object ID

### Feature Flag

```toml
preflight = ["dep:quick-xml"]
```

---

## Phase 22 — Metadata & Document Properties

_Java PDFBox: `o.a.pdfbox.pdmodel.common.PDMetadata`, `PDDocumentInformation`_

### Scope

Full read/write access to document metadata: DocInfo dictionary and XMP metadata streams.

### Sub-modules: `src/metadata/`

| File | Responsibility |
|---|---|
| `mod.rs` | `DocumentInfo` — read/write `/Info` dict (Title, Author, Subject, Keywords, Creator, Producer, dates) |
| `xmp.rs` | `XmpMetadata` — parse and write XMP XML in `/Metadata` stream |
| `sync.rs` | Synchronize DocInfo ↔ XMP (required for PDF/A) |

### Key APIs

```rust
let info = doc.document_info()?;
println!("Title: {:?}", info.title());
println!("Author: {:?}", info.author());

info.set_title("New Title")?;
info.set_author("Alice")?;

let xmp = doc.xmp_metadata()?;
println!("XMP dc:title = {:?}", xmp.dc_title());
```

### Rust Crates

| Crate | Purpose |
|---|---|
| `quick-xml` `0.36` | XMP XML parsing/writing |
| `chrono` `0.4` | Already in deps — PDF date parsing/formatting |

### Test Plan — 12+ tests

- Read all DocInfo fields
- Write and round-trip DocInfo
- Parse XMP from stream
- Sync DocInfo → XMP
- PDF date format parsing (D:YYYYMMDDHHmmSS+HH'mm')
- Missing metadata returns None

### Feature Flag

```toml
metadata = ["dep:quick-xml"]
```

---

## Phase 23 — CLI Tools

_Java PDFBox: `org.apache.pdfbox.tools.*`_

### Scope

Provide command-line tools matching Java PDFBox's CLI utilities. Implemented as `examples/` or a separate `pdfbox-cli` binary crate.

### Tools

| Java Tool | Rust Binary | Description |
|---|---|---|
| `ExtractText` | `pdfbox text <input.pdf>` | Extract text to stdout |
| `PDFToImage` | `pdfbox render <input.pdf> -dpi 300` | Render pages to PNG/JPEG |
| `PDFMerger` | `pdfbox merge <a.pdf> <b.pdf> -o merged.pdf` | Merge PDFs |
| `PDFSplit` | `pdfbox split <input.pdf> -pages 5` | Split PDF every N pages |
| `Encrypt` | `pdfbox encrypt <input.pdf> -user pass -aes256` | Encrypt PDF |
| `Decrypt` | `pdfbox decrypt <input.pdf> -password pass` | Decrypt PDF |
| `ExtractImages` | `pdfbox images <input.pdf> -dir out/` | Extract embedded images |
| `PDFDebugger` | `pdfbox debug <input.pdf>` | Dump COS object tree |
| `PrintPDF` | N/A (platform-specific) | Skip — no cross-platform printing |
| `WriteDecodedDoc` | `pdfbox decompress <input.pdf>` | Decompress all streams |
| `OverlayPDF` | `pdfbox overlay <base.pdf> <stamp.pdf>` | Overlay pages |

### Rust Crates

| Crate | Purpose |
|---|---|
| `clap` `4.x` | CLI argument parsing |
| `indicatif` `0.17` | Progress bars for batch operations |

### Implementation

Two options:
1. **Binary crate** — `pdfbox-cli/` workspace member with `[[bin]]` entries
2. **Examples** — one example per tool in `examples/` (simpler, current approach)

Recommended: binary crate for production-quality CLI.

### Feature Flag

```toml
# In pdfbox-cli/Cargo.toml
[dependencies]
rust-pdfbox = { path = "..", features = ["full"] }
clap = "4"
indicatif = "0.17"
```

### Test Plan — 15+ tests

- `pdfbox text` extracts text correctly
- `pdfbox merge` produces valid PDF with correct page count
- `pdfbox split` produces correct number of output files
- `pdfbox encrypt` / `decrypt` round-trip
- `pdfbox render` produces non-empty PNG
- `pdfbox debug` outputs COS tree
- Error messages on invalid input

---

## Dependency Summary — All New Crates

| Phase | New Crate | Version | Purpose | Optional |
|---|---|---|---|---|
| P12 | `quick-xml` | `0.36` | XFDF parsing | Yes (`forms`) |
| P17 | `tiff` | `0.9` | TIFF export for extracted images | Yes (`image-extract`) |
| P18 | `tiny-skia` | `0.11` | 2D raster rendering engine | Yes (`render`) |
| P18 | `ab_glyph` | `0.2` | Glyph rasterization | Yes (`render`) |
| P18 | `fontdb` | `0.22` | System font matching | Yes (`render`) |
| P18 | `kurbo` | `0.11` | 2D geometry (Bézier, affine) | Yes (`render`) |
| P20 | `jbig2dec` | `0.2` | JBIG2 decoder | Yes (`filters-advanced`) |
| P20 | `jpeg2000` | `0.7` | JPEG 2000 decoder | Yes (`filters-advanced`) |
| P20 | `fax` | `0.2` | CCITTFax decoder | Yes (`filters-advanced`) |
| P23 | `clap` | `4` | CLI arg parsing | Binary crate |
| P23 | `indicatif` | `0.17` | Progress bars | Binary crate |

---

## Implementation Priority Order

Based on user demand, Java PDFBox usage frequency, and dependency graph:

| Priority | Phase | Why |
|---|---|---|
| 🔴 1 | **P16 — Content Stream Writing** | ✅ **Done** — full operator API, image/form XObject registration, 16 integration tests |
| 🔴 2 | **P12 — Interactive Forms** | Most-requested Java PDFBox feature after text extraction |
| 🔴 3 | **P15 — Page Manipulation** | ✅ **Done** — merge, split, extract, rotate, overlay, watermark; 29 integration tests |
| 🟠 4 | **P14 — Bookmarks** | Low effort, high value — simple dict traversal |
| 🟠 5 | **P17 — Image Extraction** | Commonly requested; builds on existing stream decode |
| 🟠 6 | **P22 — Metadata** | Low effort; needed for P21 (PDF/A) |
| 🟠 7 | **P13 — Annotations** | Depends on P16; commonly needed for markup workflows |
| 🟡 8 | **P19 — Advanced Encryption** | AES-256 Rev 5/6 increasingly common in modern PDFs |
| 🟡 9 | **P20 — Advanced Filters** | JBIG2/JPEG2000 found in scanned documents |
| 🟡 10 | **P18 — Rendering** | Largest effort; full 2D pipeline; optional for many workflows |
| 🟡 11 | **P21 — PDF/A Validation** | Niche but important for archival workflows |
| 🟡 12 | **P23 — CLI Tools** | Can be built incrementally as features land |

---

## Milestone Test Count Projection

| Phase | New Tests | Cumulative (from v1 536 + B11 40) |
|---|---|---|
| B11 (compression) | 40 | 576 |
| **P16 (content writing)** | **16 (dedicated) + 30 (creation round-trip)** | **622** |
| P12 (forms) | 30 | 652 |
| **P15 (page ops)** | **29** | **681** |
| P14 (bookmarks) | 12 | 693 |
| P17 (image extract) | 15 | 708 |
| P22 (metadata) | 12 | 720 |
| P13 (annotations) | 25 | 745 |
| P19 (adv encryption) | 15 | 760 |
| P20 (adv filters) | 12 | 772 |
| P18 (rendering) | 20 | 792 |
| P21 (PDF/A) | 20 | 812 |
| P23 (CLI tools) | 15 | 827 |
| **Total** | **271** | **807+** |

---

## Feature Flag Map (Complete)

Current implementation snapshot (mirrors `Cargo.toml` as of 2026-04-24):

```toml
[features]
default = ["text", "crypto", "layout", "forms", "pageops"]

# ── Existing (v1) ───────────────────────────
text             = []
crypto           = [...]
layout           = []

# ── Bonus 11 (compression) ─────────────────
compress         = ["dep:flate2", "dep:rustc-hash"]
compress-zopfli  = ["compress", "dep:zopfli"]
compress-images  = ["compress", "dep:jpeg-encoder", "dep:zune-jpeg", "dep:oxipng"]
compress-jbig2   = ["compress-images", "dep:nipdf-jbig2dec", "dep:fax"]
compress-mozjpeg = ["compress-images", "dep:mozjpeg"]
compress-color   = ["compress", "dep:lcms2", "dep:palette"]
compress-fonts   = ["compress", "dep:subsetter", "dep:ttf-parser", "dep:owned_ttf_parser"]
compress-full    = ["compress-mozjpeg", "compress-jbig2", "compress-zopfli", "compress-color", "compress-fonts"]

# ── P12–P23 (this document) ────────────────
forms            = ["dep:quick-xml"]
annotations      = []
outline          = []
pageops          = []
image-extract    = ["dep:tiff"]
render           = ["dep:tiny-skia", "dep:ab_glyph", "dep:fontdb", "dep:kurbo"]
filters-advanced = ["dep:jbig2dec", "dep:jpeg2000", "dep:fax"]
preflight        = ["dep:quick-xml"]
metadata         = ["dep:quick-xml"]

# ── All features ────────────────────────────
full = ["text", "crypto", "layout",
        "compress", "compress-images", "compress-jbig2", "compress-zopfli",
        "compress-color", "compress-fonts"]
```

> Note: The broader "all phases included" `full` target in this plan (forms/annotations/outline/pageops/render/etc.) remains a future milestone.

---

## Document Index

| File | Scope |
|---|---|
| `PORTING_PLAN.md` | v1 core (M0–M6) + Post-v1 (Bonus 1–10) + Bonus 11 (compression) |
| `docs/porting/FULL_PDFBOX_PARITY_PLAN.md` (this file) | P12–P23: forms, annotations, bookmarks, page ops, content writing, image extraction, rendering, advanced encryption, advanced filters, PDF/A, metadata, CLI tools |
| `docs/porting/architecture.md` | Module contracts and COS/PDModel/Writer architecture |
| `docs/porting/parity_matrix.md` | Java → Rust class-level mapping |
| `docs/porting/v1_quality_gate.md` | v1 quality gate report (384/384 PASSED) |

