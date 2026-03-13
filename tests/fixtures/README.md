# Fixture Corpus

This directory stores PDF test fixtures used for parser compatibility and regression tests.

## Layout

- `smoke/`: small, valid PDFs for baseline open/parse/page checks.
- `malformed/`: damaged or partially invalid PDFs for recovery behavior tests.
- `font_heavy/`: PDFs with varied fonts/CMaps for text extraction fidelity.
- `encrypted/`: password-protected PDFs for security/permission checks.
- `large/`: larger PDFs for performance and memory profiling.

## Fixture Naming Convention

Use lowercase snake_case names:

- `<source>_<scenario>_<id>.pdf`
- optional metadata file: `<same_name>.meta.json`

Example:

- `acme_invoice_smoke_001.pdf`
- `acme_invoice_smoke_001.meta.json`

## Metadata Template

```json
{
  "description": "Short fixture purpose",
  "expected": {
    "page_count": 1,
    "encrypted": false,
    "text_extractable": true
  },
  "tags": ["smoke"]
}
```

## Notes

- Avoid committing sensitive or licensed documents without approval.
- Prefer minimal-size fixtures that isolate one behavior per file.
- Track any known Java PDFBox behavior differences in `docs/porting/parity_matrix.md`.

