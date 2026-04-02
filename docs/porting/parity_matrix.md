# PDFBox Parity Matrix (Java → Rust)

_Last updated: 2026-04-02 — ALL phases M0–M6 + all post-v1 bonuses complete. **510 tests passing, 0 failed.** v1 quality gate: ✅ PASSED. Post-v1 backlog: ✅ ALL COMPLETE._

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
| Post-v1 | Bonus features beyond v1 gate |

---

## High-Level Parity Scoreboard

| Area | Status | Milestone | Tests | Notes |
|---|---|---|---:|---|
| COS object model | `DV` | M1 | 31 | ObjectId, CosName, CosDictionary, CosStream, CosObject enum |
| Lexer / tokenizer | `DV` | M1 | 25 | All token types; `T*`, `'`, `"` content operators |
| Object parser | `DV` | M1 | 18 | Indirect objects, streams, nested dicts/arrays |
| XRef + trailer (table) | `DV` | M1 | 11 | Table + stream; Prev chain; startxref scan |
| XRef streams (binary) | `DV` | Post-v1 | 8 | `XRefEntry`, `XRefSubsection`, `XRefStream`; variable-width /W |
| Object Streams (ObjStm) | `DV` | Post-v1 | 9 | `ObjectStream`; preamble /N+/First; get_object; to_stream |
| Document::load | `DV` | M1 | 6 | Header → xref → ObjectStore → catalog |
| Malformed/edge lexer | `DV` | M2+ | 48 | Lexer edge tokens + error recovery |
| Malformed parser | `DV` | M2+ | 36 | Truncated, mismatched, deeply nested, bad keywords |
| Parser regression (integration) | `DV` | M2+ | 28 | Full pipeline tests via `tests/parser_regression.rs` |
| Document::load_lenient | `DV` | M6 | 8 | Missing header, broken xref linear scan, truncated objs, garbage |
| RecoveryReport | `DV` | M6 | 3 | `is_clean`, dirty state, valid-PDF clean; `#[non_exhaustive]` |
| Document / page model | `DV` | M2 | 14 | PDPage, PDPageTree, Rectangle, Resources |
| Content stream tokenizer | `DV` | M2 | 11 | ContentTokenizer, Operator, ContentToken |
| Content stream instruction parser | `DV` | M2 | 8 | parse_content_stream, Instruction, operand stack |
| Document pages() end-to-end | `DV` | M2 | 3 | pages(), page_count(), iter(), get() |
| Graphics state model | `DV` | M3 | 15 | Matrix, TextState, GraphicsState, q/Q, cm, BT/ET, Tm/Td/TD/T*/Tf/TL/Tc/Tw/Tz/Ts |
| Text operator dispatch | `DV` | M3 | — | Tj, TJ, ', " — implemented in extract_text |
| ToUnicode CMap parser | `DV` | M3 | 10 | bfchar, bfrange sequential+array, 1/2/4-byte codes, surrogate pairs |
| Text extraction MVP | `DV` | M3 | 14 | extract_text, TextChunk, Y-sort, line breaks, Latin-1 fallback |
| Font descriptor parsing | `DV` | Post-v1 | 8 | FontDescriptor, flags, metrics, bbox, ascent/descent |
| Font encoding parsing | `DV` | Post-v1 | 13 | WinAnsi, MacRoman, Standard, PDFDoc, Differences, glyph names |
| Font parsing — Type1/TrueType | `DV` | Post-v1 | 10 | SimpleFont; per-char widths; decode_bytes fallback chain |
| Font parsing — Type0/CID | `DV` | Post-v1 | 11 | Type0Font, DescendantFont, CIDSystemInfo, /W array, Identity-H/V |
| FontResolver | `DV` | Post-v1 | 9 | PdfFont enum; per-page font lookup; type dispatch |
| Positional heuristics / layout | `DV` | Post-v1 | 16 | LayoutConfig, detect_columns, group_into_lines, extract_with_layout |
| Compatibility harness | `DV` | Post-v1 | 13 | NormalizedOutput, CompatReport, compare_outputs, Corpus, FixtureSpec |
| Cross-validation suite | `DV` | Post-v1 | 19 | 5 JSON snapshots; Java PDFBox reference fixture tests; VResult engine |
| Full rewrite writer | `DV` | M4 | 1+1 | `Writer::write_document` + round-trip via `tests::round_trip_save_and_reload` |
| COS object serializer | `DV` | M4 | 8 | `Serializer` — all CosObject variants, name hex-escape, indirect object |
| Incremental append writer | `DV` | M4 | 9+2 | `IncrementalWriter::write_update` — subsection xref, `/Prev` chain |
| Standard Security Handler | `DV` | M5 | 17 | Key derivation Rev 2/3/4; user/owner auth; per-object key; RC4 decrypt |
| RC4 stream cipher | `DV` | M5 | 7 | RFC 6229 vectors; apply_keystream; crypt |
| AES-CBC decrypt | `DV` | Post-v1 | 5 | RustCrypto aes+cbc crates; PKCS#7 padding; Rev 4+ PDFs |
| Permission evaluation | `DV` | M5 | 5 | Permissions — all 8 flags; from_bits_p/to_bits_p; forced reserved bits |
| MD5 hash (key derivation) | `DV` | M5 | 8 | RustCrypto md-5 crate; RFC 1321 all 6 vectors pass |
| FlateDecode filter | `DV` | M6 | 3 | Pure-Rust deflate via miniz — stored block round-trip, bad-header error |
| ASCIIHexDecode filter | `DV` | M6 | 4 | Whitespace tolerance, odd nibble, EOD |
| ASCII85Decode filter | `DV` | M6 | 3 | `z` shorthand, partial group, EOD |
| RunLengthDecode filter | `DV` | M6 | 3 | Literal, repeat, EOD |
| LZW filter | `DV` | Post-v1 | 7 | LzwDecoder; 9–12 bit codes; MSB-first; reset/EOI; table growth |
| `decode_stream` dispatch | `DV` | M6 | 4 | Passthrough, named, array chain, unknown error |
| Stream data backfill | `DV` | M6 | 2 | Stream data populated from raw bytes on load |
| Corpus breadth — smoke | `DV` | M6 | 7 | Single-page A4/Letter, 5/10-page, media box, round-trip, incremental |
| Corpus breadth — malformed | `DV` | M6 | 8 | Broken xref, duplicate objs, empty bytes, garbage, missing header, etc. |
| Corpus breadth — font-heavy | `DV` | M6 | 4 | Content stream accessible, text extraction, multiline, empty stream |
| Corpus breadth — encrypted | `DV` | M6 | 5 | All/none/print permissions, auth result API, key derivation |
| Corpus breadth — large | `DV` | M6 | 5 | 50/100-page load/iter/round-trip, 200-object store |
| Crate feature flags | `DV` | Post-v1 | — | `text`, `crypto`, `layout`, `full`; `default = ["text","crypto","layout"]` |
| Rendering adapter | `N/A` | — | — | Out of MVP scope |
| Signatures / PKI | `N/A` | — | — | Out of MVP scope |

