//! PDF Compression pipeline — Bonus 11 (full ilovepdf gap closure).
//!
//! Implements a multi-pass compression engine that mirrors the techniques
//! used by ilovepdf, achieving 60–90% size reduction on image-heavy PDFs.
//!
//! # Passes (in execution order)
//!
//! 1. **cleanup** — strip XMP metadata, thumbnails, dead resources, embedded files
//! 2. **streams** — re-encode all streams to FlateDecode level 9
//! 3. **dedup** — hash-deduplicate identical stream/dict objects
//! 4. **color** — convert CMYK/ICC colour spaces to sRGB
//! 5. **images** — DPI-based downsample + JPEG/PNG re-encode
//! 6. **fonts** — TrueType/CFF glyph subsetting
//! 7. **version** — PDF version downgrade + ObjStm repack
//! 8. **linearize** — PDF Annex F web optimisation
//!
//! # Feature flags
//!
//! | Feature | Passes unlocked |
//! |---|---|
//! | `compress` | cleanup, streams, dedup, version, linearize |
//! | `compress-images` | images (pure-Rust JPEG/PNG) |
//! | `compress-mozjpeg` | images via libjpeg-turbo (best quality) |
//! | `compress-color` | color (lcms2 + palette) |
//! | `compress-fonts` | fonts (subsetter + ttf-parser) |
//! | `compress-full` | all of the above |
//!
//! Enable `compress` in `Cargo.toml` default features or explicitly:
//! ```toml
//! rust-pdfbox = { features = ["compress-full"] }
//! ```

pub mod cleanup;
pub mod dedup;
pub mod linearize;
pub mod streams;
pub mod version;

#[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
pub mod images;

#[cfg(feature = "compress-color")]
pub mod color;

#[cfg(feature = "compress-fonts")]
pub mod font_subset;

use crate::{Document, PdfResult};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Selects a pre-configured compression profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionMode {
    /// 60–90% reduction: JPEG q=40, DPI 96, all passes including CMYK + font subset.
    Extreme,
    /// 40–70% reduction: JPEG q=72, DPI 150, all passes.
    #[default]
    Recommended,
    /// 10–25% reduction: stream re-compression + dedup + metadata strip only.
    /// No image re-encoding.
    Less,
    /// Fully configurable — individual pass toggles via [`CompressOptions`].
    Custom,
}

/// Target colour space for image XObjects after colour conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageColorspace {
    /// Keep the original colour space (no conversion).
    Preserve,
    /// Convert all colour images to DeviceRGB.
    #[default]
    DeviceRGB,
    /// Convert all colour images to DeviceGray (lossless only when already grey).
    DeviceGray,
}

/// Full set of per-pass options for the compression pipeline.
///
/// Use [`CompressOptions::for_mode`] to get a pre-configured set based on
/// [`CompressionMode`], or build a `Custom` set by constructing this struct
/// directly.
#[derive(Debug, Clone)]
pub struct CompressOptions {
    /// Pre-configured profile that filled these options.
    pub mode: CompressionMode,

    // ── Pass toggles ─────────────────────────────────────────────────────────
    /// Re-encode all streams → FlateDecode level 9 (pass: streams).
    pub recompress_streams: bool,
    /// Strip `/Metadata` XMP stream from catalog + all page dicts (pass: cleanup).
    pub remove_metadata: bool,
    /// Strip `/Thumb` image XObject from every page dict (pass: cleanup).
    pub remove_thumbnails: bool,
    /// Strip `/StructTreeRoot` + `/MarkInfo` from catalog (pass: cleanup).
    pub remove_structure_tree: bool,
    /// Strip `/PieceInfo` dict from catalog + pages (pass: cleanup).
    pub remove_piece_info: bool,
    /// Strip `/OCProperties` layer definitions + `/OC` entries (pass: cleanup).
    pub remove_optional_content: bool,
    /// Remove unreferenced `/Font`, `/XObject`, `/ExtGState`, … entries (pass: cleanup).
    pub clean_dead_resources: bool,
    /// Hash-deduplicate identical stream/dict objects (pass: dedup).
    pub deduplicate_objects: bool,
    /// Resample + JPEG re-encode image XObjects (pass: images).
    pub optimize_images: bool,
    /// Lossless PNG re-optimisation via oxipng (pass: images).
    pub optimize_png_images: bool,
    /// Convert CMYK/spot → sRGB via lcms2 / palette (pass: color).
    pub convert_cmyk_to_srgb: bool,
    /// Subset embedded TrueType/CFF fonts (pass: fonts).
    pub subset_fonts: bool,
    /// Re-pack objects into ObjStm + XRef streams (pass: version).
    pub repack_object_streams: bool,
    /// Downgrade PDF version to 1.4 where safe (pass: version).
    pub downgrade_pdf_version: bool,
    /// Web-optimise: linearize object order for fast first-page display (pass: linearize).
    pub linearize: bool,

