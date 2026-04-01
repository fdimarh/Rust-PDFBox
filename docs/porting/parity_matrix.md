# PDFBox Parity Matrix (Java → Rust)

_Last updated: 2026-03-26 — M0/M1/M2/M2+/M3 complete, 256 tests passing._

This document tracks feature parity between Apache Java PDFBox and this Rust port.

## How to Use

- Update status per feature as implementation progresses.
- Link each row to tests and milestone deliverables.
- Record intentional deviations in the Notes column.

## Status Legend

| Code | Meaning |
|---|---|
| `NS` | Not Started |
| `IP` | In Progress |
| `PV` | Partial — usable but incomplete |
| `DV` | Done / Verified (tests passing) |
| `N/A` | Not in scope |

## Milestone Mapping

| Milestone | Description |
|---|---|
| M0 | Discovery / Design |
| M1 | Core parser + COS + xref |
| M2 | Document / page / content primitives |
| M2+ | Malformed / edge-case hardening |
| M3 | Fonts + text extraction MVP |
| M4 | Writer + incremental save |
| M5 | Encryption |
| M6 | v1 candidate hardening |

---

## High-Level Parity Scoreboard

| Area | Status | Target Milestone | Test Count | Notes |
|---|---|---:|---:|---|
| COS object model | `DV` | M1 | 31 | ObjectId, CosName, CosDictionary, CosStream, CosObject enum |
| Lexer / tokenizer | `DV` | M1 | 25 | All token types; `T*`, `'`, `"` content operators |
| Object parser | `DV` | M1 | 18 | Indirect objects, streams, nested dicts/arrays |
| XRef + trailer | `DV` | M1 | 11 | Table + stream; Prev chain; startxref scan |
| Document::load | `DV` | M1 | 6 | Header → xref → ObjectStore → catalog |
| Malformed/edge lexer | `DV` | M2+ | 48 | Lexer edge tokens + error recovery |
| Malformed parser | `DV` | M2+ | 36 | Truncated, mismatched, deeply nested, bad keywords |
| Integration regression | `DV` | M2+ | 27 | Full pipeline tests via `tests/parser_regression.rs` |
| Document / page model | `DV` | M2 | 14 | PDPage, PDPageTree, Rectangle, Resources |
| Content stream tokenizer | `DV` | M2 | 11 | ContentTokenizer, Operator, ContentToken |
| Content stream instruction parser | `DV` | M2 | 8 | parse_content_stream, Instruction, operand stack |
| Document pages() end-to-end | `DV` | M2 | 3 | pages(), page_count(), iter(), get() |
| Graphics state model | `DV` | M3 | 15 | Matrix, TextState, GraphicsState, q/Q stack, Tm/Td/TD/T*/Tf/TL/Tc/Tw/Tz/Ts |
| Text operator dispatch | `DV` | M3 | — | Tj, TJ, ', " — implemented in extract_text |
| ToUnicode CMap parser | `DV` | M3 | 10 | bfchar, bfrange sequential+array, 1/2/4-byte codes, surrogate pairs |
| Text extraction MVP | `DV` | M3 | 14 | extract_text, TextChunk, Y-sort, line breaks, Latin-1 fallback |
| Font parsing (Type1) | `NS` | M3+ | — | |
| Font parsing (TrueType) | `NS` | M3+ | — | |
| Font parsing (Type0 / CID) | `NS` | M3+ | — | |
| Positional heuristics | `PV` | M3+ | — | Basic Y-sort + gap detection in chunks_to_string |
| Full rewrite writer | `NS` | M4 | — | |
| Incremental append writer | `NS` | M4 | — | |
| Standard Security Handler | `NS` | M5 | — | |
| RC4 / AES decrypt | `NS` | M5 | — | |
| Permission evaluation | `NS` | M5 | — | |
| Compatibility harness | `NS` | M6 | — | Java vs Rust output diff |

**Total tests passing: 256**

---

## Package-Level Matrix

