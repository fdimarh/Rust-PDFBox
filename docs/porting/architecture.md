# Architecture and Module Contracts

_Last updated: 2026-07-30 — All modules implemented. **1,133 tests passing**, ~95% overall completion._

This document defines the architecture contracts for the Rust PDFBox port.

## Goals

- Keep a recognizable PDFBox mental model with idiomatic Rust APIs.
- Separate low-level PDF internals from high-level user-facing APIs.
- Preserve compatibility and testability through strict module boundaries.
- Support optional features via crate feature flags without API breakage.

## Design Principles

- Layered architecture: `io` → `parser`/`cos` → `pdmodel` → optional features.
- Fallible APIs with structured errors (`PdfError`) and context.
- Read paths are immutable by default; writing/editing is explicit.
- Optional modules (`text`, `crypto`, `layout`) gated by crate feature flags.

## Top-Level Module Map

```text
src/
  lib.rs            # public exports, feature flags, Document API, crate docs
  io/               # stream filter decoders (FlateDecode, ASCIIHex, ASCII85, RunLength, LZW)
  cos/              # low-level PDF object model (CosObject, CosName, CosDictionary, CosStream, ObjectId)
  parser/           # lexer, parser, xref (table+stream), object streams, malformed recovery
  pdmodel/          # high-level Document/Page/Resources/Rectangle APIs
  content/          # content stream operators, graphics state, text state
  font/             # font dictionaries, encodings, glyph mapping (feature: text)
  text/             # text extraction pipeline and layout heuristics (feature: text)
  writer/           # full-rewrite and incremental update save flows
  crypto/           # security handlers, RC4/AES/MD5, permission logic (feature: crypto)
  render/           # optional adapters — out of MVP scope
```

## Module Contracts

### `io` ✅ Implemented

Responsibilities:
- Decode compressed and encoded PDF streams.
- Dispatch to the correct filter by name (FlateDecode, ASCIIHexDecode, ASCII85Decode, RunLengthDecode, LZWDecode).

Public contract:
- `decode_stream(data, filter_name) -> Result<Vec<u8>, FilterError>` — single-filter decode
- `decode_stream_with_filters(data, filters) -> Result<Vec<u8>, FilterError>` — filter chain
- `FilterError` — missing filter, bad data, unsupported codec

Out of scope:
- PDF semantics, object parsing, or caching policy decisions.

### `cos` ✅ Implemented

Responsibilities:
- Define canonical PDF object types: `Null`, `Bool`, `Integer`, `Real`, `String`, `Name`, `Array`, `Dictionary`, `Stream`, `Reference`.
- Represent indirect object identity (`ObjectId`) and object containers (`ObjectStore`).

Public contract:
- `CosObject` enum — all variant types; `as_dictionary()`, `as_stream()`, `as_integer()`, etc.
- `CosName` — well-known name constants (`root`, `pages`, `type`, `kids`, `count`, etc.)
- `CosDictionary` — insertion-ordered key/value store; typed getters
- `CosStream` — dictionary + raw bytes
- `ObjectId` — (object_number: u32, generation: u16)
- `ObjectStore` — `HashMap<ObjectId, CosObject>`; `insert`, `get`, `len`

Out of scope:
- Xref resolution logic and high-level document navigation.

### `parser` ✅ Implemented

Responsibilities:
- Tokenize and parse raw PDF bytes into COS structures.
- Parse xref tables and xref streams (binary, variable-width /W).
- Parse object streams (ObjStm) for compressed PDF 1.5+ objects.
- Support lenient recovery for malformed files.

Public contract:
- `Parser::parse_document(bytes)` — full document parse into ObjectStore + xref
- `XRefTable` — merged xref (table + stream + Prev chain); `lookup(ObjectId)`
- `XRefStream` — binary xref; `from_stream()`, `to_stream()`, `lookup()`
- `ObjectStream` — ObjStm; `from_stream()`, `get_object()`, `object_numbers()`
- Error context with byte offsets and object IDs when available.
- Lenient path: `parse_lenient` returns partial document + `RecoveryReport`

