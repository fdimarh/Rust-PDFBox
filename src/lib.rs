//! Rust port of Apache PDFBox — comprehensive PDF reading and writing.
//!
//! This crate provides a Rust implementation of PDF reading and writing,
//! following the architecture of Apache Java PDFBox.
//!
//! # Features
//!
//! - `text` (default): Text extraction and font handling (CMap, Type1, TrueType, Type0)
//! - `crypto` (default): Encryption/decryption (RC4, AES, MD5) and permissions
//! - `layout` (default): Advanced layout analysis (column detection, reading order)
//! - `full`: All features enabled
//!
//! See `docs/porting/` for the porting plan and detailed architecture.

pub mod content;
#[cfg(feature = "forms")]
pub mod forms;
#[cfg(feature = "outline")]
pub mod outline;
#[cfg(feature = "pageops")]
pub mod pageops;
#[cfg(feature = "compress")]
pub mod compress;
pub mod cos;
#[cfg(feature = "crypto")]
pub mod crypto;
#[cfg(feature = "text")]
pub mod font;
pub mod io;
pub mod parser;
pub mod pdmodel;
pub mod render;
pub mod signing;
#[cfg(feature = "text")]
pub mod text;
pub mod writer;

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io as std_io;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cos::{CosDictionary, CosName, CosObject, ObjectId};
use parser::xref::{XRefEntry, XRefTable};
use parser::{ParseError, Parser};

pub use cos::ObjectId as PdfObjectId;

// ---------------------------------------------------------------------------
// Error model
// ---------------------------------------------------------------------------

#[derive(Debug)]
#[non_exhaustive]
pub enum PdfError {
    Io(std_io::Error),
    Parse {
        offset: Option<u64>,
        context: String,
    },
    Xref {
        object_id: Option<ObjectId>,
    },
    Font {
        font_name: String,
    },
    Crypto,
    Unsupported {
        feature: &'static str,
    },
    #[cfg(feature = "compress")]
    Compress {
        reason: String,
    },
}

impl Display for PdfError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Parse { offset, context } => {
                if let Some(offset) = offset {
                    write!(f, "parse error at byte {offset}: {context}")
                } else {
                    write!(f, "parse error: {context}")
                }
            }
            Self::Xref { object_id } => {
                if let Some(object_id) = object_id {
                    write!(
                        f,
                        "xref resolution error for object {} {}",
                        object_id.object_number, object_id.generation
                    )
                } else {
                    write!(f, "xref resolution error")
                }
            }
            Self::Font { font_name } => write!(f, "font error: {font_name}"),
            Self::Crypto => write!(f, "crypto error"),
            Self::Unsupported { feature } => write!(f, "unsupported feature: {feature}"),
            #[cfg(feature = "compress")]
            Self::Compress { reason } => write!(f, "compress error: {reason}"),
        }
    }
}

impl Error for PdfError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std_io::Error> for PdfError {
    fn from(value: std_io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ParseError> for PdfError {
    fn from(e: ParseError) -> Self {
        Self::Parse {
            offset: Some(e.offset as u64),
            context: e.message,
        }
    }
}

pub type PdfResult<T> = Result<T, PdfError>;

// ---------------------------------------------------------------------------
// Public re-exports for crate users
// ---------------------------------------------------------------------------

#[cfg(feature = "crypto")]
pub use crypto::{AuthResult, EncryptionDict, Permissions, StandardSecurityHandler};

#[cfg(feature = "text")]
pub use font::{
    BaseEncoding, Encoding, FontBBox, FontDescriptor, FontFlags, FontResolver,
    GlyphWidths, PdfFont, SimpleFont, SimpleFontSubtype, Type0Font, glyph_name_to_char,
};

pub use io::FilterError;
pub use pdmodel::{Page, PageTree};

#[cfg(feature = "text")]
pub use text::extract_text;

#[cfg(feature = "layout")]
pub use text::LayoutConfig;

// ---------------------------------------------------------------------------
// RecoveryReport — accumulates warnings from lenient loading
// ---------------------------------------------------------------------------

/// Summary of recovery actions taken during [`Document::load_lenient`].
///
/// Inspect this to understand what was wrong with the PDF and what was salvaged.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct RecoveryReport {
    /// Human-readable warning messages, one per recovery action.
    pub warnings: Vec<String>,
    /// `true` if the xref/startxref was broken and a linear scan was used.
    pub xref_recovered: bool,
    /// Number of individual objects that could not be parsed and were skipped.
    pub objects_skipped: usize,
}

impl RecoveryReport {
    /// Returns `true` if no warnings were recorded (clean load in lenient mode).
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Linear object scan — fallback when xref is missing or broken
// ---------------------------------------------------------------------------

/// Scans `bytes` sequentially for `N G obj` patterns and builds a best-effort
/// [`XRefTable`] from the found objects. Also attempts to recover the trailer.
///
/// This is the same strategy used by Adobe Reader and Java PDFBox in
/// lenient/recovery mode.
fn linear_scan_xref(bytes: &[u8], report: &mut RecoveryReport) -> XRefTable {
    use parser::xref::XRefTable;
    let mut table = XRefTable::new();
    let mut found = 0usize;

    // Scan for "N G obj" at every byte position
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        // Quick pre-filter: is this a digit?
        if !bytes[i].is_ascii_digit() { i += 1; continue; }

        // Try to parse "N G obj" at position i
        if let Some((obj_num, generation, body_start)) = try_parse_obj_header(bytes, i) {
            let id = ObjectId::new(obj_num, generation as u16);
            // Only record first occurrence (highest priority in a broken file)
            if table.get(&id).is_none() {
                table.insert_if_absent(
                    id,
                    parser::xref::XRefEntry::InUse { offset: i as u64, generation: generation as u16 },
                );
                found += 1;
            }
            // Advance past the object header to avoid re-matching
            i = body_start;
            continue;
        }

        // Also look for "trailer" keyword to recover the trailer dict
        if i + 7 <= len && &bytes[i..i+7] == b"trailer" {
            let after = &bytes[i+7..];
            let mut p = Parser::new(after);
            if let Ok(Some(CosObject::Dictionary(d))) = p.parse_object() {
                table.merge_trailer(&d);
            }
        }

        // Also scan for cross-reference stream dicts (PDF 1.5+):
        // look for /Type /XRef in a dict, then extract /Root, /Info, /Encrypt, /Size.
        // This handles PDFs that have no "trailer" keyword (only xref streams).
        if i + 5 <= len && &bytes[i..i+5] == b"/Type" {
            let after = &bytes[i+5..];
            let trimmed = skip_spaces(after);
            if trimmed.starts_with(b"/XRef") {
                // We're inside an xref stream dict — scan backwards for the opening <<
                // and try to parse the whole dict. If that fails, hunt for /Root manually.
                if let Some(dict_start) = find_dict_start_before(bytes, i) {
                    let dict_slice = &bytes[dict_start..];
                    let mut p = Parser::new(dict_slice);
                    if let Ok(Some(CosObject::Dictionary(d))) = p.parse_object() {
                        table.merge_trailer(&d);
                    }
                }
            }
        }

        i += 1;
    }