| Java PDFBox Package | Rust Module | Capability | Status | Milestone | Test Reference | Notes |
|---|---|---|---|---|---|---|
| `o.a.p.cos` | `src/cos/object.rs` | Primitive types | `DV` | M1 | `cos::object::tests` (10) | All 8 variant types + Reference; `into_dictionary()` added |
| `o.a.p.cos` | `src/cos/name.rs` | Name type + well-known names | `DV` | M1 | `cos::name::tests` (6) | `prev`, `info`, `encrypt`, `resources`, `kids`, `count`, `length` |
| `o.a.p.cos` | `src/cos/dictionary.rs` | Dictionary | `DV` | M1 | `cos::dictionary::tests` (8) | Insertion-order; typed getters |
| `o.a.p.cos` | `src/cos/stream.rs` | Stream | `DV` | M1 | `cos::stream::tests` (3) | Raw bytes; decode on demand |
| `o.a.p.cos` | `src/cos/object_id.rs` | ObjectId | `DV` | M1 | `cos::object_id::tests` (3) | Object number + generation |
| `o.a.p.pdfparser` | `src/parser/lexer.rs` | Lexer | `DV` | M1 | `parser::lexer::tests` (25) | `'`, `"`, `T*` operators; edge tokens; whitespace variants |
| `o.a.p.pdfparser` | `src/parser/parser.rs` | Object parser | `DV` | M1 | `parser::parser::tests` (18) | Indirect objects, streams, lookahead |
| `o.a.p.pdfparser` | `src/parser/xref.rs` | XRef table | `DV` | M1 | `parser::xref::tests` (11) | Traditional table; XRef stream; Prev chain; startxref |
| `o.a.p.pdfparser` | `src/parser/xref.rs` | XRef stream | `PV` | M1 | `parser::xref::tests` | Uncompressed; FlateDecode deferred to io/filter |
| `o.a.p.pdfparser` | `src/parser/xref.rs` | Object stream | `NS` | M1 | — | Deferred |
| `o.a.p.pdfparser` | `src/parser/malformed.rs` | Lexer edge + error regression | `DV` | M2+ | `parser::malformed::lexer_edge_tokens` (48) | Numbers, strings, names, comments, whitespace, operators |
| `o.a.p.pdfparser` | `src/parser/malformed.rs` | Parser malformed regression | `DV` | M2+ | `parser::malformed::parser_malformed` (36) | Truncated, mismatched, nested, keywords, streams |
| `o.a.p.pdmodel` | `src/lib.rs` | `Document::load` | `DV` | M1 | `tests` (6) | Header → xref → ObjectStore → catalog ref |
| `o.a.p.pdmodel` | `src/pdmodel/page_tree.rs` | Page tree traversal | `DV` | M2 | `pdmodel::page_tree::tests` (5) | Recursive walk; depth guard 64; O(1) index |
| `o.a.p.pdmodel` | `src/pdmodel/page.rs` | PDPage attributes | `DV` | M2 | `pdmodel::page::tests` (6) | media_box, crop_box, rotation, resources, contents_object |
| `o.a.p.pdmodel` | `src/pdmodel/page.rs` | PDResources | `DV` | M2 | `pdmodel::page::tests::resources_font_dict` | font_dict, xobject_dict, ext_gstate_dict, color_space_dict |
| `o.a.p.pdmodel` | `src/pdmodel/page.rs` | PDRectangle | `DV` | M2 | `pdmodel::page::tests::rectangle_dimensions` | width(), height(), Display |
| `o.a.p.pdmodel` | `src/lib.rs` | `Document::pages()` / `page_count()` | `DV` | M2 | `tests::document_page_count`, `document_pages_iter`, `document_pages_get_by_index` | End-to-end via real PDF bytes |
| `o.a.p.contentstream` | `src/content/mod.rs` | Content stream tokenizer | `DV` | M2 | `content::tests` (11) | ContentTokenizer, Operator (14 predicates), ContentToken |
| `o.a.p.contentstream` | `src/content/mod.rs` | Instruction parser | `DV` | M2 | `content::tests` (8) | parse_content_stream; T*, ', " operators; operand stack |
| `o.a.p.contentstream` | `src/content/` | Graphics state model | `DV` | M3 | `content::graphics_state::tests` (15) | Matrix, TextState, GraphicsState, q/Q, Tm/Td/TD/T*/Tf/TL/Tc/Tw/Tz/Ts |
| `o.a.p.pdmodel.font` | `src/font/cmap.rs` | ToUnicode CMap | `DV` | M3 | `font::cmap::tests` (10) | bfchar, bfrange sequential+array, 1/2/4-byte, surrogate pairs |
| `o.a.p.text` | `src/text/mod.rs` | extract_text MVP | `DV` | M3 | `text::tests` (14) | Tj/TJ/'/", CMap decode, Latin-1 fallback, Y-sort, TextChunk |
| `o.a.p.pdmodel.font` | `src/font/` | PDFont base | `NS` | M3+ | — | |
| `o.a.p.pdmodel.font` | `src/font/` | Type1 | `NS` | M3+ | — | |
| `o.a.p.pdmodel.font` | `src/font/` | TrueType | `NS` | M3+ | — | |
| `o.a.p.pdmodel.font` | `src/font/` | Type0 / CID | `NS` | M3+ | — | |
| `o.a.p.text` | `src/text/` | PDFTextStripper | `PV` | M3+ | `text::tests` | MVP done; full stripper (columns, multi-page) pending |
| `o.a.p.text` | `src/text/` | Content-order extraction | `DV` | M3 | `text::tests::tj_*` | Implemented via Y-sort heuristic |
| `o.a.p.text` | `src/text/` | Positional heuristics | `PV` | M3+ | `text::tests::chunks_to_string_*` | Basic Y-gap + X-gap detection; column layout pending |
| `o.a.p.pdfwriter` | `src/writer/writer.rs` | Full rewrite writer | `DV` | M4 | `lib.rs#round_trip_save_and_reload` | `Document::save_to` with full object and xref serialization |
| `o.a.p.pdfwriter` | `src/writer/serializer.rs` | COS Object Serializer | `DV` | M4 | `writer::serializer::tests` | Writes all `CosObject` variants to correct syntax |
| `o.a.p.pdfwriter` | `src/writer/` | Incremental append writer | `NS` | M4 | — | |
| `o.a.p.pdmodel.encryption` | `src/crypto/` | Standard Security Handler | `NS` | M5 | — | |
| `o.a.p.pdmodel.encryption` | `src/crypto/` | RC4 / AES | `NS` | M5 | — | |
| `o.a.p.pdmodel.encryption` | `src/crypto/` | Permission evaluation | `NS` | M5 | — | |

