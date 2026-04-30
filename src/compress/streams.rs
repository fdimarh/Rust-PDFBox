//! Pass 2 — FlateDecode stream re-compression.
//!
//! Iterates every `CosObject::Stream` in the `ObjectStore` and re-encodes it
//! using the best available DEFLATE backend:
//!
//! - **`compress-zopfli`** feature: Zopfli optimal DEFLATE (3–8% better than
//!   zlib level 9, ~100× slower). Same codec as used by ilovepdf for non-image
//!   streams.
//! - Otherwise: `flate2` at `Compression::best()` (level 9, fast).
//!
//! Accepts new encoding only when `new_len < original_len` (never inflate).
//! Image XObjects that are already `DCTDecode` or `JPXDecode` are skipped.
//!
//! **Crates:** [`flate2`](https://crates.io/crates/flate2) `1.x`,
//!             [`zopfli`](https://crates.io/crates/zopfli) `0.8` (optional)

use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

use crate::cos::{CosName, CosObject};
use crate::{Document, PdfResult};
use super::CompressOptions;

// ---------------------------------------------------------------------------
// Public report
// ---------------------------------------------------------------------------

/// Statistics returned by [`run`].
#[derive(Debug, Default)]
pub struct StreamsReport {
    /// Number of streams successfully re-encoded to a smaller representation.
    pub streams_recompressed: usize,
    /// Approximate bytes saved across all re-compressed streams.
    pub bytes_saved: usize,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Re-compress all eligible streams to FlateDecode level 9.
pub fn run(doc: &mut Document, opts: &CompressOptions) -> PdfResult<StreamsReport> {
    let mut report = StreamsReport::default();

    // Collect stream object IDs up-front to avoid borrow conflicts.
    let stream_ids: Vec<crate::cos::ObjectId> = doc
        .objects()
        .filter_map(|(id, obj)| {
            if matches!(obj, CosObject::Stream(_)) { Some(id) } else { None }
        })
        .collect();

    for id in stream_ids {
        if let Err(_) = try_recompress_stream(doc, id, opts, &mut report) {
            if !opts.skip_on_decode_error {
                return Err(crate::PdfError::Compress {
                    reason: format!("stream re-compression failed on object {:?}", id),
                });
            }
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Per-stream logic
// ---------------------------------------------------------------------------

fn try_recompress_stream(
    doc: &mut Document,
    id: crate::cos::ObjectId,
    opts: &CompressOptions,
    report: &mut StreamsReport,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if this is an image XObject with DCT/JPX filter — skip.
    let should_skip = is_image_with_dct_or_jpx(doc, id);
    if should_skip {
        return Ok(());
    }

    // Decode the stream using the existing io pipeline.
    let decoded = match doc.get_decoded_stream(&id) {
        Some(bytes) => bytes,
        None => return Ok(()), // not a stream or already clean
    };

    let original_compressed_len = {
        let obj = doc.get_object_ref(id);
        match obj {
            Some(CosObject::Stream(s)) => s.data.len(),
            _ => return Ok(()),
        }
    };

    // Re-encode at level 9 (or Zopfli if opts.use_zopfli).
    let reencoded = deflate_best(&decoded, opts.use_zopfli)?;

    // Only accept if we made it smaller.
    if reencoded.len() >= original_compressed_len {
        return Ok(());
    }

    let saved = original_compressed_len.saturating_sub(reencoded.len());

    // Write back: update filter to FlateDecode + update /Length.
    doc.mutate_object(id, |obj| {
        if let CosObject::Stream(stream) = obj {
            stream.data = reencoded.clone();
            // Update or set /Filter → /FlateDecode
            stream.dictionary.set(
                CosName::new(b"Filter".to_vec()),
                CosObject::Name(CosName::new(b"FlateDecode".to_vec())),
            );
            // Remove /DecodeParms for old filter (level 9 needs none)
            stream.dictionary.remove(&CosName::new(b"DecodeParms".to_vec()));
            // Update /Length
            stream.dictionary.set(
                CosName::new(b"Length".to_vec()),
                CosObject::Integer(reencoded.len() as i64),
            );
        }
    });

    report.streams_recompressed += 1;
    report.bytes_saved += saved;
    Ok(())
}

/// Returns `true` if this is an image XObject that already uses DCTDecode or JPXDecode.
/// These are handled by the image pass and should not be re-wrapped in FlateDecode here.
fn is_image_with_dct_or_jpx(doc: &Document, id: crate::cos::ObjectId) -> bool {
    let obj = doc.get_object_ref(id);
    let stream = match obj {
        Some(CosObject::Stream(s)) => s,
        _ => return false,
    };

    // Check /Subtype /Image
    let subtype = stream.dictionary.get(&CosName::new(b"Subtype".to_vec()));
    let is_image = matches!(subtype, Some(CosObject::Name(n)) if n.as_str() == Some("Image"));
    if !is_image {
        return false;
    }

    // Check /Filter
    let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
    match filter {
        Some(CosObject::Name(n)) => {
            matches!(n.as_str(), Some("DCTDecode") | Some("JPXDecode") | Some("DCT") | Some("JPX"))
        }
        Some(CosObject::Array(arr)) => arr.iter().any(|f| {
            matches!(f, CosObject::Name(n) if matches!(n.as_str(), Some("DCTDecode") | Some("JPXDecode")))
        }),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// DEFLATE helper — Zopfli when available, else flate2 level 9
// ---------------------------------------------------------------------------

/// Compress `data` using the best available DEFLATE/zlib backend.
///
/// - `use_zopfli = true` + `compress-zopfli` feature: Zopfli optimal DEFLATE (smallest output).
/// - Otherwise: flate2 level-9 (fast, still good).
///
/// Output is a **zlib** stream (RFC 1950) — compatible with PDF `FlateDecode`.
pub(crate) fn deflate_best(data: &[u8], use_zopfli: bool) -> Result<Vec<u8>, std::io::Error> {
    #[cfg(feature = "compress-zopfli")]
    if use_zopfli {
        use zopfli::{BlockType, ZlibEncoder};
        use std::io::Write;

        let mut out = Vec::with_capacity(data.len() / 2);
        let mut encoder = ZlibEncoder::new(
            Default::default(),   // Options (iterations = 15 by default)
            BlockType::Dynamic,
            &mut out,
        )?;
        encoder.write_all(data)?;
        encoder.finish()?;
        return Ok(out);
    }

    // Fallback / non-Zopfli: flate2 level 9
    #[allow(unused_variables)]
    let _ = use_zopfli;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data)?;
    encoder.finish()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deflate_best_compresses_repetitive_data() {
        let data = b"Hello, World! ".repeat(100);
        let compressed = deflate_best(&data, false).unwrap();
        assert!(compressed.len() < data.len(), "deflate should compress repetitive data");
    }

    #[test]
    fn deflate_best_handles_empty() {
        let compressed = deflate_best(b"", false).unwrap();
        // A valid empty zlib stream is just header + adler32 checksum (2+4 bytes)
        assert!(compressed.len() < 16);
    }

    #[test]
    fn deflate_best_roundtrip() {
        use flate2::read::ZlibDecoder;
        use std::io::Read;

        let data = b"The quick brown fox jumps over the lazy dog.";
        let compressed = deflate_best(data, false).unwrap();
        let mut decoder = ZlibDecoder::new(compressed.as_slice());
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn run_on_minimal_pdf_no_panic() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let opts = crate::compress::CompressOptions::default();
        let result = run(&mut doc, &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn streams_report_default_zero() {
        let r = StreamsReport::default();
        assert_eq!(r.streams_recompressed, 0);
        assert_eq!(r.bytes_saved, 0);
    }

    #[test]
    fn skip_already_small_stream() {
        let data = b"x";
        let compressed = deflate_best(data, false).unwrap();
        assert!(compressed.len() > data.len());
    }
}