Out of scope:
- End-user page/text APIs and write/update operations.

### `pdmodel` ✅ Implemented

Responsibilities:
- Expose user-facing `Document`, `Page`, and `Resources` APIs.
- Bridge parsed COS data into typed, ergonomic operations.

Public contract:
- `Document::load(path)` / `load_from_bytes(bytes)` / `load_lenient(bytes)`
- `Document::pages()` → `PageTree` — iteration and random access
- `Document::page_count()` → usize
- `Document::catalog()` → `&CosDictionary`
- `Document::trailer()` → `&CosDictionary`
- `Document::save(path)` / `save_to(writer)` / `save_incremental(writer, objects)`
- `Page::media_box()` / `crop_box()` / `rotation()` / `resources()` / `contents_object()`
- `Resources::font_dict()` / `xobject_dict()` / `ext_gstate_dict()`
- `Rectangle::width()` / `height()`
- `RecoveryReport` — `is_clean()`, `warnings`, `xref_recovered`, `objects_skipped`

Out of scope:
- Direct token parsing, low-level stream decoding internals.

### `content` ✅ Implemented

Responsibilities:
- Decode and tokenize content streams.
- Represent operators and maintain graphics/text state for text extraction.

Public contract:
- `ContentTokenizer` — iterator yielding `ContentToken` (operands + operators)
- `Operator` — 14 predicate methods: `is_text_show()`, `is_text_position()`, etc.
- `parse_content_stream(data)` → `Vec<Instruction>` — groups operands with operators
- `GraphicsState` — `q`/`Q` stack, `cm` (CTM), text state, font
- `TextState` — `Tf`/`Tl`/`Tc`/`Tw`/`Tz`/`Ts`; `Tm`/`Td`/`TD`/`T*` position tracking

Out of scope:
- Font file parsing and final text output formatting.

### `font` ✅ Implemented (feature: `text`)

Responsibilities:
- Parse font-related dictionaries and resolve character code → Unicode.
- Support all common font types in PDF: Type1, TrueType, Type0/CID.

Public contract:
- `ToUnicodeCMap` — `bfchar`/`bfrange` parser; `map_char(code)` → `Option<char>`
- `FontDescriptor` — `/FontDescriptor` dict; flags, metrics (ascent, descent, bbox, italic angle)
- `Encoding` — WinAnsiEncoding, MacRomanEncoding, StandardEncoding, PDFDocEncoding, custom `/Differences`
- `SimpleFont` — Type1/MMType1/TrueType/Type3; per-char widths; `decode_bytes()` (CMap → Encoding → Latin-1)
- `Type0Font` — composite font; `DescendantFont`; `CIDSystemInfo`; `/W` array width parsing; Identity-H/V
- `PdfFont` — unified dispatch enum; `FontResolver::resolve(resources, name)`
- `glyph_name_to_char(name)` — 150+ Adobe Glyph List subset

Out of scope:
- Text line/paragraph heuristics (in `text` module).

### `text` ✅ Implemented (feature: `text`)

Responsibilities:
- Convert content operators + font mappings into extracted text.
- Provide positional heuristics for multi-column and paragraph detection.

Public contract:
- `extract_text(stream_data, cmap)` → `String` — Tj/TJ/'/"; Y-sort line breaks; Latin-1 fallback
- `TextChunk` — text fragment with (x, y) position and font size
- `extract_text_with_layout(chunks, config)` → `String` — column detection + reading order
- `LayoutConfig` — `column_gap_threshold`, `line_y_tolerance`, `paragraph_gap_threshold`, `min_chunks_per_column`

Out of scope:
- Writing PDFs or mutating document objects.

### `writer` ✅ Implemented

Responsibilities:
- Serialize in-memory object model to valid PDFs.
- Support full rewrite and incremental append mode.

