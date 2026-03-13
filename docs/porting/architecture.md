# Architecture and Module Contracts

This document defines the initial architecture contracts for the Rust PDFBox port.

## Goals

- Keep a recognizable PDFBox mental model with idiomatic Rust APIs.
- Separate low-level PDF internals from high-level user-facing APIs.
- Preserve compatibility and testability through strict module boundaries.

## Design Principles

- Layered architecture: `io` -> `parser`/`cos` -> `pdmodel` -> optional features.
- Fallible APIs with structured errors (`PdfError`) and context.
- Read paths are immutable by default; writing/editing is explicit.
- Lazy loading for indirect objects and stream decoding.

## Top-Level Module Map

```text
src/
  lib.rs        # public exports, feature flags, crate docs
  io/           # random access reader abstractions
  cos/          # low-level PDF object model and serialization primitives
  parser/       # lexer/parser/xref/trailer/object stream readers
  pdmodel/      # high-level Document/Page/Resources APIs
  content/      # content stream operators and graphics/text state
  font/         # font dictionaries, encodings, glyph mapping
  text/         # extraction pipeline and ordering heuristics
  writer/       # full rewrite and incremental update save flows
  crypto/       # security handlers and permission logic
  render/       # optional adapters, feature-gated
```

## Module Contracts

## `io`

Responsibilities:
- Provide random-access read behavior and buffering abstractions.
- Expose traits needed by parser and lazy object loading.

Public contract:
- `ReadAt` trait (or equivalent) for offset-based reads.
- Implementations over in-memory bytes and file-backed sources.

Out of scope:
- PDF semantics, object parsing, or caching policy decisions.

## `cos`

Responsibilities:
- Define canonical PDF object types (`Null`, `Bool`, `Number`, `String`, `Name`, `Array`, `Dictionary`, `Stream`, `Reference`).
- Represent indirect object identity (`ObjectId`) and object containers.

Public contract:
- Stable, serializable object representation used by parser and writer.
- Dictionary/name helpers that avoid silent type coercions.

Out of scope:
- Xref resolution logic and high-level document navigation.

## `parser`

Responsibilities:
- Tokenize and parse raw PDF bytes into COS structures.
- Parse xref tables/streams, trailer dictionaries, and object streams.
- Support lazy object resolution hooks for `pdmodel`.

Public contract:
- `Parser` entry points for full load and partial/lazy load.
- Error context with byte offsets and object IDs when available.

Out of scope:
- End-user page/text APIs and write/update operations.

## `pdmodel`

Responsibilities:
- Expose user-facing `Document`, `Page`, and `Resources` APIs.
- Bridge parsed COS data into typed, ergonomic operations.

Public contract:
- `Document::load`, page iteration, metadata access.
- Stable object/resource access patterns for downstream modules.

Out of scope:
- Direct token parsing, low-level stream decoding internals.

## `content`

Responsibilities:
- Decode and tokenize content streams.
- Represent operators and maintain graphics/text state needed by text extraction.

Public contract:
- Operator iterator/token API.
- Reusable state model for `text` and future rendering adapters.

Out of scope:
- Font file parsing and final text output formatting.

## `font`

Responsibilities:
- Parse font-related dictionaries and embedded font data.
- Resolve character code to Unicode mapping pathways.

Public contract:
- Font abstraction APIs consumed by `text`.
- Explicit support levels per font type (tracked in parity matrix).

Out of scope:
- Text line/paragraph heuristics.

## `text`

Responsibilities:
- Convert content operators + font mappings into extracted text.
- Provide deterministic extraction modes.

Public contract:
- `extract_text`-style API with documented ordering semantics.

Out of scope:
- Writing PDFs or mutating document objects.

## `writer`

Responsibilities:
- Serialize in-memory object model to valid PDFs.
- Support full rewrite and later incremental append mode.

Public contract:
- Save APIs with option structs for strategy and compatibility.

Out of scope:
- Parser recovery logic and text extraction concerns.

## `crypto`

Responsibilities:
- Handle supported standard security flows.
- Enforce permission checks and decryption boundaries.

Public contract:
- Password-based open/decrypt interface used by `Document::load` paths.

Out of scope:
- Signatures and advanced PKI workflows (post-MVP).

## `render` (optional)

Responsibilities:
- Define adapter interfaces for external rendering backends.

Public contract:
- Feature-gated traits/types only; no core parser dependency inversion.

Out of scope:
- Mandatory rasterizer implementation in MVP.

## Cross-Module Invariants

- `parser` is the only module that reads raw syntax tokens.
- `cos` contains no file IO and no parser state.
- `pdmodel` never bypasses parser/xref rules.
- `writer` consumes validated model state and emits deterministic structure.
- `text` correctness is measured against fixture + PDFBox parity outputs.

## Error Boundary

A unified crate error type should include module-specific context:

- `Io`
- `Parse { offset, context }`
- `Xref { object_id }`
- `Font { font_name }`
- `Crypto`
- `Unsupported { feature }`

## Initial Feature Flags

- `text` - enables text extraction stack (`content`, `font`, `text`).
- `crypto` - enables encryption/decryption support.
- `render` - enables optional rendering adapters.

## Validation Strategy for This Architecture

- Unit tests per module for core contracts.
- Integration tests in `tests/fixtures/` to verify load/page/text behavior.
- Compatibility tests in `tests/compat_pdfbox/` to track parity drift.
- Round-trip tests for writer correctness once `writer` is introduced.