**Total tests passing: 510** (lib: 417 · compat_harness: 7 · corpus_breadth: 33 · cross_validate: 19 · fixture_gen: 6 · parser_regression: 28)

---

## Package-Level Matrix

| Java PDFBox Package | Rust Module | Capability | Status | Milestone | Test Reference | Notes |
|---|---|---|---|---|---|---|
| `o.a.p.cos` | `src/cos/object.rs` | Primitive types | `DV` | M1 | `cos::object::tests` (10) | All 8 variant types + Reference |
| `o.a.p.cos` | `src/cos/name.rs` | Name type + well-known names | `DV` | M1 | `cos::name::tests` (6) | prev, info, encrypt, resources, kids, count, length |
| `o.a.p.cos` | `src/cos/dictionary.rs` | Dictionary | `DV` | M1 | `cos::dictionary::tests` (8) | Insertion-order; typed getters |
| `o.a.p.cos` | `src/cos/stream.rs` | Stream | `DV` | M1 | `cos::stream::tests` (3) | Raw bytes; decode on demand |
| `o.a.p.cos` | `src/cos/object_id.rs` | ObjectId | `DV` | M1 | `cos::object_id::tests` (3) | Object number + generation |
| `o.a.p.pdfparser` | `src/parser/lexer.rs` | Lexer | `DV` | M1 | `parser::lexer::tests` (25) | `'`, `"`, `T*` operators; edge tokens; whitespace variants |
| `o.a.p.pdfparser` | `src/parser/parser.rs` | Object parser | `DV` | M1 | `parser::parser::tests` (18) | Indirect objects, streams, lookahead |
| `o.a.p.pdfparser` | `src/parser/xref.rs` | XRef table | `DV` | M1 | `parser::xref::tests` (11) | Traditional table; XRef stream; Prev chain; startxref |
| `o.a.p.pdfparser` | `src/parser/xref_stream.rs` | XRef stream (binary) | `DV` | Post-v1 | `parser::xref_stream::tests` (8) | Variable-width /W; /Index subsections; lookup by obj num |
| `o.a.p.pdfparser` | `src/parser/object_stream.rs` | Object stream / ObjStm | `DV` | Post-v1 | `parser::object_stream::tests` (9) | /N + /First preamble; get_object; round-trip |
| `o.a.p.pdfparser` | `src/parser/malformed.rs` | Lexer edge + malformed regression | `DV` | M2+ | `parser::malformed` (84) | Numbers, strings, names, comments, operators, truncated |
| `o.a.p.pdmodel` | `src/lib.rs` | `Document::load` | `DV` | M1 | lib `tests` (6) | Header → xref → ObjectStore → catalog ref |
| `o.a.p.pdmodel` | `src/lib.rs` | `Document::load_lenient` | `DV` | M6 | lib `tests` (8) | Missing header, broken xref, truncated, garbage, duplicate objs |
| `o.a.p.pdmodel` | `src/pdmodel/page_tree.rs` | Page tree traversal | `DV` | M2 | `pdmodel::page_tree::tests` (5) | Recursive walk; depth guard 64; O(1) index |
| `o.a.p.pdmodel` | `src/pdmodel/page.rs` | PDPage attributes | `DV` | M2 | `pdmodel::page::tests` (6) | media_box, crop_box, rotation, resources, contents_object |
| `o.a.p.pdmodel` | `src/pdmodel/page.rs` | PDResources | `DV` | M2 | `pdmodel::page::tests` | font_dict, xobject_dict, ext_gstate_dict, color_space_dict |
| `o.a.p.pdmodel` | `src/pdmodel/page.rs` | PDRectangle | `DV` | M2 | `pdmodel::page::tests` | width(), height(), Display |
| `o.a.p.contentstream` | `src/content/mod.rs` | Content stream tokenizer | `DV` | M2 | `content::tests` (11) | ContentTokenizer, Operator (14 predicates), ContentToken |
| `o.a.p.contentstream` | `src/content/mod.rs` | Instruction parser | `DV` | M2 | `content::tests` (8) | parse_content_stream; T*, ', "; operand stack |
| `o.a.p.contentstream` | `src/content/graphics_state.rs` | Graphics state model | `DV` | M3 | `content::graphics_state::tests` (15) | Matrix, TextState, GraphicsState, q/Q, Tm/Td/TD/T*/Tf/TL/Tc/Tw/Tz/Ts |
| `o.a.p.pdmodel.font` | `src/font/cmap.rs` | ToUnicode CMap | `DV` | M3 | `font::cmap::tests` (10) | bfchar, bfrange sequential+array, 1/2/4-byte, surrogate pairs |
| `o.a.p.pdmodel.font` | `src/font/descriptor.rs` | FontDescriptor | `DV` | Post-v1 | `font::descriptor::tests` (8) | /FontDescriptor; flags; metrics; bbox; ascent/descent |
| `o.a.p.pdmodel.font` | `src/font/encoding.rs` | Font Encoding | `DV` | Post-v1 | `font::encoding::tests` (13) | WinAnsi, MacRoman, Standard, PDFDoc, /Differences, glyph list |
| `o.a.p.pdmodel.font` | `src/font/simple.rs` | Type1 / TrueType | `DV` | Post-v1 | `font::simple::tests` (10) | SimpleFont; per-char widths; decode_bytes CMap→Enc→Latin1 |
| `o.a.p.pdmodel.font` | `src/font/type0.rs` | Type0 / CID composite | `DV` | Post-v1 | `font::type0::tests` (11) | Type0Font; DescendantFont; CIDSystemInfo; /W range+list; Identity-H/V |
| `o.a.p.pdmodel.font` | `src/font/font.rs` | PdfFont + FontResolver | `DV` | Post-v1 | `font::font::tests` (9) | Unified PdfFont enum; per-page font lookup; type dispatch |
| `o.a.p.text` | `src/text/mod.rs` | extract_text | `DV` | M3 | `text::tests` (14) | Tj/TJ/'/"; CMap decode; Latin-1 fallback; Y-sort; TextChunk |
| `o.a.p.text` | `src/text/mod.rs` | extract_text_with_layout | `DV` | Post-v1 | `text::tests` (16) | LayoutConfig; detect_columns; group_into_lines; paragraph breaks |
| `o.a.p.pdfwriter` | `src/writer/serializer.rs` | COS Object Serializer | `DV` | M4 | `writer::serializer::tests` (8) | All CosObject variants; name hex-escape; indirect object |
| `o.a.p.pdfwriter` | `src/writer/writer.rs` | Full rewrite writer | `DV` | M4 | lib `round_trip_save_and_reload` | Document::save_to; full object + xref serialization |
| `o.a.p.pdfwriter` | `src/writer/incremental.rs` | Incremental append writer | `DV` | M4 | `writer::incremental::tests` (9) | write_update; subsection xref; /Prev chain; Document::save_incremental |
| `o.a.p.pdmodel.encryption` | `src/crypto/permissions.rs` | Permission evaluation | `DV` | M5 | `crypto::permissions::tests` (5) | All 8 flags; from_bits_p/to_bits_p; forced reserved bits |
| `o.a.p.pdmodel.encryption` | `src/crypto/rc4.rs` | RC4 stream cipher | `DV` | M5 | `crypto::rc4::tests` (7) | RFC 6229 vectors; apply_keystream; crypt |
| `o.a.p.pdmodel.encryption` | `src/crypto/md5.rs` | MD5 hash | `DV` | M5 | `crypto::md5::tests` (8) | RustCrypto md-5; RFC 1321 all 6 test vectors |
| `o.a.p.pdmodel.encryption` | `src/crypto/handlers.rs` | Standard Security Handler | `DV` | M5 | `crypto::handlers::tests` (17) | Rev 2/3/4 key derivation; user/owner auth; per-object key; RC4 decrypt |
| `o.a.p.pdmodel.encryption` | `src/crypto/aes.rs` | AES-CBC decrypt | `DV` | Post-v1 | `crypto::aes::tests` (5) | RustCrypto aes+cbc; PKCS#7; Rev 4+ AES-encrypted PDFs |
| `o.a.p.pdfparser` | `src/io/mod.rs` | Stream filter decode | `DV` | M6 | `io::tests` (17) | FlateDecode, ASCIIHex, ASCII85, RunLength, decode_stream dispatch |
| `o.a.p.pdfparser` | `src/io/lzw.rs` | LZW filter | `DV` | Post-v1 | `io::lzw::tests` (7) | 9–12 bit codes; MSB-first; reset/EOI; table growth |
| `o.a.p.rendering` | `src/render/` | Rendering adapters | `N/A` | — | — | Out of MVP scope |

