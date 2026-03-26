# PDFBox Parity Matrix (Java -> Rust)

This document tracks feature parity between Apache Java PDFBox and this Rust port.

## How to Use

- Update status per feature as implementation progresses.
- Link each row to tests and milestone deliverables.
- Record intentional deviations in the Notes column.

## Status Legend

- `NS` = Not Started
- `IP` = In Progress
- `PV` = Partial (usable but incomplete)
- `DV` = Done / Verified
- `N/A` = Not in scope

## Milestone Mapping

- `M0` Discovery/Design
- `M1` Core parser + COS + xref
- `M2` Document/page/content primitives
- `M3` Fonts + text extraction MVP
- `M4` Writer + incremental save
- `M5` Encryption
- `M6` v1 candidate hardening

## High-Level Parity Scoreboard

| Area | Status | Target Milestone | Notes |
|---|---|---:|---|
| COS object model | DV | M1 | ObjectId, CosName, CosDictionary, CosStream, CosObject enum — 31 tests |
| Lexer/parser | DV | M1 | Lexer + Parser with lookahead, indirect refs, all COS types — 43 tests |
| XRef + trailer (table/stream) | DV | M1 | Traditional xref table, XRef stream (uncompressed), startxref discovery, Prev chain, merged XRefTable — 11 tests |
| Document::load baseline | DV | M1 | Header check, xref load, eager object store, catalog ref resolution — 6 tests |
| Document/page model | NS | M2 | |
| Content stream operators | NS | M2 | |
| Font parsing + CMap | NS | M3 | |
| Text extraction | NS | M3 | |
| Full rewrite writer | NS | M4 | |
| Incremental save | NS | M4 | |
| Standard Security | NS | M5 | |
| Compatibility hardening | NS | M6 | |

## Package-Level Matrix

| Java PDFBox Area | Rust Module | Key Capability | Status | Target | Test Reference | Notes |
|---|---|---|---|---:|---|---|
| `org.apache.pdfbox.cos` | `src/cos/` | Primitive object types | DV | M1 | `cos::object::tests`, `cos::object_id::tests` | CosObject enum with all 8 types + Reference |
| `org.apache.pdfbox.cos` | `src/cos/` | Dictionary/array/name helpers | DV | M1 | `cos::dictionary::tests`, `cos::name::tests` | Typed getters, insertion-order, well-known names (prev, info, encrypt added) |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | Header/startxref discovery | DV | M1 | `parser::xref::tests::find_startxref_*` | `find_startxref` scans tail 1024 bytes, handles CRLF |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | Object parsing + indirect refs | DV | M1 | `parser::lexer::tests`, `parser::parser::tests` | Lexer + Parser with lookahead for indirect refs |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | XRef table parsing | DV | M1 | `parser::xref::tests::parse_traditional_xref_table_basic`, `load_xref_end_to_end` | 20 and 21-byte entry variants, subsection support, Prev chain follow |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | XRef stream parsing | PV | M1 | `parser::xref` | Uncompressed xref streams parsed; FlateDecode deferred to io/filter module |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | Object stream parsing | NS | M1 | | |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | `Document::load` | DV | M1 | `tests::loads_minimal_pdf`, `minimal_pdf_catalog_resolved`, `trailer_has_size` | Header + xref + eager object store + catalog ref resolution |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | Page tree traversal | DV | M2 | `pdmodel::page_tree::tests` (5 tests) | `PageTree::new`, `iter`, `get`, depth-guard, error on missing root |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | `PDPage` attributes | DV | M2 | `pdmodel::page::tests` (6 tests) | `media_box`, `crop_box`, `rotation`, `resources`, `contents_object` |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | Resources access | DV | M2 | `pdmodel::page::tests::resources_font_dict` | `font_dict`, `xobject_dict`, `ext_gstate_dict`, `color_space_dict` |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | `Document::pages()` / `page_count()` | DV | M2 | `tests::document_page_count`, `document_pages_iter`, `document_pages_get_by_index` | End-to-end via real minimal PDF bytes |
| `org.apache.pdfbox.contentstream` | `src/content/` | Content stream tokenizer | DV | M2 | `content::tests` (11 tokenizer tests) | `ContentTokenizer`, `Operator` with predicates, `ContentToken` |
| `org.apache.pdfbox.contentstream` | `src/content/` | Instruction parser | DV | M2 | `content::tests` (8 instruction tests) | `parse_content_stream` groups operands + operator; handles `T*`, `'`, `"` |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | Page model | NS | M2 | | |
| content stream APIs | `src/content/` | Operator tokenization | NS | M2 | | |
| content stream APIs | `src/content/` | Graphics/text state tracking | NS | M3 | | |
| font handling | `src/font/` | Type1 support | NS | M3 | | |
| font handling | `src/font/` | TrueType support | NS | M3 | | |
| font handling | `src/font/` | Type0/CID basics | NS | M3 | | |
| text extraction | `src/text/` | ToUnicode mapping pipeline | NS | M3 | | |
| text extraction | `src/text/` | Content-order extraction | NS | M3 | | |
| text extraction | `src/text/` | Basic positional heuristics | NS | M3 | | |
| writer APIs | `src/writer/` | Full rewrite save | NS | M4 | | |
| writer APIs | `src/writer/` | Incremental append save | NS | M4 | | |
| encryption | `src/crypto/` | Password open flow | NS | M5 | | |
| encryption | `src/crypto/` | Permission evaluation | NS | M5 | | |
| encryption | `src/crypto/` | RC4/AES support | NS | M5 | | |

## Compatibility Test Matrix

| Fixture Tier | Scope | Java Comparator Output | Rust Output | Status |
|---|---|---|---|---|
| Smoke | Page count, metadata, open/load | Pending | Pending | NS |
| Malformed | Recovery behavior + warnings | Pending | Pending | NS |
| Font-heavy | Text output fidelity | Pending | Pending | NS |
| Encrypted | Open + permissions | Pending | Pending | NS |
| Large files | Parse throughput/memory profile | Pending | Pending | NS |

## Known Deviations and Intentional Differences

| Feature | Decision | Rationale | Review Date |
|---|---|---|---|
| | | | |

## Tracking Rules

- Every row moving to `DV` must link to at least one test path.
- Any row in `PV` must include explicit missing behavior in Notes.
- Deviations from Java behavior must be documented and approved.

## Update Log

- 2026-03-26: Phase 2 complete — `pdmodel::page` (Page, Rectangle, Resources), `pdmodel::page_tree` (PageTree, recursive walk, depth guard), `content` (ContentTokenizer, Operator predicates, parse_content_stream, Instruction). Lexer extended to support `T*`/`'`/`"` operators. Document::pages() and page_count() wired end-to-end. 33 new tests. Total: 127 tests.
- 2026-03-26: XRef table/stream parsing, `startxref` discovery, `Prev` chain following, merged `XRefTable`, `ObjectStore`, and full `Document::load` wired — 11 new xref tests + 6 Document tests. M1 complete (except compressed xref streams pending filter decoding).
- 2026-03-13: Lexer and Parser implemented — tokenizer, object parser, indirect reference detection, indirect object definitions — 43 unit tests.
- 2026-03-13: COS object model implemented (ObjectId, CosName, CosDictionary, CosStream, CosObject enum) — 31 unit tests.
- 2026-03-13: Initial parity matrix created.