    // Last-resort: if we still have no /Root, scan for /Root N G R patterns
    // directly in the raw bytes (handles PDFs where dict parsing above fails).
    if table.trailer.get(&CosName::new(b"Root".to_vec())).is_none() {
        if let Some(root_id) = scan_bytes_for_root_ref(bytes) {
            let mut d = CosDictionary::new();
            d.insert(
                CosName::new(b"Root".to_vec()),
                CosObject::Reference(root_id),
            );
            table.merge_trailer(&d);
        }
    }

    report.warnings.push(format!("linear scan found {found} objects"));
    table
}

/// Scan raw bytes for the pattern `/Root N G R` and return the ObjectId.
fn scan_bytes_for_root_ref(bytes: &[u8]) -> Option<ObjectId> {
    let needle = b"/Root ";
    let mut pos = 0;
    while pos + needle.len() < bytes.len() {
        if &bytes[pos..pos+needle.len()] == needle {
            let rest = &bytes[pos+needle.len()..];
            if let Some((obj_num, after)) = parse_u32_prefix(rest) {
                let after = skip_spaces(after);
                if let Some((generation, after2)) = parse_u32_prefix(after) {
                    let after2 = skip_spaces(after2);
                    if after2.starts_with(b"R") {
                        return Some(ObjectId::new(obj_num, generation as u16));
                    }
                }
            }
        }
        pos += 1;
    }
    None
}

/// Walk backwards from `keyword_pos` to find the `<<` that opens
/// the dict containing the keyword. Returns the offset of `<<`.
fn find_dict_start_before(bytes: &[u8], keyword_pos: usize) -> Option<usize> {
    // Simple heuristic: scan backwards up to 2 KB for `<<`
    let search_start = keyword_pos.saturating_sub(2048);
    let slice = &bytes[search_start..keyword_pos];
    // Find the last `<<` in the slice
    slice.windows(2).rposition(|w| w == b"<<").map(|p| search_start + p)
}

/// Tries to parse an indirect object header `N G obj` starting at `pos`.
/// Returns `(object_number, generation, body_start_pos)` on success.
fn try_parse_obj_header(bytes: &[u8], pos: usize) -> Option<(u32, u32, usize)> {
    let slice = &bytes[pos..];
    let (obj_num, rest1) = parse_u32_prefix(slice)?;
    let rest1 = skip_spaces(rest1);
    let (generation, rest2) = parse_u32_prefix(rest1)?;
    let rest2 = skip_spaces(rest2);
    if rest2.len() < 3 || &rest2[..3] != b"obj" { return None; }
    if rest2.len() > 3 && rest2[3].is_ascii_alphanumeric() { return None; }
    let consumed = (slice.len() - rest2.len()) + 3;
    Some((obj_num, generation, pos + consumed))
}

fn parse_u32_prefix(bytes: &[u8]) -> Option<(u32, &[u8])> {
    if bytes.is_empty() || !bytes[0].is_ascii_digit() { return None; }
    let end = bytes.iter().position(|b| !b.is_ascii_digit()).unwrap_or(bytes.len());
    let n: u32 = std::str::from_utf8(&bytes[..end]).ok()?.parse().ok()?;
    Some((n, &bytes[end..]))
}

fn skip_spaces(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|b| !matches!(b, b' '|b'\t'|b'\r'|b'\n'|b'\x0c'|b'\x00')).unwrap_or(bytes.len());
    &bytes[end..]
}

/// If `obj` is a `CosObject::Stream` with empty `.data` (parser placeholder),
/// locate the `stream` keyword in `slice` and read the actual bytes using
/// the `/Length` entry from the stream's dictionary.
fn backfill_stream_data(obj: CosObject, slice: &[u8]) -> CosObject {
    let CosObject::Stream(mut stream) = obj else { return obj; };
    if !stream.data.is_empty() { return CosObject::Stream(stream); }

    // Get declared length
    let length = stream.dictionary
        .get(&CosName::new(b"Length".to_vec()))
        .and_then(|v: &CosObject| v.as_integer())
        .unwrap_or(0) as usize;
    if length == 0 { return CosObject::Stream(stream); }

    // Find "stream" keyword in slice
    const KW: &[u8] = b"stream";
    let stream_kw_pos = slice.windows(KW.len()).position(|w| w == KW);
    let Some(kw_pos) = stream_kw_pos else { return CosObject::Stream(stream); };
    let data_start = kw_pos + KW.len(); // position right after "stream"

    if let Ok(data) = parser::xref::read_stream_data(slice, data_start, length) {
        stream.data = data;
    }

    CosObject::Stream(stream)
}

/// Recovery fallback for `load_lenient`: when `parse_indirect_object` fails
/// (e.g. JPEG `0xFF` bytes confusing the parser), try to reconstruct the
/// stream object by:
///   1. Parsing just the dict header (`<< … >>`) at the start of `slice`.
///   2. Using `/Length` to locate and copy the raw stream body.
///
/// Returns `Some(CosObject::Stream)` on success, `None` if the slice doesn't
/// look like a stream object at all.
fn recover_stream_object(slice: &[u8]) -> Option<CosObject> {
    // Skip past "N G obj" header if present — find the opening <<
    let dict_start = slice.windows(2).position(|w| w == b"<<")?;
    let dict_slice = &slice[dict_start..];
    let mut p = Parser::new(dict_slice);
    let dict = match p.parse_object() {
        Ok(Some(CosObject::Dictionary(d))) => d,
        _ => return None,
    };

    // Must have /Length to safely extract raw binary body
    let length = dict
        .get(&CosName::new(b"Length".to_vec()))
        .and_then(|v| v.as_integer())? as usize;
    if length == 0 { return None; }

    // Find "stream" keyword after the dict
    const KW: &[u8] = b"stream";
    let kw_pos = slice.windows(KW.len()).position(|w| w == KW)?;
    let data_start = kw_pos + KW.len();
    let data = parser::xref::read_stream_data(slice, data_start, length).ok()?;

    Some(CosObject::Stream(cos::CosStream::new(dict, data)))
}

// ---------------------------------------------------------------------------
// Object store — lazy in-memory object cache keyed by ObjectId
// ---------------------------------------------------------------------------

/// Stores all loaded indirect COS objects indexed by [`ObjectId`].
///
/// Objects are parsed on demand from the raw byte slice and cached here.
/// This corresponds to `COSDocument` in Java PDFBox.
#[derive(Debug, Default, Clone)]
pub struct ObjectStore {
    objects: HashMap<ObjectId, CosObject>,
}