    // ── Stream options ────────────────────────────────────────────────────────
    /// Use Zopfli optimal DEFLATE instead of zlib level-9 for stream compression.
    /// Produces 3–8% smaller streams but is ~100× slower. Only meaningful when
    /// `compress-zopfli` feature is enabled. Default: `false` (recommended), `true` (extreme).
    pub use_zopfli: bool,

    // ── Image options ─────────────────────────────────────────────────────────
    /// JPEG encode quality 1–100. `Recommended`=72, `Extreme`=40.
    pub image_jpeg_quality: u8,
    /// Downsample images with DPI above this threshold. `Recommended`=150, `Extreme`=96.
    pub image_max_dpi: u32,
    /// Use mozjpeg (libjpeg-turbo) encoder when `compress-mozjpeg` feature is active.
    pub image_use_mozjpeg: bool,
    /// Auto-detect visually-grey RGB images and convert to DeviceGray.
    pub image_grayscale_detect: bool,
    /// Target colour space for image XObjects after compression.
    pub image_target_colorspace: ImageColorspace,

    // ── Font options ──────────────────────────────────────────────────────────
    /// Subset TrueType fonts (`/FontFile2`) via the `subsetter` crate.
    pub font_subset_truetype: bool,
    /// Subset CFF/Type1C fonts (`/FontFile3`) via the `subsetter` crate.
    pub font_subset_cff: bool,
    /// Remove fonts that are declared but have zero glyphs used in content streams.
    pub font_remove_unused: bool,

    // ── Error handling ────────────────────────────────────────────────────────
    /// When `true`, objects that fail to decode are silently skipped rather than
    /// aborting the whole pipeline.
    pub skip_on_decode_error: bool,
}