---

## Test Scorecard

| Milestone | Description | Tests | Status |
|---|---|---:|---|
| M0 | Design baseline | — | ✅ Done |
| M1 | COS + parser + xref + load | 94 | ✅ Done |
| M2 | Page / content primitives | 127 | ✅ Done |
| M2+ | Malformed / edge-case hardening | 211 | ✅ Done |
| M3 | Text extraction MVP | 256 | ✅ Done |
| M4 | Writer + incremental save | TBD | 🔲 Next |
| M5 | Encrypted PDF | TBD | 🔲 Planned |
| M6 | v1 candidate | TBD | 🔲 Planned |

---

## Compatibility Test Matrix

| Fixture Tier | Scope | Java Output | Rust Output | Status |
|---|---|---|---|---|
| Smoke | Page count, metadata, open/load | Pending | Pending | `NS` |
| Malformed | Recovery behavior + warnings | Pending | Pending | `NS` |
| Font-heavy | Text output fidelity | Pending | Pending | `NS` |
| Encrypted | Open + permissions | Pending | Pending | `NS` |
| Large files | Parse throughput / memory profile | Pending | Pending | `NS` |

---

## Known Deviations and Intentional Differences

| Feature | Decision | Rationale | Review Date |
|---|---|---|---|
| `T*` / `'` / `"` operator lexing | Allow `*`, `'`, `"` as keyword continuation/start chars | PDF spec Table 107; Java PDFBox handles these natively | 2026-03-26 |
| XRef stream with FlateDecode | Deferred — raw byte streams parsed; compressed deferred to `src/io/` filter module | Avoid bloating M1 scope | 2026-03-26 |
| Duplicate dict keys | Last-write wins (first in insertion-order map) | PDF §7.3.7 allows either; matches common practice | 2026-03-26 |

---

## Tracking Rules

- Every row moving to `DV` must link to at least one test path.
- Any row in `PV` must include explicit missing behavior in Notes.
- Deviations from Java behavior must be documented in the table above.

---

## Update Log

- **2026-03-26:** M3 complete — `src/content/graphics_state.rs` (GraphicsState, Matrix, TextState — 15 tests), `src/font/cmap.rs` (ToUnicode CMap parser — 10 tests), `src/text/mod.rs` (extract_text, TextChunk, Y-sort line breaks — 14 tests). Total: **256 tests passing**.
- **2026-03-26:** M2+ hardening — `src/parser/malformed.rs` (84 unit tests: lexer edge tokens, parser malformed), `tests/parser_regression.rs` (27 integration tests). Fixed lexer `'` and `"` operator handling. Removed unused `CosName` import. Total: **211 tests passing**.
- **2026-03-26:** M2 complete — `pdmodel::page` (Page, Rectangle, Resources), `pdmodel::page_tree` (PageTree, recursive walk, depth guard), `content` (ContentTokenizer, Operator predicates, parse_content_stream, Instruction). Lexer extended for `T*`/`'`/`"`. `Document::pages()` and `page_count()` wired. 33 new tests. Total: **127 tests**.
- **2026-03-26:** M1 complete — XRef table/stream parsing, `startxref` discovery, `Prev` chain, merged `XRefTable`, `ObjectStore`, full `Document::load`. 11 xref tests + 6 Document tests. Total: **94 tests**.
- **2026-03-13:** Lexer and Parser — tokenizer, object parser, indirect reference detection, indirect object definitions. **43 unit tests**.
- **2026-03-13:** COS object model — ObjectId, CosName, CosDictionary, CosStream, CosObject enum. **31 unit tests**.
- **2026-03-13:** Initial parity matrix created.