---

## Test Scorecard

| Milestone | Description | Tests | Status |
|---|---|---:|---|
| M0 | Design baseline | — | ✅ Done |
| M1 | COS + parser + xref + load | 94 | ✅ Done |
| M2 | Page / content primitives | 127 | ✅ Done |
| M2+ | Malformed / edge-case hardening | 211 | ✅ Done |
| M3 | Text extraction MVP | 256 | ✅ Done |
| M4 | Writer + incremental save | 297 | ✅ Done |
| M5 | Encrypted PDF | 334 | ✅ Done |
| M6 | v1 candidate hardening | 384 | ✅ Done — v1 PASSED |
| Post-v1 Bonus 1 | Font parsing (Type1, TrueType, Type0, descriptor, encoding, resolver) | +51 → 435 | ✅ Done |
| Post-v1 Bonus 2 | Positional heuristics (columns, lines, paragraphs) | +16 → 451 | ✅ Done |
| Post-v1 Bonus 3 | Compatibility testing harness (NormalizedOutput, CompatReport, FixtureSpec) | +13 → 464 | ✅ Done |
| Post-v1 Bonus 4 | XRef streams (binary xref, PDF 1.5+) | +8 → 472 | ✅ Done |
| Post-v1 Bonus 5 | Object Streams / ObjStm (PDF 1.5+) | +9 → 481 | ✅ Done |
| Post-v1 Bonus 6 | AES encryption (RustCrypto) | +5 → 486 | ✅ Done |
| Post-v1 Bonus 7 | LZW filter | +7 → 493 | ✅ Done |
| Post-v1 Bonus 8 | Crate feature flags | — → 510* | ✅ Done |
| Post-v1 Bonus 9 | Cross-validation suite (Java PDFBox snapshots) | +19 → 510 | ✅ Done |

