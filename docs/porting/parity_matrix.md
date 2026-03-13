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
| COS object model | IP | M1 | ObjectId, CosName, CosDictionary, CosStream, CosObject enum implemented |
| Lexer/parser | NS | M1 | |
| XRef + trailer (table/stream) | NS | M1 | |
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
| `org.apache.pdfbox.cos` | `src/cos/` | Primitive object types | IP | M1 | `cos::object::tests`, `cos::object_id::tests` | CosObject enum with all 8 types + Reference |
| `org.apache.pdfbox.cos` | `src/cos/` | Dictionary/array/name helpers | IP | M1 | `cos::dictionary::tests`, `cos::name::tests` | Typed getters, insertion-order, well-known names |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | Header/startxref discovery | NS | M1 | | |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | Object parsing + indirect refs | NS | M1 | | |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | XRef table parsing | NS | M1 | | |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | XRef stream parsing | NS | M1 | | |
| `org.apache.pdfbox.pdfparser` | `src/parser/` | Object stream parsing | NS | M1 | | |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | `Document::load` | NS | M1 | | |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | Page tree traversal | NS | M2 | | |
| `org.apache.pdfbox.pdmodel` | `src/pdmodel/` | Resources access | NS | M2 | | |
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

- 2026-03-13: COS object model implemented (ObjectId, CosName, CosDictionary, CosStream, CosObject enum) — 31 unit tests.
- 2026-03-13: Initial parity matrix created.