impl ObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a pre-parsed object.
    pub fn insert(&mut self, id: ObjectId, obj: CosObject) {
        self.objects.insert(id, obj);
    }

    /// Looks up an object by ID.
    pub fn get(&self, id: &ObjectId) -> Option<&CosObject> {
        self.objects.get(id)
    }

    /// Looks up an object by ID mutably.
    pub fn get_mut(&mut self, id: &ObjectId) -> Option<&mut CosObject> {
        self.objects.get_mut(id)
    }

    /// Returns the number of stored objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Returns `true` when the store is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Returns the maximum object number stored, or 0 if empty.
    /// Used to allocate the next free object ID.
    pub fn max_object_number(&self) -> u32 {
        self.objects.keys().map(|id| id.object_number).max().unwrap_or(0)
    }

    /// Iterates over all stored object IDs.
    pub fn keys(&self) -> impl Iterator<Item = &ObjectId> {
        self.objects.keys()
    }

    /// Iterates over all (ObjectId, CosObject) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&ObjectId, &CosObject)> {
        self.objects.iter()
    }

    /// Resolve a reference chain: if `obj` is a `Reference`, follow it through
    /// the store until a non-reference value is found (or the chain breaks).
    pub fn resolve<'a>(&'a self, obj: &'a CosObject) -> Option<&'a CosObject> {
        let mut cur = obj;
        for _ in 0..16 {
            match cur {
                CosObject::Reference(id) => {
                    cur = self.objects.get(id)?;
                }
                other => return Some(other),
            }
        }
        None // circular / too deep
    }
}

// ---------------------------------------------------------------------------
// StreamCache — on-demand stream decoding with selective caching
// ---------------------------------------------------------------------------

/// On-demand stream filter decoder with a selective in-memory cache.
///
/// Stream bytes (e.g. FlateDecode content streams) are decoded lazily — only
/// when first requested — and the result is kept in a bounded cache so
/// repeated access is O(1).
///
/// This maps to the deferred decode strategy used by Java PDFBox's
/// `COSStream.createInputStream()`.
#[derive(Debug, Default, Clone)]
pub struct StreamCache {
    /// Decoded bytes keyed by ObjectId.
    cache: HashMap<ObjectId, Arc<[u8]>>,
}

impl StreamCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the decoded bytes for `id`, decoding and caching on first call.
    ///
    /// Looks up the stream object in `store`, applies its `/Filter` pipeline,
    /// and stores the result. Returns `None` if the object is not a stream or
    /// cannot be decoded.
    pub fn get_decoded(
        &mut self,
        id: &ObjectId,
        store: &ObjectStore,
    ) -> Option<Arc<[u8]>> {
        // Fast path — already cached.
        if let Some(cached) = self.cache.get(id) {
            return Some(Arc::clone(cached));
        }

        // Decode on demand.
        let stream = store.get(id)?.as_stream()?;
        let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
        let decoded = io::decode_stream(&stream.data, filter).ok()?;
        let arc: Arc<[u8]> = decoded.into();
        self.cache.insert(*id, Arc::clone(&arc));
        Some(arc)
    }

    /// Returns the decoded bytes if already in the cache, without decoding.
    pub fn peek(&self, id: &ObjectId) -> Option<Arc<[u8]>> {
        self.cache.get(id).map(Arc::clone)
    }

    /// Evicts a single entry from the cache, freeing memory.
    pub fn evict(&mut self, id: &ObjectId) {
        self.cache.remove(id);
    }

    /// Clears the entire cache.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Returns the number of currently cached streams.
    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }
}

// ---------------------------------------------------------------------------
// Document — high-level PDF document handle
// ---------------------------------------------------------------------------

/// High-level PDF document handle.
///
/// After loading, provides access to:
/// - The merged [`XRefTable`] from all xref sections and chains.
/// - The trailer [`CosDictionary`].
/// - The [`ObjectStore`] of all eagerly loaded in-use objects.
/// - Page count and catalog reference (when available).
/// - A [`StreamCache`] for on-demand stream decoding with selective caching.
/// - The raw source bytes as a shared `Arc<[u8]>` buffer for zero-copy sharing.
///
/// Maps to `PDDocument` in Java PDFBox.
#[derive(Debug, Clone)]
pub struct Document {
    /// Total byte length of the source file.
    pub source_len: usize,
    /// Merged xref table built from all sections and Prev chains.
    pub xref: XRefTable,
    /// All eagerly loaded indirect objects.
    pub objects: ObjectStore,
    /// Shared source bytes — `Arc<[u8]>` so callers can hold a reference
    /// without copying. `None` when the document was constructed without
    /// retaining the raw buffer (e.g. after a write round-trip).
    source_bytes: Option<Arc<[u8]>>,
    /// On-demand stream decoder cache — populated lazily on first decode.
    stream_cache: Arc<Mutex<StreamCache>>,
}

impl Document {
    /// Creates a completely empty document.
    pub fn empty() -> Self {
        Self {
            source_len: 0,
            xref: XRefTable::new(),
            objects: ObjectStore::new(),
            source_bytes: None,
            stream_cache: Arc::new(Mutex::new(StreamCache::new())),
        }
    }

    /// Loads a PDF document from a file on disk.
    pub fn load<P: AsRef<Path>>(path: P) -> PdfResult<Self> {
        let file = File::open(path)?;
        Self::load_from_reader(file)
    }