_*feature flags added crypto refactoring which adjusted some test counts; final verified count: **510 passing, 0 failed**_

---

## Suite Breakdown (2026-04-02)

| Test Suite | File | Passing | Failed |
|---|---|---:|---:|
| Lib unit tests | `src/lib.rs` (+ all `src/**`) | 417 | 0 |
| Compat harness | `tests/compat_harness.rs` | 7 | 0 |
| Corpus breadth | `tests/corpus_breadth.rs` | 33 | 0 |
| Cross-validation | `tests/cross_validate.rs` | 19 | 0 |
| Fixture generator | `tests/fixture_gen.rs` | 6 | 0 |
| Parser regression | `tests/parser_regression.rs` | 28 | 0 |
| Doc-tests | — | 0 (1 ignored) | 0 |
| **Total** | | **510** | **0** |

---

## Compatibility Test Matrix

| Fixture Tier | Scope | Java Output | Rust Output | Status |
|---|---|---|---|---|
| Smoke | Page count, dimensions, rotation, version | Reference JSON snapshot | Validated ✅ | `DV` |
| Large | Page count spot-checks (0, 49, 99) | Reference JSON snapshot | Validated ✅ | `DV` |
| Malformed | Recovery behavior + warnings | N/A (lenient loader) | RecoveryReport | `DV` |
| Font-heavy | Text output fidelity | N/A | Content stream extraction | `DV` |
| Encrypted | Open + permissions | N/A | StandardSecurityHandler | `DV` |