impl CompressOptions {
    /// Returns a pre-configured [`CompressOptions`] for the given [`CompressionMode`].
    pub fn for_mode(mode: CompressionMode) -> Self {
        match mode {
            CompressionMode::Extreme => Self {
                mode,
                recompress_streams: true,
                remove_metadata: true,
                remove_thumbnails: true,
                remove_structure_tree: true,
                remove_piece_info: true,
                remove_optional_content: true,
                clean_dead_resources: true,
                deduplicate_objects: true,
                optimize_images: true,
                optimize_png_images: true,
                convert_cmyk_to_srgb: true,
                subset_fonts: true,
                repack_object_streams: true,
                downgrade_pdf_version: true,
                linearize: true,
                use_zopfli: true,     // Extreme: use Zopfli for best stream compression
                image_jpeg_quality: 40,
                image_max_dpi: 96,
                image_use_mozjpeg: true,
                image_grayscale_detect: true,
                image_target_colorspace: ImageColorspace::DeviceRGB,
                font_subset_truetype: true,
                font_subset_cff: true,
                font_remove_unused: true,
                skip_on_decode_error: true,
            },
            CompressionMode::Recommended => Self {
                mode,
                recompress_streams: true,
                remove_metadata: true,
                remove_thumbnails: true,
                remove_structure_tree: false,
                remove_piece_info: true,
                remove_optional_content: false,
                clean_dead_resources: true,
                deduplicate_objects: true,
                optimize_images: true,
                optimize_png_images: true,
                convert_cmyk_to_srgb: true,
                subset_fonts: true,
                repack_object_streams: true,
                downgrade_pdf_version: false,
                linearize: true,
                use_zopfli: false,    // Recommended: fast zlib level-9
                image_jpeg_quality: 72,
                image_max_dpi: 150,
                image_use_mozjpeg: true,
                image_grayscale_detect: true,
                image_target_colorspace: ImageColorspace::DeviceRGB,
                font_subset_truetype: true,
                font_subset_cff: true,
                font_remove_unused: true,
                skip_on_decode_error: true,
            },
            CompressionMode::Less => Self {
                mode,
                recompress_streams: true,
                remove_metadata: true,
                remove_thumbnails: true,
                remove_structure_tree: false,
                remove_piece_info: false,
                remove_optional_content: false,
                clean_dead_resources: false,
                deduplicate_objects: true,
                optimize_images: false,
                optimize_png_images: false,
                convert_cmyk_to_srgb: false,
                subset_fonts: false,
                repack_object_streams: false,
                downgrade_pdf_version: false,
                linearize: false,
                use_zopfli: false,    // Less: fast zlib level-9
                image_jpeg_quality: 85,
                image_max_dpi: 300,
                image_use_mozjpeg: false,
                image_grayscale_detect: false,
                image_target_colorspace: ImageColorspace::Preserve,
                font_subset_truetype: false,
                font_subset_cff: false,
                font_remove_unused: false,
                skip_on_decode_error: true,
            },
            CompressionMode::Custom => Self::default(),
        }
    }
}

impl Default for CompressOptions {
    /// Defaults to `Recommended` mode settings.
    fn default() -> Self {
        Self::for_mode(CompressionMode::Recommended)
    }
}

/// Statistics produced by [`compress`].
///
/// All `bytes_saved_*` values are lower-bounds — they reflect the difference
/// in serialised size of the affected objects only, not final file overhead.
#[derive(Debug, Default, Clone)]
pub struct CompressReport {
    /// Total serialised size of the document *before* compression (bytes).
    pub bytes_before: usize,
    /// Total serialised size of the document *after* compression (bytes).
    pub bytes_after: usize,
    /// Overall size reduction as a percentage: `(1 - after/before) * 100`.
    pub reduction_pct: f64,

    /// Number of content/resource streams re-encoded to FlateDecode level 9.
    pub streams_recompressed: usize,
    /// Number of image XObjects that were resampled and/or re-encoded.
    pub images_resampled: usize,
    /// Number of image XObjects that were losslessly re-optimised via oxipng.
    pub images_png_optimized: usize,
    /// Number of 1-bit images re-encoded as CCITT Group 4 (lossless).
    pub images_ccitt_encoded: usize,
    /// Number of image XObjects whose colour space was converted from CMYK → sRGB.
    pub images_cmyk_converted: usize,
    /// Number of image XObjects converted to DeviceGray.
    pub images_grayscale_converted: usize,
    /// Number of font programs that were subset.
    pub fonts_subsetted: usize,
    /// Number of font objects removed (declared but never used in content).
    pub fonts_removed: usize,
    /// Number of duplicate objects collapsed to a single canonical reference.
    pub objects_deduped: usize,
    /// Number of objects removed entirely (dead resources, stripped metadata, etc.).
    pub objects_removed: usize,

    /// Bytes saved by stream re-compression.
    pub bytes_saved_streams: usize,
    /// Bytes saved by image resampling + re-encoding.
    pub bytes_saved_images: usize,
    /// Bytes saved by font subsetting + removal.
    pub bytes_saved_fonts: usize,
    /// Bytes saved by object deduplication.
    pub bytes_saved_dedup: usize,
    /// Bytes saved by metadata + resource stripping.
    pub bytes_saved_metadata: usize,
    /// Number of objects skipped due to decode errors (when `skip_on_decode_error = true`).
    pub decode_errors_skipped: usize,
}

