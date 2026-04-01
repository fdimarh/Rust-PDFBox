# Step: XRef Streams Support (PDF 1.5+)

**Status:** ✅ **Complete**  
**Date:** 2026-04-01  
**Test Count:** +8 tests  
**Total Tests:** 560 (up from 552)  

## Summary

Implemented comprehensive XRef stream support for PDF 1.5+ compliance. XRef streams are binary-encoded cross-reference tables that replace traditional ASCII xref tables, enabling better compression and support for compressed objects (ObjStm).

## Components Delivered

### 1. XRefEntry — Binary XRef Record

Three entry types supported:

| Type | Purpose | Fields |
|---|---|---|
| **Free** | Unused object slot | next_obj_num (u32), generation (u16) |
| **InUse** | Active object | byte_offset (u64), generation (u16) |
| **Compressed** | Object in ObjStm | stream_obj_num (u32), index (u32) |

**Methods:**
- `to_bytes(widths)` — Variable-width binary encoding per `/W` array
- `from_bytes(data, widths)` — Parse binary entry, supporting flexible width combinations
- Direct field access for simple implementations

### 2. XRefSubsection — Contiguous Object Range

Represents a subsection in an xref stream (continuous object numbers).

```rust
pub struct XRefSubsection {
    pub start: u32,        // First object number
    pub count: u32,        // Number of entries
    pub entries: Vec<XRefEntry>,
}
```

**Methods:**
- `new(start)` — Create empty subsection
- `add_entry(entry)` — Add entry and auto-update count
- `to_bytes(widths)` — Serialize all entries

### 3. XRefStream — Complete XRef Stream Object

Full xref stream implementation with all PDF 1.5 features.

```rust
pub struct XRefStream {
    pub size: u32,                      // Total objects
    pub widths: [u16; 3],              // /W array: [type_width, field1_width, field2_width]
    pub subsections: Vec<XRefSubsection>,
    pub root: Option<u32>,              // /Root reference
    pub info: Option<u32>,              // /Info reference
    pub prev: Option<u64>,              // Previous xref offset (incremental)
}
```

**Methods:**
- `new(size)` — Create with default widths [1, 2, 2]
- `from_stream(dict, data)` — Parse from dictionary + binary data
  - Reads `/Size`, `/W`, `/Index` (optional subsection ranges)
  - Parses binary entries with variable widths
  - Supports multiple subsections
- `to_stream()` — Serialize to CosStream for writing
- `lookup(obj_num)` — Find entry by object number

## Features

✅ **Variable-width encoding** — `/W` array can specify 0-8 bytes per field  
✅ **Subsection support** — Multiple `/Index` ranges for sparse object numbers  
✅ **Compressed objects** — Type 2 entries reference ObjStm containers  
✅ **Incremental updates** — `/Prev` field chains to previous xref  
✅ **Flexible widths** — Automatically infer from `/W` during parsing  
✅ **Roundtrip** — Parse and re-serialize without data loss  

## Usage Examples

### Parse XRef Stream

```rust
use rust_pdfbox::parser::XRefStream;

let xref = XRefStream::from_stream(&stream_dict, &stream_data)?;
let entry = xref.lookup(5)?; // Find object 5

match entry {
    XRefEntry::InUse { offset, generation } => {
        println!("Object 5 at offset {}", offset);
    }
    XRefEntry::Compressed { stream, index } => {
        println!("Object 5 compressed in stream {} at index {}", stream, index);
    }
    XRefEntry::Free { next, generation } => {
        println!("Object 5 is free, next is {}", next);
    }
}
```

### Serialize XRef Stream

```rust
let mut xref = XRefStream::new(10);
let mut subsec = XRefSubsection::new(0);
subsec.add_entry(XRefEntry::InUse { offset: 100, generation: 0 });
subsec.add_entry(XRefEntry::InUse { offset: 200, generation: 0 });
xref.subsections.push(subsec);

let stream_obj = xref.to_stream();
```

## Test Coverage (8 tests)

✅ **Entry serialization** — InUse, Free, Compressed types
✅ **Entry parsing** — from_bytes with variable widths
✅ **Roundtrip** — parse then re-serialize, data unchanged
✅ **Subsection** — creation, entry management, serialization
✅ **Stream creation** — XRefStream::new with defaults
✅ **Lookup** — find entries across multiple subsections
✅ **Width flexibility** — support non-standard width combinations
✅ **Compressed references** — Type 2 entries for ObjStm

## Integration Points

1. **Parser integration** — Will be used in `src/parser/xref.rs` as alternative to ASCII xref
2. **Incremental save** — `/Prev` field enables incremental xref chains
3. **ObjStm support** — Type 2 entries reference compressed object streams (next phase)
4. **PDF 1.5+ files** — Many modern PDFs use xref streams

## Architecture

```
src/parser/xref_stream.rs
├── XRefEntry
│   ├── Free { next, generation }
│   ├── InUse { offset, generation }
│   └── Compressed { stream, index }
├── XRefSubsection
│   └── entries: Vec<XRefEntry>
└── XRefStream
    ├── subsections: Vec<XRefSubsection>
    ├── from_stream(dict, data)
    ├── to_stream() → CosStream
    └── lookup(obj_num) → Option<XRefEntry>
```

## Standards Compliance

**PDF §8.6** — Cross-reference streams (PDF 1.5+)
- ✅ `/Type` = `/XRef`
- ✅ `/Size` — highest object number + 1
- ✅ `/W` — width array (required)
- ✅ `/Index` — subsection ranges (optional, default [0, size])
- ✅ `/Root` — document catalog reference (optional)
- ✅ `/Info` — document info reference (optional)
- ✅ `/Prev` — previous xref offset (optional)

## Performance

- **Parse:** O(n) where n = total entries (single pass)
- **Lookup:** O(s) where s = number of subsections (binary search ready)
- **Memory:** Subsections only store entries (sparse support)

## Next Steps (Post-v1 Backlog)

1. **ObjStm (Object Streams)** — Decompress Type 2 entries
2. **Parser integration** — Use XRefStream in xref.rs
3. **Incremental save** — Write xref streams in incremental updates
4. **AES encryption** — Support encrypted xref streams
5. **LZW filter** — Additional stream filter

---

**v0.1.1 Candidate:** XRef streams enable PDF 1.5+ support. Foundation for next major version.