---

## Known Deviations and Intentional Differences

| Feature | Decision | Rationale | Review Date |
|---|---|---|---|
| `T*` / `'` / `"` operator lexing | Allow `*`, `'`, `"` as keyword continuation/start chars | PDF spec Table 107; Java PDFBox handles these natively | 2026-03-26 |
| Duplicate dict keys | Last-write wins | PDF §7.3.7 allows either; matches common practice | 2026-03-26 |
| MD5 implementation | RustCrypto `md-5` crate (was pure-Rust) | Simplify maintenance; avoid rolling own crypto | 2026-04-02 |
| AES implementation | RustCrypto `aes` + `cbc` crates | Correct, audited, actively maintained | 2026-04-02 |
| Rendering | Not implemented | Out of MVP scope; would require separate crate | 2026-03-26 |

---

## Tracking Rules

- Every row moving to `DV` must link to at least one test path.
- Any row in `PV` must include explicit missing behavior in Notes.
- Deviations from Java behavior must be documented in the table above.

---

## Update Log

- **2026-04-02:** Cross-validation suite complete (Bonus 9) — `tests/cross_validate.rs` with hand-rolled JSON parser, `VResult` engine, `cv!` macro, 5 JSON reference snapshots in `tests/cross_validation/`, in-memory PDF generators. 19 tests, all passing. **510 total tests, 0 failed.**
- **2026-04-02:** Crate feature flags complete (Bonus 8) — `text`, `crypto`, `layout`, `full`; RustCrypto deps optional under `crypto` feature; `md5`+`digest` always-on; `--no-default-features` builds cleanly.
- **2026-04-02:** RustCrypto migration — `aes`/`cbc`/`cipher`/`block-padding` optional deps under `crypto` feature; `md-5`/`digest` always-on non-optional. Fixed AES empty-ciphertext validation. Resolved all IDE import false-positives.
- **2026-04-01:** LZW filter complete (Bonus 7) — `LzwDecoder`; 9–12 bit MSB-first codes; reset/EOI; table growth; 7 tests. Integrated into `decode_stream` dispatch.
- **2026-04-01:** AES encryption complete (Bonus 6) — `aes_cbc_decrypt` via RustCrypto `aes`+`cbc` crates; PKCS#7 padding; 7 tests.
- **2026-04-01:** ObjStm complete (Bonus 5) — `ObjectStream`; preamble /N+/First; `get_object`; `contains`; `to_stream`; 9 tests.
- **2026-04-01:** XRef streams complete (Bonus 4) — `XRefEntry` (3 types); `XRefSubsection`; `XRefStream`; variable-width /W; /Index subsections; 8 tests.
- **2026-04-01:** Compatibility harness complete (Bonus 3) — `NormalizedOutput`, `CompatReport`, `compare_outputs`, `Corpus`, `FixtureSpec`, `FixtureMetadata`; 13 integration tests.
- **2026-04-01:** Positional heuristics complete (Bonus 2) — `LayoutConfig`, `detect_columns`, `group_into_lines`, `extract_with_layout`, paragraph breaks; 16 tests.
- **2026-04-01:** Font parsing complete (Bonus 1) — `FontDescriptor` (8), `Encoding` (13), `SimpleFont` Type1+TrueType (10), `Type0Font` (11), `PdfFont`+`FontResolver` (9); 51 font tests.
- **2026-04-01:** M6 complete — `Document::load_lenient` + `RecoveryReport`, `backfill_stream_data`, `tests/corpus_breadth.rs` (33), public re-exports, `docs/porting/v1_quality_gate.md`. **384 tests, v1 PASSED.**
- **2026-04-01:** M5 complete — `Permissions` (5), `Rc4` (7), `md5` (8), `StandardSecurityHandler` (17). **334 tests.**
- **2026-04-01:** M4 complete — `Serializer` (8), `Writer` + round-trip, `IncrementalWriter` (9+2). **297 tests.**
- **2026-03-26:** M3 complete — `GraphicsState` (15), ToUnicode CMap (10), `extract_text` (14). **256 tests.**
- **2026-03-26:** M2+ hardening — malformed regression (84 unit), parser_regression (27 integration). **211 tests.**
- **2026-03-26:** M2 complete — page/page_tree/resources/rectangle, content tokenizer + instruction parser. **127 tests.**
- **2026-03-26:** M1 complete — XRef table/stream, startxref, Prev chain, ObjectStore, Document::load. **94 tests.**
- **2026-03-13:** Lexer and Parser — tokenizer, object parser, indirect references. **43 unit tests**.
- **2026-03-13:** COS object model — ObjectId, CosName, CosDictionary, CosStream, CosObject. **31 unit tests**.
- **2026-03-13:** Initial parity matrix created.