impl CompressReport {
    /// Returns a human-readable one-line summary.
    pub fn summary(&self) -> String {
        format!(
            "{} → {} bytes ({:.1}% reduction) | \
             streams={} images={} fonts_subsetted={} dedup={} removed={}",
            self.bytes_before,
            self.bytes_after,
            self.reduction_pct,
            self.streams_recompressed,
            self.images_resampled,
            self.fonts_subsetted,
            self.objects_deduped,
            self.objects_removed,
        )
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Applies the configured multi-pass compression pipeline to `doc` in-place.
///
/// Passes execute in a fixed order that maximises savings:
///
/// 1. `cleanup`   — strip dead weight (metadata, thumbnails, unused resources)
/// 2. `streams`   — re-compress all non-image streams to FlateDecode level 9
/// 3. `dedup`     — collapse duplicate objects
/// 4. `color`     — CMYK / ICC colour space conversion (requires `compress-color`)
/// 5. `images`    — DPI-aware resample + JPEG/PNG re-encode (requires `compress-images`)
/// 6. `fonts`     — TrueType + CFF glyph subsetting (requires `compress-fonts`)
/// 7. `version`   — PDF version downgrade + ObjStm repack
/// 8. `linearize` — web-optimise object order (PDF Annex F)
///
/// Returns a [`CompressReport`] with detailed statistics.
///
/// # Errors
///
/// Returns [`PdfError::Compress`] if a non-recoverable error occurs and
/// `opts.skip_on_decode_error` is `false`.
pub fn compress(doc: &mut Document, opts: CompressOptions) -> PdfResult<CompressReport> {
    let bytes_before = estimate_size(doc);
    let mut report = CompressReport {
        bytes_before,
        ..Default::default()
    };

    // ── Pass 1: cleanup ───────────────────────────────────────────────────────
    {
        let cleanup_report = cleanup::run(doc, &opts)?;
        report.objects_removed += cleanup_report.objects_removed;
        report.bytes_saved_metadata += cleanup_report.bytes_saved;
    }

    // ── Pass 2: streams ───────────────────────────────────────────────────────
    if opts.recompress_streams {
        let streams_report = streams::run(doc, &opts)?;
        report.streams_recompressed += streams_report.streams_recompressed;
        report.bytes_saved_streams += streams_report.bytes_saved;
    }

    // ── Pass 3: dedup ─────────────────────────────────────────────────────────
    if opts.deduplicate_objects {
        let dedup_report = dedup::run(doc, &opts)?;
        report.objects_deduped += dedup_report.objects_deduped;
        report.bytes_saved_dedup += dedup_report.bytes_saved;
    }

    // ── Pass 4: color (requires compress-color) ───────────────────────────────
    #[cfg(feature = "compress-color")]
    if opts.convert_cmyk_to_srgb {
        let color_report = color::run(doc, &opts)?;
        report.images_cmyk_converted += color_report.images_converted;
    }

    // ── Pass 5: images (requires compress-images, compress-mozjpeg, or compress-jbig2) ─
    #[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
    if opts.optimize_images || opts.optimize_png_images {
        let images_report = images::run(doc, &opts)?;
        report.images_resampled += images_report.images_resampled;
        report.images_png_optimized += images_report.images_png_optimized;
        report.images_ccitt_encoded += images_report.images_ccitt_encoded;
        report.images_grayscale_converted += images_report.images_grayscale_converted;
        report.bytes_saved_images += images_report.bytes_saved;
        report.decode_errors_skipped += images_report.errors_skipped;
    }

    // ── Pass 6: fonts (requires compress-fonts) ───────────────────────────────
    #[cfg(feature = "compress-fonts")]
    if opts.subset_fonts {
        let font_report = font_subset::run(doc, &opts)?;
        report.fonts_subsetted += font_report.fonts_subsetted;
        report.fonts_removed += font_report.fonts_removed;
        report.bytes_saved_fonts += font_report.bytes_saved;
    }

    // ── Pass 7: version + ObjStm repack ──────────────────────────────────────
    {
        let version_report = version::run(doc, &opts)?;
        report.streams_recompressed += version_report.objstm_repacked;
    }

    // ── Pass 8: linearize ─────────────────────────────────────────────────────
    if opts.linearize {
        linearize::run(doc, &opts)?;
    }

    // ── Final metrics ─────────────────────────────────────────────────────────
    let bytes_after = estimate_size(doc);
    report.bytes_after = bytes_after;
    report.reduction_pct = if bytes_before > 0 {
        (1.0 - bytes_after as f64 / bytes_before as f64) * 100.0
    } else {
        0.0
    };

    Ok(report)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Rough in-memory size estimate of all objects in the document.
///
/// Uses the sum of raw stream `.data` lengths plus a fixed per-object overhead
/// for dicts and other object types. This is used to compute before/after
/// delta — it's not the final serialised byte count, but it's proportional.
fn estimate_size(doc: &Document) -> usize {
    let mut total = 0usize;
    for (_id, obj) in doc.objects() {
        total += object_size(obj);
    }
    total
}

fn object_size(obj: &crate::cos::CosObject) -> usize {
    use crate::cos::CosObject;
    match obj {
        CosObject::Stream(stream) => stream.data.len() + 64,
        CosObject::String(s) => s.len() + 4,
        CosObject::Array(arr) => arr.iter().map(object_size).sum::<usize>() + 8,
        CosObject::Dictionary(dict) => {
            dict.entries()
                .map(|(k, v)| k.as_bytes().len() + object_size(v) + 4)
                .sum::<usize>()
                + 8
        }
        _ => 16,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_options_recommended_defaults() {
        let opts = CompressOptions::for_mode(CompressionMode::Recommended);
        assert_eq!(opts.mode, CompressionMode::Recommended);
        assert!(opts.recompress_streams);
        assert!(opts.remove_metadata);
        assert!(opts.deduplicate_objects);
        assert!(opts.optimize_images);
        assert_eq!(opts.image_jpeg_quality, 72);
        assert_eq!(opts.image_max_dpi, 150);
    }

    #[test]
    fn compress_options_extreme_quality() {
        let opts = CompressOptions::for_mode(CompressionMode::Extreme);
        assert_eq!(opts.image_jpeg_quality, 40);
        assert_eq!(opts.image_max_dpi, 96);
        assert!(opts.downgrade_pdf_version);
        assert!(opts.remove_structure_tree);
        assert!(opts.convert_cmyk_to_srgb);
        assert!(opts.subset_fonts);
    }

    #[test]
    fn compress_options_less_no_images() {
        let opts = CompressOptions::for_mode(CompressionMode::Less);
        assert!(!opts.optimize_images);
        assert!(!opts.subset_fonts);
        assert!(!opts.convert_cmyk_to_srgb);
        assert!(!opts.downgrade_pdf_version);
        assert!(opts.recompress_streams);
        assert!(opts.deduplicate_objects);
    }

    #[test]
    fn compress_options_default_is_recommended() {
        let a = CompressOptions::default();
        let b = CompressOptions::for_mode(CompressionMode::Recommended);
        assert_eq!(a.image_jpeg_quality, b.image_jpeg_quality);
        assert_eq!(a.image_max_dpi, b.image_max_dpi);
        assert_eq!(a.recompress_streams, b.recompress_streams);
    }

    #[test]
    fn compress_report_summary_format() {
        let r = CompressReport {
            bytes_before: 100_000,
            bytes_after: 60_000,
            reduction_pct: 40.0,
            streams_recompressed: 5,
            images_resampled: 3,
            fonts_subsetted: 2,
            objects_deduped: 10,
            objects_removed: 4,
            ..Default::default()
        };
        let s = r.summary();
        assert!(s.contains("40.0%"));
        assert!(s.contains("100000"));
        assert!(s.contains("60000"));
    }

    #[test]
    fn image_colorspace_default_is_device_rgb() {
        assert_eq!(ImageColorspace::default(), ImageColorspace::DeviceRGB);
    }

    #[test]
    fn compression_mode_default_is_recommended() {
        assert_eq!(CompressionMode::default(), CompressionMode::Recommended);
    }
}