Public contract:
- `Serializer` — serialize any `CosObject` to correct PDF syntax bytes
- `Writer::write_document(doc, writer)` — full rewrite: all objects + xref + trailer
- `IncrementalWriter::write_update(original_bytes, new_objects, writer)` — append-only; subsection xref; `/Prev` chain
- `Document::save(path)` / `save_to(writer)` — full rewrite
- `Document::save_incremental(writer, objects)` — incremental append

Out of scope:
- Parser recovery logic and text extraction concerns.

### `crypto` ✅ Implemented (feature: `crypto`)

Responsibilities:
- Handle PDF Standard Security Handler (Rev 2, 3, 4).
- Enforce permission checks and decryption boundaries.

Public contract:
- `Permissions` — 32-bit /P flags; all 8 user permission bits; `can_print()`, `can_copy()`, etc.
- `Rc4` — RC4 stream cipher; `apply_keystream()`, `crypt(key, data)`
- `md5(data)` — MD5 hash (RustCrypto `md-5`); used for key derivation
- `aes_cbc_decrypt(key, iv, ciphertext)` — AES-128 CBC with PKCS#7 padding (RustCrypto `aes`+`cbc`)
- `EncryptionDict` — parsed /Encrypt dictionary parameters
- `StandardSecurityHandler` — key derivation Rev 2/3/4; user/owner password auth; per-object key; `decrypt_object()`
- `AuthResult` — `UserPassword(key)`, `OwnerPassword(key)`, `Failed`

Out of scope:
- Signatures and advanced PKI workflows.

### `render` 🔲 Out of scope

Responsibilities (future):
- Define adapter interfaces for external rendering backends.

Public contract (future):
- Feature-gated traits/types only; no core parser dependency inversion.

---

## Feature Flags

```toml
[features]
default = ["text", "crypto", "layout"]
text    = []           # font + text modules; glyph/encoding support
crypto  = [            # crypto module + optional RustCrypto deps
    "dep:aes", "dep:cbc", "dep:cipher", "dep:block-padding"
]
layout  = []           # LayoutConfig and positional heuristics re-exports
full    = ["text", "crypto", "layout"]   # all features

[dependencies]
md5     = { package = "md-5", version = "0.10" }   # always-on (key derivation)
digest  = "0.10"                                    # always-on (Digest trait)
aes     = { version = "0.8",  optional = true }
cbc     = { version = "0.1",  optional = true }
cipher  = { version = "0.4",  optional = true }
block-padding = { version = "0.3", optional = true }
```

| Build command | Result |
|---|---|
| `cargo build` (default) | ✅ text + crypto + layout |
| `cargo build --no-default-features` | ✅ core parser/COS/IO only |
| `cargo build --features text` | ✅ font + text stack |
| `cargo build --features crypto` | ✅ encryption |
| `cargo build --features full` | ✅ everything |

---

## Cross-Module Invariants

- `parser` is the only module that reads raw syntax tokens.
- `cos` contains no file IO and no parser state.
- `pdmodel` never bypasses parser/xref rules.
- `writer` consumes validated model state and emits deterministic structure.
- `text` correctness is measured against fixture + PDFBox parity outputs.
- `crypto` is entirely opt-in; core parsing never panics due to missing crypto deps.

## Error Boundary

```rust
#[non_exhaustive]
pub enum PdfError {
    Io(std::io::Error),
    Parse { offset: Option<usize>, context: String },
    Xref { object_id: String },
    Font { font_name: String },
    Crypto,
    Unsupported { feature: String },
}
```

## Validation Strategy

- Unit tests per module for core contracts (417 lib tests).
- Integration tests: `tests/parser_regression.rs` (28), `tests/corpus_breadth.rs` (33).
- Compatibility harness: `tests/compat_harness.rs` (7), `tests/fixture_gen.rs` (6).
- Cross-validation: `tests/cross_validate.rs` (19 tests, 5 Java PDFBox JSON snapshots).
- Round-trip tests for writer correctness (full-rewrite + incremental).
