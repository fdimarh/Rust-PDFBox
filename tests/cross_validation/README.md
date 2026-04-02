# Cross-Validation Reference Snapshots

This directory contains reference JSON snapshots extracted from PDFs generated
by **Apache PDFBox 3.0.7** (Java). The Rust port is validated against these
snapshots in `tests/cross_validate.rs`.

## Snapshot Format

Each `.json` file corresponds to one PDF fixture in `tests/fixtures/`. Fields:

```json
{
  "file": "smoke/letter_single_page.pdf",
  "source": "java_pdfbox_3.0.7",
  "description": "Single US Letter page (612x792 pt), no content",
  "pdf_version": "1.6",
  "page_count": 1,
  "pages": [
    {
      "index": 0,
      "width": 612.0,
      "height": 792.0,
      "rotation": 0,
      "text_len_min": 0,
      "text_len_max": 9999,
      "text_contains": []
    }
  ],
  "permissions": {
    "print": true,
    "copy": true,
    "modify": true,
    "annotate": true
  },
  "fonts": [],
  "metadata": {
    "title": null,
    "author": null
  }
}
```

## Fixture Tiers (23 files)

| Tier        | Count | Description                                    |
|-------------|------:|------------------------------------------------|
| `smoke`     |    11 | Basic valid PDFs — page sizes, rotation, round-trip |
| `font_heavy`|     3 | Content streams with text operators            |
| `encrypted` |     3 | Permission flag combinations (128-bit RC4)     |
| `large`     |     3 | 50 / 100 / 200 page scalability checks         |
| `malformed` |     3 | Missing header, empty bytes, broken xref       |

## Tolerances

- Page dimensions: ±0.5 pt
- Text length: within declared `[text_len_min, text_len_max]` range
- Text contains: all strings in `text_contains[]` must appear
- Permissions: exact match

## Regenerating

Requires Java 11+ and `pdfbox-app-3.0.7.jar` at the project root.

### Step 1 — Generate PDF fixtures

```bash
cd rust-pdfbox
javac -cp pdfbox-app-3.0.7.jar tools/GenerateFixtures.java
java  -cp pdfbox-app-3.0.7.jar:tools GenerateFixtures
```

Output goes to `tests/fixtures/{smoke,font_heavy,encrypted,large,malformed}/*.pdf`.

### Step 2 — Extract reference snapshots

```bash
javac -cp pdfbox-app-3.0.7.jar tools/ExtractSnapshots.java
java  -cp pdfbox-app-3.0.7.jar:tools ExtractSnapshots
```

Reads each PDF with PDFBox and writes JSON snapshots to this directory.

### Step 3 — Validate the Rust port

```bash
cargo test --test cross_validate
```

## Files

| Snapshot JSON | PDF Fixture | Notes |
|---|---|---|
| `smoke_a4_single_page.json` | `smoke/a4_single_page.pdf` | A4 (595.3×841.9) |
| `smoke_letter_single_page.json` | `smoke/letter_single_page.pdf` | Letter (612×792) |
| `smoke_custom_page_size.json` | `smoke/custom_page_size.pdf` | 200×300 pt |
| `smoke_three_pages.json` | `smoke/three_pages.pdf` | 3 Letter pages |
| `smoke_five_pages.json` | `smoke/five_pages.pdf` | 5 Letter pages |
| `smoke_ten_pages.json` | `smoke/ten_pages.pdf` | 10 Letter pages |
| `smoke_minimal_catalog.json` | `smoke/minimal_catalog.pdf` | 0-page catalog |
| `smoke_version_1_7.json` | `smoke/version_1_7.pdf` | PDF-1.7 header |
| `smoke_rotated_90.json` | `smoke/rotated_90.pdf` | /Rotate 90 |
| `smoke_rotated_270.json` | `smoke/rotated_270.pdf` | /Rotate 270 |
| `smoke_round_trip.json` | `smoke/round_trip.pdf` | Save→reload round-trip |
| `font_heavy_text_hello_world.json` | `font_heavy/text_hello_world.pdf` | Helvetica "Hello World" |
| `font_heavy_text_multiline.json` | `font_heavy/text_multiline.pdf` | Two-line text |
| `font_heavy_text_empty_stream.json` | `font_heavy/text_empty_stream.pdf` | Empty content stream |
| `encrypted_permissions_all.json` | `encrypted/permissions_all.pdf` | All perms true |
| `encrypted_permissions_none.json` | `encrypted/permissions_none.pdf` | All perms false |
| `encrypted_permissions_print_only.json` | `encrypted/permissions_print_only.pdf` | Print only |
| `large_fifty_pages.json` | `large/fifty_pages.pdf` | 50 pages |
| `large_100_pages.json` | `large/100_pages.pdf` | 100 pages |
| `large_200_pages.json` | `large/200_pages.pdf` | 200 pages |
| `malformed_missing_header.json` | `malformed/missing_header.pdf` | No %PDF- header |
| `malformed_empty_bytes.json` | `malformed/empty_bytes.pdf` | 0 bytes |
| `malformed_broken_xref.json` | `malformed/broken_xref.pdf` | Garbage xref |