    /// Loads a PDF document from any reader.
    pub fn load_from_reader<R: Read>(mut reader: R) -> PdfResult<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::load_from_bytes(&bytes)
    }

    /// Loads a PDF document from a raw byte slice.
    ///
    /// Steps performed:
    /// 1. Validate `%PDF-` header.
    /// 2. Discover `startxref` offset.
    /// 3. Parse all xref sections (table or stream), following `Prev` chains.
    /// 4. Eagerly load all in-use objects from the xref table into the object store.
    pub fn load_from_bytes(bytes: &[u8]) -> PdfResult<Self> {
        // Step 1 — header check.
        if !looks_like_pdf_header(bytes) {
            return Err(PdfError::Parse {
                offset: Some(0),
                context: "missing %PDF- header within first 1024 bytes".to_string(),
            });
        }

        // Step 2 & 3 — xref discovery and parsing.
        let xref = parser::xref::load_xref(bytes)?;

        // Step 4a — load all InUse (direct byte-offset) objects.
        let mut objects = ObjectStore::new();
        for (id, entry) in xref.iter() {
            if let XRefEntry::InUse { offset, .. } = entry {
                let offset = *offset as usize;
                if offset == 0 || offset >= bytes.len() {
                    continue;
                }
                let slice = &bytes[offset..];
                let mut parser = Parser::new(slice);
                match parser.parse_indirect_object() {
                    Ok(Some((_parsed_id, obj))) => {
                        // If the object is a stream with empty data (parser placeholder),
                        // backfill the actual bytes from the raw slice.
                        let obj = backfill_stream_data(obj, slice);
                        objects.insert(id.clone(), obj);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        // Non-fatal: skip malformed individual objects.
                    }
                }
            }
        }

        // Step 4b — expand Compressed objects (PDF 1.5+ ObjStm).
        // Collect all compressed entries first to avoid borrowing `xref` while mutating `objects`.
        let compressed: Vec<(ObjectId, u32, u32)> = xref
            .iter()
            .filter_map(|(id, entry)| {
                if let XRefEntry::Compressed { stream_object_number, index_in_stream } = entry {
                    Some((id.clone(), *stream_object_number, *index_in_stream))
                } else {
                    None
                }
            })
            .collect();

        // Build a cache of decoded ObjStm streams to avoid re-decoding the same stream.
        let mut objstm_cache: std::collections::HashMap<u32, parser::ObjectStream> =
            std::collections::HashMap::new();

        for (id, stream_obj_num, _index_in_stream) in compressed {
            // Skip if already loaded (e.g. from a Prev update).
            if objects.get(&id).is_some() {
                continue;
            }

            // Decode and cache the ObjStm on first use.
            if !objstm_cache.contains_key(&stream_obj_num) {
                let stream_id = ObjectId::new(stream_obj_num, 0);
                let cos_stream = match objects.get(&stream_id).and_then(|o| o.as_stream()) {
                    Some(s) => s.clone(),
                    None => continue,
                };
                let filter_obj = cos_stream.dictionary.get(&CosName::new(b"Filter".to_vec())).cloned();
                let decoded = io::decode_stream(&cos_stream.data, filter_obj.as_ref())
                    .unwrap_or_else(|_| cos_stream.data.clone());
                if let Some(s) = parser::ObjectStream::from_stream(&cos_stream.dictionary, decoded) {
                    objstm_cache.insert(stream_obj_num, s);
                }
            }

            let obj_num = id.object_number;
            if let Some(objstm) = objstm_cache.get(&stream_obj_num) {
                // get_object takes the object number, returns raw bytes
                if let Some(obj_bytes) = objstm.get_object(obj_num) {
                    let mut p = Parser::new(obj_bytes);
                    if let Ok(Some(obj)) = p.parse_object() {
                        objects.insert(id, obj);
                    }
                }
            }
        }

        Ok(Self {
            source_len: bytes.len(),
            xref,
            objects,
            source_bytes: Some(Arc::from(bytes)),
            stream_cache: Arc::new(Mutex::new(StreamCache::new())),
        })
    }

    /// Returns the raw input length.
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// Returns a shared reference to the original source bytes, if retained.
    ///
    /// The `Arc<[u8]>` can be cloned cheaply — all clones share the same
    /// underlying allocation with no copy. Returns `None` for documents that
    /// were constructed without retaining the buffer (e.g. after a save/reload
    /// round-trip through a `Cursor`).
    pub fn source_bytes(&self) -> Option<Arc<[u8]>> {
        self.source_bytes.as_ref().map(Arc::clone)
    }

    /// Lazily resolves an indirect object by `id`.
    ///
    /// First checks the in-memory [`ObjectStore`]. If not found and the
    /// source bytes are retained (`source_bytes` is `Some`), attempts to
    /// locate the object via the xref table and parse it on demand, then
    /// inserts it into the store for future O(1) access.
    ///
    /// This implements the *lazy-load* ownership model: objects not accessed
    /// at load time are only parsed when first requested.
    pub fn get_object(&mut self, id: &ObjectId) -> Option<&CosObject> {
        // Fast path — already in store.
        if self.objects.get(id).is_some() {
            return self.objects.get(id);
        }

        // Lazy path — parse from raw source bytes via xref.
        let entry = self.xref.get(id)?.clone();
        let raw: Arc<[u8]> = self.source_bytes.as_ref()?.clone();

        match entry {
            XRefEntry::InUse { offset, .. } => {
                let offset = offset as usize;
                if offset == 0 || offset >= raw.len() {
                    return None;
                }
                let slice = &raw[offset..];
                let mut p = Parser::new(slice);
                if let Ok(Some((_pid, obj))) = p.parse_indirect_object() {
                    let obj = backfill_stream_data(obj, slice);
                    self.objects.insert(*id, obj);
                }
            }
            XRefEntry::Compressed { stream_object_number, .. } => {
                // Expand the parent ObjStm (already loaded in Step 4b) — just
                // re-check the store. If still absent, nothing we can do here.
                let stream_id = ObjectId::new(stream_object_number, 0);
                let cos_stream = self.objects.get(&stream_id)?.as_stream()?.clone();
                let filter = cos_stream.dictionary
                    .get(&CosName::new(b"Filter".to_vec())).cloned();
                let decoded = io::decode_stream(&cos_stream.data, filter.as_ref()).ok()?;
                let objstm = parser::ObjectStream::from_stream(&cos_stream.dictionary, decoded)?;
                if let Some(obj_bytes) = objstm.get_object(id.object_number) {
                    let mut p = Parser::new(obj_bytes);
                    if let Ok(Some(obj)) = p.parse_object() {
                        self.objects.insert(*id, obj);
                    }
                }
            }
            XRefEntry::Free { .. } => return None,
        }

        self.objects.get(id)
    }

    /// Returns decoded stream bytes for the object at `id`, using the
    /// [`StreamCache`] for on-demand decoding with selective caching.
    ///
    /// On first call the raw stream data is decoded through its `/Filter`
    /// pipeline and the result is stored as `Arc<[u8]>`. Subsequent calls
    /// return the cached `Arc` — a cheap reference-count increment, no copy.
    ///
    /// Returns `None` if `id` is not a stream object or decoding fails.
    pub fn get_decoded_stream(&self, id: &ObjectId) -> Option<Arc<[u8]>> {
        self.stream_cache
            .lock()
            .ok()?
            .get_decoded(id, &self.objects)
    }

    /// Evicts a cached decoded stream from the [`StreamCache`], freeing memory.
    pub fn evict_stream(&self, id: &ObjectId) {
        if let Ok(mut cache) = self.stream_cache.lock() {
            cache.evict(id);
        }
    }

    /// Returns the number of currently cached decoded streams.
    pub fn cached_stream_count(&self) -> usize {
        self.stream_cache
            .lock()
            .map(|c| c.cached_count())
            .unwrap_or(0)
    }

    /// Returns the trailer dictionary from the merged xref.
    pub fn trailer(&self) -> &CosDictionary {
        &self.xref.trailer
    }

    /// Returns the catalog object reference from the trailer, if present.
    pub fn catalog_ref(&self) -> Option<ObjectId> {
        self.xref
            .trailer
            .get(&CosName::root())
            .and_then(|v| v.as_reference())
    }

    /// Returns the catalog dictionary, if it can be resolved.
    pub fn catalog(&self) -> Option<&cos::CosDictionary> {
        let cat_id = self.catalog_ref()?;
        self.objects.get(&cat_id)?.as_dictionary()
    }

    #[cfg(feature = "outline")]
    pub fn outline(&self) -> Option<outline::DocumentOutline<'_>> {
        outline::OutlineExt::outline(self)
    }

    #[cfg(feature = "forms")]
    pub fn acro_form(&self) -> Option<forms::PdAcroForm<'_>> {
        let catalog = self.catalog()?;
        catalog
            .get(&CosName::new(b"AcroForm".to_vec()))
            .and_then(|v| {
                if let Some(id) = v.as_reference() {
                    self.objects.get(&id)?.as_dictionary()
                } else {
                    v.as_dictionary()
                }
            })
            .map(|dict| forms::PdAcroForm::new(dict, &self.objects))
    }

    /// Returns the number of objects in the object store.
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Builds and returns the page tree for this document.
    ///
    /// Traverses the catalog → Pages tree, resolving all leaf pages from the
    /// object store. Returns an error if the catalog or page tree is missing
    /// or malformed.
    pub fn pages(&self) -> PdfResult<pdmodel::PageTree<'_>> {
        let catalog = self.catalog().ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "cannot resolve catalog dictionary".to_string(),
        })?;
        pdmodel::PageTree::new(catalog, &self.objects)
    }

    /// Returns the total page count by building the page tree.
    ///
    /// Returns `0` if the page tree cannot be resolved.
    pub fn page_count(&self) -> usize {
        self.pages().map(|t| t.count()).unwrap_or(0)
    }

    /// Loads a PDF document from a byte slice, recovering as much as possible
    /// from structural errors (broken xref, malformed objects, missing header).
    ///
    /// Unlike [`load_from_bytes`], this method:
    /// - Accepts files whose `%PDF-` header is missing or at an offset > 0.
    /// - Falls back to a linear scan when `startxref` / xref parsing fails.
    /// - Skips individual objects that cannot be parsed rather than aborting.
    /// - Always returns `Ok`, accumulating warnings in the [`RecoveryReport`].
    ///
    /// Maps to Java PDFBox `PDFParser` lenient mode.
    pub fn load_lenient(bytes: &[u8]) -> (Self, RecoveryReport) {
        let mut report = RecoveryReport::default();

        // ---- 1. Header leniency: warn but continue if missing ----
        if !looks_like_pdf_header(bytes) {
            report.warnings.push(
                "missing or non-standard %PDF- header — attempting recovery".into(),
            );
        }

        // ---- 2. Try normal xref path; on failure, fall back to linear scan ----
        let xref = match parser::xref::load_xref(bytes) {
            Ok(x) => x,
            Err(e) => {
                report.warnings.push(format!(
                    "xref/startxref parse failed ({e}) — falling back to linear object scan"
                ));
                report.xref_recovered = true;
                linear_scan_xref(bytes, &mut report)
            }
        };

        // ---- 3. Eagerly parse all in-use objects, skipping failures ----
        let mut objects = ObjectStore::new();
        let mut skipped = 0usize;
        for (id, entry) in xref.iter() {
            if let XRefEntry::InUse { offset, .. } = entry {
                let offset = *offset as usize;
                if offset == 0 || offset >= bytes.len() {
                    skipped += 1;
                    continue;
                }
                let slice = &bytes[offset..];
                let mut p = Parser::new(slice);
                match p.parse_indirect_object() {
                    Ok(Some((_pid, obj))) => {
                        let obj = backfill_stream_data(obj, slice);
                        objects.insert(id.clone(), obj);
                    }
                    Ok(None) => { skipped += 1; }
                    Err(_e) => {
                        // Parser failed — try manual stream recovery (handles
                        // objects whose binary body (e.g. JPEG 0xFF bytes)
                        // confuses the PDF tokeniser).
                        if let Some(recovered) = recover_stream_object(slice) {
                            objects.insert(id.clone(), recovered);
                        } else {
                            skipped += 1;
                            report.warnings.push(format!(
                                "skipped object {} {}: {}",
                                id.object_number, id.generation, _e
                            ));
                        }
                    }
                }
            }
        }
        if skipped > 0 {
            report.objects_skipped = skipped;
        }

        // ---- 4. Expand ObjStm (object streams) ----
        // Objects stored inside ObjStm are not visible to the linear scan.
        // We must decompress each ObjStm and inject the embedded objects
        // into the store so the Page tree, fonts, etc. become accessible.
        let objstm_ids: Vec<ObjectId> = objects
            .iter()
            .filter_map(|(id, obj)| {
                if let CosObject::Stream(s) = obj {
                    let is_objstm = s.dictionary
                        .get(&CosName::new(b"Type".to_vec()))
                        .map(|v| matches!(v, CosObject::Name(n) if n.as_str() == Some("ObjStm")))
                        .unwrap_or(false);
                    if is_objstm { return Some(*id); }
                }
                None
            })
            .collect();

        for stm_id in &objstm_ids {
            let stream_clone = match objects.get(stm_id) {
                Some(CosObject::Stream(s)) => s.clone(),
                _ => continue,
            };
            // Decode (FlateDecode) the ObjStm data.
            let filter = stream_clone.dictionary
                .get(&CosName::new(b"Filter".to_vec()))
                .cloned();
            let decoded = match io::decode_stream(&stream_clone.data, filter.as_ref()) {
                Ok(d) => d,
                Err(_) => continue,
            };
            // Parse the object stream preamble.
            let objstm = match parser::ObjectStream::from_stream(
                &stream_clone.dictionary, decoded,
            ) {
                Some(os) => os,
                None => continue,
            };
            // Extract each embedded object and insert into the store.
            for obj_num in objstm.object_numbers() {
                let id = ObjectId::new(obj_num, 0);
                if objects.get(&id).is_some() {
                    continue; // already loaded
                }
                if let Some(obj_bytes) = objstm.get_object(obj_num) {
                    let mut p = Parser::new(obj_bytes);
                    match p.parse_object() {
                        Ok(Some(parsed)) => {
                            objects.insert(id, parsed);
                        }
                        _ => {} // skip unparseable
                    }
                }
            }
        }

        let doc = Self {
            source_len: bytes.len(),
            xref,
            objects,
            source_bytes: Some(Arc::from(bytes)),
            stream_cache: Arc::new(Mutex::new(StreamCache::new())),
        };
        (doc, report)
    }

    /// Saves the document to a file path using a full-rewrite save.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> std_io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);
        self.save_to(&mut writer)
    }

    /// Saves the document to a mutable writer using a full-rewrite save.
    pub fn save_to<W: std::io::Write + std::io::Seek>(&self, writer: &mut W) -> std_io::Result<()> {
        let mut doc_writer = writer::Writer::new(writer);
        doc_writer.write_document(self)
    }

    /// Appends an incremental update to `out`, writing only the changed objects.
    ///
    /// `original` must be the exact bytes this document was loaded from.
    /// `changed` maps each new or modified `ObjectId` to its new body.
    ///
    /// See [`writer::IncrementalWriter`] for full details.
    pub fn save_incremental<W: std_io::Write>(
        &self,
        original: &[u8],
        changed: &std::collections::BTreeMap<PdfObjectId, cos::CosObject>,
        out: &mut W,
    ) -> std_io::Result<()> {
        writer::IncrementalWriter::write_update(original, self, changed, out)
    }

    // ── Compression API (Bonus 11) ────────────────────────────────────────────

    /// Runs the multi-pass PDF compression pipeline in-place.
    ///
    /// This is a convenience method equivalent to calling [`compress::compress`]
    /// directly. Requires the `compress` feature flag.
    ///
    /// # Example
    /// ```no_run
    /// # use rust_pdfbox::{Document, compress::{CompressOptions, CompressionMode}};
    /// let mut doc = Document::load("input.pdf").unwrap();
    /// let report = doc.compress(CompressOptions::for_mode(CompressionMode::Recommended)).unwrap();
    /// println!("{}", report.summary());
    /// doc.save("output.pdf").unwrap();
    /// ```
    #[cfg(feature = "compress")]
    pub fn compress(
        &mut self,
        opts: compress::CompressOptions,
    ) -> PdfResult<compress::CompressReport> {
        compress::compress(self, opts)
    }

    // ── Document mutation helpers (used by compress module) ───────────────────

    /// Iterates all (ObjectId, &CosObject) pairs in the object store.
    pub fn objects(&self) -> impl Iterator<Item = (ObjectId, &CosObject)> {
        self.objects.iter().map(|(id, obj)| (*id, obj))
    }

    /// Returns a shared reference to the object with `id`, if present.
    pub fn get_object_ref(&self, id: ObjectId) -> Option<&CosObject> {
        self.objects.get(&id)
    }

    /// Mutates the object at `id` in-place via a closure.
    ///
    /// The closure receives `&mut CosObject` and can modify it freely.
    /// Does nothing if `id` is not in the store.
    pub fn mutate_object<F>(&mut self, id: ObjectId, f: F)
    where
        F: FnOnce(&mut CosObject),
    {
        if let Some(obj) = self.objects.objects.get_mut(&id) {
            f(obj);
        }
    }

    /// Removes and returns the object at `id` from the store, if present.
    pub fn remove_object(&mut self, id: ObjectId) -> Option<CosObject> {
        self.objects.objects.remove(&id)
    }

    /// Inserts `obj` at `id` into the object store (overwrites any existing entry).
    pub fn insert_object(&mut self, id: ObjectId, obj: CosObject) {
        self.objects.insert(id, obj);
    }

    /// Allocates a fresh `ObjectId` with generation 0, one above the current maximum.
    pub fn allocate_object_id(&mut self) -> ObjectId {
        let next = self.objects.max_object_number() + 1;
        let id = ObjectId::new(next, 0);
        self.objects.insert(id, CosObject::Null); // Reserve it
        id
    }

    /// Returns the `ObjectId` of the document catalog, if resolvable.
    pub fn catalog_id(&self) -> Option<ObjectId> {
        self.catalog_ref()
    }

    /// Returns the `ObjectId` of the trailer `/Info` dict, if present.
    pub fn info_id(&self) -> Option<ObjectId> {
        self.xref
            .trailer
            .get(&CosName::new(b"Info".to_vec()))
            .and_then(|v| v.as_reference())
    }

    /// Returns the `ObjectId` of the `/Encrypt` dict in the trailer, if present.
    pub fn encryption_dict_id(&self) -> Option<ObjectId> {
        self.xref
            .trailer
            .get(&CosName::new(b"Encrypt".to_vec()))
            .and_then(|v| v.as_reference())
    }

    /// Returns the PDF major and minor version from the header (e.g. `(1, 7)`).
    ///
    /// Reads from the stored version if set; otherwise parses from source bytes.
    pub fn pdf_version(&self) -> (u8, u8) {
        self.xref.pdf_version.unwrap_or((1, 4))
    }

    /// Overrides the PDF version that will be written to the header on the
    /// next save (e.g. `set_version(1, 4)` for PDF 1.4).
    pub fn set_version(&mut self, major: u8, minor: u8) {
        self.xref.pdf_version = Some((major, minor));
    }

    /// Returns an iterator over the `ObjectId`s of all page dictionaries in
    /// document order.
    pub fn page_object_ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        // Walk the page tree in order, emitting leaf page IDs.
        // We use a simple iterative stack traversal.
        let catalog_id = self.catalog_ref();
        let pages_id = catalog_id.and_then(|id| {
            let cat = self.objects.get(&id)?.as_dictionary()?;
            cat.get(&CosName::new(b"Pages".to_vec()))?.as_reference()
        });

        let mut stack: Vec<ObjectId> = pages_id.into_iter().collect();
        let mut result: Vec<ObjectId> = Vec::new();

        while let Some(node_id) = stack.pop() {
            let obj = match self.objects.get(&node_id) {
                Some(o) => o,
                None => continue,
            };
            let dict = match obj.as_dictionary() {
                Some(d) => d,
                None => continue,
            };
            let type_name = dict
                .get(&CosName::new(b"Type".to_vec()))
                .and_then(|v| v.as_name())
                .map(|n| n.as_bytes().to_vec());

            match type_name.as_deref() {
                Some(b"Page") => result.push(node_id),
                Some(b"Pages") | None => {
                    // Push kids in reverse order so the first kid is processed first.
                    if let Some(CosObject::Array(kids)) =
                        dict.get(&CosName::new(b"Kids".to_vec()))
                    {
                        for kid in kids.iter().rev() {
                            if let Some(kid_id) = kid.as_reference() {
                                stack.push(kid_id);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        result.into_iter()
    }

    /// Returns the `ObjectId` of the `/Resources` dict for `page_id`, if any.
    ///
    /// Returns the Resources ID whether the Resources dict is inline or a
    /// reference to an indirect object.
    pub fn page_resources_id(&self, page_id: ObjectId) -> Option<ObjectId> {
        let page_obj = self.objects.get(&page_id)?;
        let page_dict = page_obj.as_dictionary()?;
        match page_dict.get(&CosName::new(b"Resources".to_vec()))? {
            CosObject::Reference(id) => Some(*id),
            _ => None, // inline resources dict — handled separately
        }
    }

    /// Returns the decoded content stream bytes for `page_id`.
    ///
    /// Supports both single-stream (`/Contents N G R`) and multi-stream
    /// (`/Contents [N G R …]`) pages. Streams are decoded through their
    /// `/Filter` pipeline and concatenated in order.
    pub fn page_content_bytes(&self, page_id: ObjectId) -> PdfResult<Vec<u8>> {
        let page_obj = self.objects.get(&page_id).ok_or_else(|| PdfError::Xref {
            object_id: Some(page_id),
        })?;
        let page_dict = page_obj.as_dictionary().ok_or_else(|| PdfError::Parse {
            offset: None,
            context: format!("page object {page_id:?} is not a dictionary"),
        })?;

        let contents = match page_dict.get(&CosName::new(b"Contents".to_vec())) {
            Some(v) => v.clone(),
            None => return Ok(Vec::new()),
        };

        let stream_ids: Vec<ObjectId> = match &contents {
            CosObject::Reference(id) => vec![*id],
            CosObject::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_reference())
                .collect(),
            _ => return Ok(Vec::new()),
        };

        let mut all_bytes = Vec::new();
        for stream_id in stream_ids {
            let stream_obj = match self.objects.get(&stream_id) {
                Some(o) => o,
                None => continue,
            };
            let stream = match stream_obj.as_stream() {
                Some(s) => s,
                None => continue,
            };
            let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
            let decoded = io::decode_stream(&stream.data, filter)
                .unwrap_or_else(|_| stream.data.clone());
            all_bytes.extend_from_slice(&decoded);
            all_bytes.push(b' '); // separator between concatenated streams
        }

        Ok(all_bytes)
    }

    /// Records that `object_id` is now stored inside an ObjStm container.
    ///
    /// Updates the xref entry for `object_id` to
    /// `XRefEntry::Compressed { stream_object_number, index_in_stream }`.
    pub fn mark_compressed(
        &mut self,
        object_id: ObjectId,
        stream_object_number: u32,
        index_in_stream: u32,
    ) {
        self.xref.insert_if_absent(
            object_id,
            XRefEntry::Compressed {
                stream_object_number,
                index_in_stream,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn looks_like_pdf_header(bytes: &[u8]) -> bool {
    const HEADER: &[u8] = b"%PDF-";
    const SEARCH_LIMIT: usize = 1024;

    if bytes.len() < HEADER.len() {
        return false;
    }

    let end = bytes.len().min(SEARCH_LIMIT);
    bytes[..end].windows(HEADER.len()).any(|w| w == HEADER)
}

// ---------------------------------------------------------------------------
// Test helpers — shared across all in-crate test modules
// ---------------------------------------------------------------------------

/// Shared test fixture builders accessible from sub-module tests via `crate::tests`.
#[cfg(test)]
pub mod tests {
    use super::*;

    /// Builds a minimal but structurally valid PDF byte sequence suitable for
    /// testing `Document::load_from_bytes`.
    pub fn minimal_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let obj1_offset = pdf.len() as u64;
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let obj2_offset = pdf.len() as u64;
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let xref_offset = pdf.len();
        let e1 = format!("{:010} 00000 n \r\n", obj1_offset);
        let e2 = format!("{:010} 00000 n \r\n", obj2_offset);
        pdf.extend_from_slice(b"xref\n");
        pdf.extend_from_slice(b"0 3\n");
        pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        pdf.extend_from_slice(e1.as_bytes());
        pdf.extend_from_slice(e2.as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
        pdf
    }

    // ── Original lib tests (kept in-module for compatibility) ─────────────────

    #[test]
    fn loads_bytes_with_pdf_header() {
        // A bare header without proper xref will fail at xref stage, not header stage.
        let result = Document::load_from_bytes(b"%PDF-1.7\n%%EOF\nstartxref\n0\n%%EOF\n");
        // We expect a parse error (no valid xref at offset 0), not a header error.
        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_pdf_bytes() {
        let err = Document::load_from_bytes(b"not a pdf").unwrap_err();
        assert!(matches!(err, PdfError::Parse { .. }));
    }

    #[test]
    fn loads_minimal_pdf() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        assert_eq!(doc.source_len(), pdf.len());
    }

    #[test]
    fn minimal_pdf_has_catalog_ref() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        let cat_ref = doc.catalog_ref();
        assert_eq!(cat_ref, Some(ObjectId::new(1, 0)));
    }

    #[test]
    fn minimal_pdf_catalog_resolved() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        let catalog = doc.catalog();
        assert!(catalog.is_some(), "catalog should resolve");
        let cat = catalog.unwrap();
        assert_eq!(
            cat.get_name(&CosName::type_name()),
            Some(&CosName::new(b"Catalog".to_vec()))
        );
    }

    #[test]
    fn minimal_pdf_object_count() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        // Expect at least objects 1 and 2 loaded (obj 0 is free).
        assert!(doc.object_count() >= 2);
    }

    #[test]
    fn trailer_has_size() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        assert_eq!(
            doc.trailer().get_int(&CosName::new(b"Size".to_vec())),
            Some(3)
        );
    }

    /// Builds a minimal PDF with 2 real pages for phase-2 tests.
    fn two_page_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();

        // page 1
        let p1_off = pdf.len() as u64;
        pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n");
        // page 2
        let p2_off = pdf.len() as u64;
        pdf.extend_from_slice(b"4 0 obj\n<< /Type /Page /MediaBox [0 0 595 842] >>\nendobj\n");
        // Pages
        let pages_off = pdf.len() as u64;
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>\nendobj\n");
        // Catalog
        let cat_off = pdf.len() as u64;
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let xref_off = pdf.len();
        let e1 = format!("{:010} 00000 n \r\n", cat_off);
        let e2 = format!("{:010} 00000 n \r\n", pages_off);
        let e3 = format!("{:010} 00000 n \r\n", p1_off);
        let e4 = format!("{:010} 00000 n \r\n", p2_off);
        pdf.extend_from_slice(b"xref\n0 5\n");
        pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        pdf.extend_from_slice(e1.as_bytes());
        pdf.extend_from_slice(e2.as_bytes());
        pdf.extend_from_slice(e3.as_bytes());
        pdf.extend_from_slice(e4.as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
        pdf
    }

    #[test]
    fn document_page_count() {
        let pdf = two_page_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        assert_eq!(doc.page_count(), 2);
    }

    #[test]
    fn document_pages_iter() {
        let pdf = two_page_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        let tree = doc.pages().unwrap();
        let pages: Vec<_> = tree.iter().collect();
        assert_eq!(pages.len(), 2);
        let mb0 = pages[0].media_box().unwrap();
        assert_eq!(mb0.width(), 612.0);
        let mb1 = pages[1].media_box().unwrap();
        assert_eq!(mb1.width(), 595.0);
    }

    #[test]
    fn document_pages_get_by_index() {
        let pdf = two_page_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        let tree = doc.pages().unwrap();
        let p0 = tree.get(0).unwrap();
        assert_eq!(p0.rotation(), 0);
        assert!(tree.get(2).is_none());
    }

    #[test]
    fn round_trip_save_and_reload() {
        let original_pdf = two_page_pdf();
        let original_doc = Document::load_from_bytes(&original_pdf).unwrap();
        assert_eq!(original_doc.page_count(), 2);

        let mut saved_buffer = std::io::Cursor::new(Vec::new());
        original_doc.save_to(&mut saved_buffer).unwrap();

        let reloaded_doc = Document::load_from_bytes(saved_buffer.get_ref()).unwrap();
        assert_eq!(reloaded_doc.page_count(), 2);
        let reloaded_pages = reloaded_doc.pages().unwrap();
        let page1_width = reloaded_pages.get(1).unwrap().media_box().unwrap().width();
        assert!((page1_width - 595.0).abs() < 1e-6);
    }

    #[test]
    fn incremental_save_preserves_existing_pages() {
        let original_pdf = two_page_pdf();
        let doc = Document::load_from_bytes(&original_pdf).unwrap();
        assert_eq!(doc.page_count(), 2);

        // Append a new integer object (obj 5) incrementally
        let mut changed = std::collections::BTreeMap::new();
        changed.insert(ObjectId::new(5, 0), CosObject::Integer(99));

        let mut out = Vec::new();
        doc.save_incremental(&original_pdf, &changed, &mut out).unwrap();

        // Updated document must still have 2 pages
        let updated = Document::load_from_bytes(&out).unwrap();
        assert_eq!(updated.page_count(), 2);
        // New object must be visible
        assert_eq!(
            updated.objects.get(&ObjectId::new(5, 0)),
            Some(&CosObject::Integer(99))
        );
    }

    #[test]
    fn source_bytes_retained_as_arc() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();

        // source_bytes() returns Some — bytes are retained
        let arc = doc.source_bytes().expect("source bytes should be retained");
        assert_eq!(arc.len(), pdf.len());
        assert_eq!(&arc[..5], b"%PDF-");

        // Cheap clone — same allocation, no copy
        let arc2 = doc.source_bytes().unwrap();
        assert!(Arc::ptr_eq(&arc, &arc2));
    }

    #[test]
    fn get_object_lazy_load_from_xref() {
        let pdf = two_page_pdf();
        // Load without expanding specific objects, then lazy-resolve one
        let mut doc = Document::load_from_bytes(&pdf).unwrap();

        // Pages object (id 2 0) should be accessible — either eagerly loaded
        // or lazily resolved on first call
        let pages_id = ObjectId::new(2, 0);
        let obj = doc.get_object(&pages_id);
        assert!(obj.is_some(), "Pages object should be resolvable");
        let dict = obj.unwrap().as_dictionary().unwrap();
        assert_eq!(
            dict.get_name(&CosName::type_name()),
            Some(&CosName::new(b"Pages".to_vec()))
        );
    }

    #[test]
    fn get_object_returns_none_for_missing() {
        let pdf = minimal_pdf();
        let mut doc = Document::load_from_bytes(&pdf).unwrap();
        // Object 999 does not exist
        assert!(doc.get_object(&ObjectId::new(999, 0)).is_none());
    }

    #[test]
    fn object_store_resolve_follows_references() {
        let pdf = two_page_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();

        // Catalog /Pages is a Reference — resolve it
        let catalog = doc.catalog().unwrap();
        let pages_ref = catalog.get(&CosName::new(b"Pages".to_vec())).unwrap();
        assert!(matches!(pages_ref, CosObject::Reference(_)));

        // resolve() follows the reference
        let resolved = doc.objects.resolve(pages_ref);
        assert!(resolved.is_some());
        let dict = resolved.unwrap().as_dictionary().unwrap();
        assert_eq!(
            dict.get_name(&CosName::type_name()),
            Some(&CosName::new(b"Pages".to_vec()))
        );
    }

    #[test]
    fn object_store_resolve_non_reference_is_identity() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        let int_obj = CosObject::Integer(42);
        // Non-reference resolves to itself
        let resolved = doc.objects.resolve(&int_obj);
        assert_eq!(resolved, Some(&CosObject::Integer(42)));
    }

    #[cfg(feature = "compress")]
    #[test]
    fn stream_cache_decode_on_demand() {
        // Build a PDF with a FlateDecode content stream
        let content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET";
        let compressed = {
            use std::io::Write;
            let mut enc = flate2::write::ZlibEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            );
            enc.write_all(content).unwrap();
            enc.finish().unwrap()
        };

        let mut pdf = b"%PDF-1.4\n".to_vec();
        let stream_off = pdf.len() as u64;
        let dict_str = format!(
            "5 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            compressed.len()
        );
        pdf.extend_from_slice(dict_str.as_bytes());
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        let page_off = pdf.len() as u64;
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Contents 5 0 R >>\nendobj\n",
        );
        let pages_off = pdf.len() as u64;
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        let cat_off = pdf.len() as u64;
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let xref_off = pdf.len();
        let e1 = format!("{:010} 00000 n \r\n", cat_off);
        let e2 = format!("{:010} 00000 n \r\n", pages_off);
        let e3 = format!("{:010} 00000 n \r\n", page_off);
        let e5 = format!("{:010} 00000 n \r\n", stream_off);
        pdf.extend_from_slice(b"xref\n0 6\n");
        pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        pdf.extend_from_slice(e1.as_bytes());
        pdf.extend_from_slice(e2.as_bytes());
        pdf.extend_from_slice(e3.as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \r\n"); // obj 4 free
        pdf.extend_from_slice(e5.as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());

        let doc = Document::load_from_bytes(&pdf).unwrap();

        // Initially nothing is cached
        assert_eq!(doc.cached_stream_count(), 0);

        // Decode on demand
        let stream_id = ObjectId::new(5, 0);
        let decoded = doc.get_decoded_stream(&stream_id);
        assert!(decoded.is_some(), "should decode FlateDecode stream");
        let decoded_bytes = decoded.unwrap();
        assert_eq!(decoded_bytes.as_ref(), content.as_slice());

        // Now cached
        assert_eq!(doc.cached_stream_count(), 1);

        // Second call returns same Arc — no re-decode
        let decoded2 = doc.get_decoded_stream(&stream_id).unwrap();
        assert!(Arc::ptr_eq(&decoded_bytes, &decoded2));

        // Evict and verify cache is cleared
        doc.evict_stream(&stream_id);
        assert_eq!(doc.cached_stream_count(), 0);
    }

    #[test]
    fn stream_cache_returns_none_for_non_stream() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();
        // Object 1 is a Dictionary, not a stream
        let result = doc.get_decoded_stream(&ObjectId::new(1, 0));
        assert!(result.is_none());
    }

    #[test]
    fn arc_source_bytes_can_be_shared_cheaply() {
        let pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&pdf).unwrap();

        let a1 = doc.source_bytes().unwrap();
        let a2 = doc.source_bytes().unwrap();
        let a3 = a1.clone(); // cheap — same allocation

        // All three point to the same allocation
        assert!(Arc::ptr_eq(&a1, &a2));
        assert!(Arc::ptr_eq(&a1, &a3));
        assert_eq!(Arc::strong_count(&a1), 4); // doc internal + a1 + a2 + a3
    }

    #[test]
    fn incremental_save_starts_with_original_bytes() {
        let original_pdf = minimal_pdf();
        let doc = Document::load_from_bytes(&original_pdf).unwrap();
        let changed = std::collections::BTreeMap::new();

        let mut out = Vec::new();
        doc.save_incremental(&original_pdf, &changed, &mut out).unwrap();

        // First bytes must be identical to original
        assert_eq!(&out[..original_pdf.len()], original_pdf.as_slice());
    }
}
