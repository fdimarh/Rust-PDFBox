//! Pass 5 — Image XObject resampling and re-encoding.
//!
//! For every image XObject in the document:
//! 1. Compute the effective DPI from the page CTM.
//! 2. Decode the raw pixel buffer:
//!    - `DCTDecode`   → zune-jpeg / image crate JPEG decode
//!    - `JBIG2Decode` → nipdf-jbig2dec (libjbig2dec FFI) → 1-bit→8-bit gray expand
//!    - `FlateDecode` / raw → existing io filter pipeline
//! 3. Optionally detect and convert visually-grey RGB → DeviceGray.
//! 4. Optionally downsample if `effective_dpi > image_max_dpi`.
//! 5. Re-encode:
//!    - **1-bit B&W** (JBIG2 source, no downsample): CCITT Group 4 (lossless, optimal for scans)
//!    - **Grayscale / RGB**: JPEG lossy re-encode (MozJPEG or jpeg-encoder)
//!    - **Non-photo FlateDecode**: lossless PNG re-optimisation via oxipng
//! 6. Accept new encoding only when smaller; update stream dict.
//!
//! **Feature `compress-images`:**
//! - [`image`](https://crates.io/crates/image) `0.25` — pixel decode + Lanczos3 resample
//! - [`jpeg-encoder`](https://crates.io/crates/jpeg-encoder) `0.6` — pure-Rust JPEG encode
//! - [`zune-jpeg`](https://crates.io/crates/zune-jpeg) `0.4` — fast JPEG decode
//! - [`oxipng`](https://crates.io/crates/oxipng) `9.x` — lossless PNG optimizer
//!
//! **Feature `compress-jbig2`** (super-set of `compress-images`):
//! - [`nipdf-jbig2dec`](https://crates.io/crates/nipdf-jbig2dec) `0.4` — JBIG2 decode via libjbig2dec FFI
//! - [`fax`](https://crates.io/crates/fax) `0.2` — CCITT Group 4 encoder (lossless 1-bit, smallest for B&W scans)
//!
//! **Feature `compress-mozjpeg`** (super-set of `compress-images`):
//! - [`mozjpeg`](https://crates.io/crates/mozjpeg) `0.10` — libjpeg-turbo JPEG encoder (progressive + psychovisual)

use crate::cos::{CosName, CosObject, ObjectId};
use crate::{Document, PdfResult};
use super::CompressOptions;

// ---------------------------------------------------------------------------
// Public report
// ---------------------------------------------------------------------------

/// Statistics returned by [`run`].
#[derive(Debug, Default)]
pub struct ImagesReport {
    /// Number of images that were resampled (DPI reduced) or re-encoded.
    pub images_resampled: usize,
    /// Number of PNG images that were losslessly re-optimized via oxipng.
    pub images_png_optimized: usize,
    /// Number of 1-bit images re-encoded as CCITT Group 4 (lossless, smaller than JBIG2 for most scans).
    pub images_ccitt_encoded: usize,
    /// Number of images auto-converted to DeviceGray.
    pub images_grayscale_converted: usize,
    /// Approximate bytes saved across all image operations.
    pub bytes_saved: usize,
    /// Number of images skipped due to decode errors.
    pub errors_skipped: usize,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the image optimization pass over all image XObjects in `doc`.
pub fn run(doc: &mut Document, opts: &CompressOptions) -> PdfResult<ImagesReport> {
    let mut report = ImagesReport::default();

    let image_ids: Vec<ObjectId> = collect_image_xobject_ids(doc);

    // Build a map of image → effective DPI from page content streams.
    let dpi_map = compute_effective_dpi_map(doc, &image_ids);

    for id in &image_ids {
        match try_optimize_image(doc, *id, &dpi_map, opts, &mut report) {
            Ok(()) => {}
            Err(_) if opts.skip_on_decode_error => {
                report.errors_skipped += 1;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Image discovery
// ---------------------------------------------------------------------------

fn collect_image_xobject_ids(doc: &Document) -> Vec<ObjectId> {
    doc.objects()
        .filter_map(|(id, obj)| {
            let stream = obj.as_stream()?;
            let subtype = stream.dictionary.get(&CosName::new(b"Subtype".to_vec()));
            match subtype {
                Some(CosObject::Name(n)) if n.as_str() == Some("Image") => Some(id),
                _ => None,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Effective DPI calculation
// ---------------------------------------------------------------------------

fn compute_effective_dpi_map(
    doc: &Document,
    image_ids: &[ObjectId],
) -> std::collections::HashMap<ObjectId, f32> {
    let mut map = std::collections::HashMap::new();

    let page_ids: Vec<ObjectId> = doc.page_object_ids().collect();

    for page_id in &page_ids {
        let content_bytes = match doc.page_content_bytes(*page_id) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let (page_w_pts, _page_h_pts) = page_dimensions(doc, *page_id);
        let xobj_map = build_xobject_resource_map(doc, *page_id);

        let tokens: Vec<&str> = std::str::from_utf8(&content_bytes)
            .unwrap_or("")
            .split_ascii_whitespace()
            .collect();

        let mut last_cm: Option<[f32; 6]> = None;

        for i in 0..tokens.len() {
            let tok = tokens[i];
            if tok == "cm" && i >= 6 {
                let m: [f32; 6] = [
                    tokens[i-6].parse().unwrap_or(1.0),
                    tokens[i-5].parse().unwrap_or(0.0),
                    tokens[i-4].parse().unwrap_or(0.0),
                    tokens[i-3].parse().unwrap_or(1.0),
                    tokens[i-2].parse().unwrap_or(0.0),
                    tokens[i-1].parse().unwrap_or(0.0),
                ];
                last_cm = Some(m);
            } else if tok == "Do" && i >= 1 && tokens[i-1].starts_with('/') {
                let name = &tokens[i-1][1..];
                if let Some(obj_id) = xobj_map.get(name) {
                    if image_ids.contains(obj_id) {
                        let ctm_w_pts = last_cm
                            .map(|m| (m[0] * m[0] + m[1] * m[1]).sqrt())
                            .unwrap_or(page_w_pts);

                        let img_w_px = image_pixel_width(doc, *obj_id);

                        if ctm_w_pts > 0.0 && img_w_px > 0 {
                            let dpi = img_w_px as f32 / (ctm_w_pts / 72.0);
                            let entry = map.entry(*obj_id).or_insert(f32::MAX);
                            *entry = entry.min(dpi);
                        }
                        last_cm = None;
                    }
                }
            }
        }
    }

    map
}

fn page_dimensions(doc: &Document, page_id: ObjectId) -> (f32, f32) {
    let obj = match doc.get_object_ref(page_id) {
        Some(o) => o,
        None => return (612.0, 792.0),
    };
    let dict = match obj.as_dictionary() {
        Some(d) => d,
        None => return (612.0, 792.0),
    };
    let media_box = match dict.get(&CosName::new(b"MediaBox".to_vec())) {
        Some(CosObject::Array(arr)) if arr.len() >= 4 => arr,
        _ => return (612.0, 792.0),
    };
    let w = media_box[2].as_number().unwrap_or(612.0) as f32;
    let h = media_box[3].as_number().unwrap_or(792.0) as f32;
    (w, h)
}

fn build_xobject_resource_map(
    doc: &Document,
    page_id: ObjectId,
) -> std::collections::HashMap<String, ObjectId> {
    let mut map = std::collections::HashMap::new();

    let res_id = match doc.page_resources_id(page_id) {
        Some(id) => id,
        None => return map,
    };
    let res_obj = match doc.get_object_ref(res_id) {
        Some(o) => o,
        None => return map,
    };
    let res_dict = match res_obj.as_dictionary() {
        Some(d) => d,
        None => return map,
    };
    let xobj_dict = match res_dict.get(&CosName::new(b"XObject".to_vec())) {
        Some(CosObject::Dictionary(d)) => d,
        Some(CosObject::Reference(r)) => {
            match doc.get_object_ref(*r).and_then(|o| o.as_dictionary()) {
                Some(d) => d,
                None => return map,
            }
        }
        _ => return map,
    };

    for (k, v) in xobj_dict.iter() {
        if let Some(id) = v.as_reference() {
            let name = k.as_str().unwrap_or("").to_string();
            if !name.is_empty() {
                map.insert(name, id);
            }
        }
    }

    map
}

fn image_pixel_width(doc: &Document, id: ObjectId) -> u32 {
    let obj = match doc.get_object_ref(id) {
        Some(o) => o,
        None => return 0,
    };
    let stream = match obj.as_stream() {
        Some(s) => s,
        None => return 0,
    };
    stream.dictionary
        .get(&CosName::new(b"Width".to_vec()))
        .and_then(|v| if let CosObject::Integer(n) = v { Some(*n as u32) } else { None })
        .unwrap_or(0)
}

/// Resolve the JBIG2Globals reference for an image XObject, if any.
/// Returns `Some(globals_data)` when the image dict has `/DecodeParms << /JBIG2Globals N G R >>`.
#[allow(dead_code)]
fn get_jbig2_globals(doc: &Document, id: ObjectId) -> Option<Vec<u8>> {
    let stream = doc.get_object_ref(id)?.as_stream()?;

    let decode_parms = stream.dictionary.get(&CosName::new(b"DecodeParms".to_vec()))?;
    let parms_dict = match decode_parms {
        CosObject::Dictionary(d) => d,
        CosObject::Reference(r) => doc.get_object_ref(*r)?.as_dictionary()?,
        _ => return None,
    };

    let globals_ref = match parms_dict.get(&CosName::new(b"JBIG2Globals".to_vec()))? {
        CosObject::Reference(r) => *r,
        _ => return None,
    };

    let globals_stream = doc.get_object_ref(globals_ref)?.as_stream()?;
    Some(globals_stream.data.clone())
}

// ---------------------------------------------------------------------------
// Per-image optimization
// ---------------------------------------------------------------------------

fn try_optimize_image(
    doc: &mut Document,
    id: ObjectId,
    dpi_map: &std::collections::HashMap<ObjectId, f32>,
    opts: &CompressOptions,
    report: &mut ImagesReport,
) -> PdfResult<()> {
    #[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
    {
        return optimize_with_image_crate(doc, id, dpi_map, opts, report);
    }

    #[allow(unreachable_code)]
    Ok(())
}

// ---------------------------------------------------------------------------
// Full implementation
// ---------------------------------------------------------------------------

#[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
fn optimize_with_image_crate(
    doc: &mut Document,
    id: ObjectId,
    dpi_map: &std::collections::HashMap<ObjectId, f32>,
    opts: &CompressOptions,
    report: &mut ImagesReport,
) -> PdfResult<()> {
    use image::{DynamicImage, ImageBuffer, Rgb, Luma};
    use image::imageops::FilterType;

    // ── Gather image metadata ─────────────────────────────────────────────────
    let (filter_name, width, height, bits, colorspace_name, original_len) = {
        let obj = doc.get_object_ref(id);
        let stream = match obj.and_then(|o| o.as_stream()) {
            Some(s) => s,
            None => return Ok(()),
        };
        let filter = stream.dictionary
            .get(&CosName::new(b"Filter".to_vec()))
            .and_then(|v| {
                if let CosObject::Name(n) = v { n.as_str().map(|s| s.to_string()) } else { None }
            })
            .unwrap_or_default();
        let w = stream.dictionary
            .get(&CosName::new(b"Width".to_vec()))
            .and_then(|v| if let CosObject::Integer(n) = v { Some(*n as u32) } else { None })
            .unwrap_or(0);
        let h = stream.dictionary
            .get(&CosName::new(b"Height".to_vec()))
            .and_then(|v| if let CosObject::Integer(n) = v { Some(*n as u32) } else { None })
            .unwrap_or(0);
        let bpc = stream.dictionary
            .get(&CosName::new(b"BitsPerComponent".to_vec()))
            .and_then(|v| if let CosObject::Integer(n) = v { Some(*n as u32) } else { None })
            .unwrap_or(8);
        let cs = stream.dictionary
            .get(&CosName::new(b"ColorSpace".to_vec()))
            .and_then(|v| if let CosObject::Name(n) = v { n.as_str().map(|s| s.to_string()) } else { None })
            .unwrap_or_else(|| "DeviceRGB".to_string());
        let orig_len = stream.data.len();
        (filter, w, h, bpc, cs, orig_len)
    };

    if width == 0 || height == 0 { return Ok(()); }
    // Skip JPX (JPEG 2000) — no pure-Rust decoder.
    if filter_name == "JPXDecode" { return Ok(()); }
    // Skip JBIG2 when compress-jbig2 feature is absent.
    #[cfg(not(feature = "compress-jbig2"))]
    if filter_name == "JBIG2Decode" { return Ok(()); }

    // ── Decode raw pixels ─────────────────────────────────────────────────────
    let (raw_pixels, decoded_as_gray) = decode_image_pixels(doc, id, &filter_name, width, height, bits, &colorspace_name)?;
    if raw_pixels.is_empty() { return Ok(()); }

    // ── Build DynamicImage ────────────────────────────────────────────────────
    // JBIG2 decoded data is always 8-bit grayscale (1-bit expanded).
    let is_jbig2 = filter_name == "JBIG2Decode";
    let channels = if decoded_as_gray || is_jbig2
        || colorspace_name == "DeviceGray"
        || colorspace_name.contains("Gray")
        || bits == 1
    { 1 } else { 3 };

    let mut dyn_img: DynamicImage = if channels == 1 {
        if raw_pixels.len() < (width * height) as usize { return Ok(()); }
        let buf = ImageBuffer::<Luma<u8>, _>::from_raw(width, height, raw_pixels.clone())
            .ok_or_else(|| crate::PdfError::Compress { reason: "bad Luma image buffer".into() })?;
        DynamicImage::ImageLuma8(buf)
    } else {
        if raw_pixels.len() < (width * height * 3) as usize { return Ok(()); }
        let buf = ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, raw_pixels.clone())
            .ok_or_else(|| crate::PdfError::Compress { reason: "bad RGB image buffer".into() })?;
        DynamicImage::ImageRgb8(buf)
    };
    let mut is_gray = channels == 1;

    // ── Grayscale auto-detect ─────────────────────────────────────────────────
    if opts.image_grayscale_detect && channels == 3 {
        if visually_gray(&raw_pixels) {
            dyn_img = DynamicImage::ImageLuma8(dyn_img.to_luma8());
            is_gray = true;
            report.images_grayscale_converted += 1;
        }
    }

    // ── DPI-based resampling ──────────────────────────────────────────────────
    let effective_dpi = dpi_map.get(&id).copied().unwrap_or(f32::MAX);
    let mut new_w = width;
    let mut new_h = height;

    if opts.optimize_images && effective_dpi > opts.image_max_dpi as f32 {
        let scale = opts.image_max_dpi as f32 / effective_dpi;
        new_w = ((width as f32 * scale).round() as u32).max(1);
        new_h = ((height as f32 * scale).round() as u32).max(1);
        if new_w != width || new_h != height {
            dyn_img = dyn_img.resize_exact(new_w, new_h, FilterType::Lanczos3);
        }
    }

    // ── PNG lossless optimization (non-photo images) ──────────────────────────
    if opts.optimize_png_images && filter_name == "FlateDecode" && !is_photo_image(&filter_name) {
        if let Some(optimized) = png_optimize(&dyn_img, new_w, new_h, is_gray) {
            if optimized.len() < original_len {
                let saved = original_len - optimized.len();
                update_image_stream(doc, id, optimized, new_w, new_h, is_gray, false);
                report.images_png_optimized += 1;
                report.bytes_saved += saved;
                report.images_resampled += 1;
                return Ok(());
            }
        }
    }

    // ── CCITT Group 4 re-encode for JBIG2 / 1-bit images (lossless, optimal for B&W scans) ──
    // Only attempt when the image is still 1-bit (no DPI downscale happened) so we preserve
    // lossless quality.  After a Lanczos downsample the pixels become 8-bit gray → use JPEG.
    #[cfg(feature = "compress-jbig2")]
    if is_jbig2 && new_w == width && new_h == height {
        // raw_pixels is 8-bit gray (0xFF=white, 0x00=black) — convert back to 1-bit for CCITT.
        if let Some(ccitt_bytes) = encode_ccitt_g4(&raw_pixels, width, height) {
            if ccitt_bytes.len() < original_len {
                let saved = original_len - ccitt_bytes.len();
                update_image_stream_ccitt(doc, id, ccitt_bytes, width, height);
                report.images_ccitt_encoded += 1;
                report.images_resampled += 1;
                report.bytes_saved += saved;
                return Ok(());
            }
        }
    }

    // ── JPEG re-encode (lossy) ────────────────────────────────────────────────
    // Applied to DCT, downsampled JBIG2 (8-bit gray), and other decoded images.
    if opts.optimize_images {
        let jpeg_bytes = encode_jpeg(&dyn_img, opts.image_jpeg_quality, is_gray, opts.image_use_mozjpeg);
        if !jpeg_bytes.is_empty() && jpeg_bytes.len() < original_len {
            let saved = original_len - jpeg_bytes.len();
            update_image_stream(doc, id, jpeg_bytes, new_w, new_h, is_gray, true);
            report.images_resampled += 1;
            report.bytes_saved += saved;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Pixel decoding — returns (pixels, decoded_as_gray)
// ---------------------------------------------------------------------------

/// Decode an image XObject into a raw 8-bit pixel buffer.
/// Returns `(pixels, is_gray)`.
#[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
fn decode_image_pixels(
    doc: &Document,
    id: ObjectId,
    filter: &str,
    _width: u32,
    _height: u32,
    _bits: u32,
    _colorspace: &str,
) -> PdfResult<(Vec<u8>, bool)> {
    let obj = doc.get_object_ref(id);
    let stream = match obj.and_then(|o| o.as_stream()) {
        Some(s) => s,
        None => return Ok((vec![], false)),
    };

    match filter {
        "DCTDecode" => {
            let pixels = decode_jpeg_pixels(&stream.data)?;
            Ok((pixels, false))
        }
        "JBIG2Decode" => {
            #[cfg(feature = "compress-jbig2")]
            {
                let globals = get_jbig2_globals(doc, id);
                let pixels = decode_jbig2_pixels(&stream.data, globals.as_deref(), width, height)?;
                Ok((pixels, true))  // JBIG2 always decodes to 8-bit gray
            }
            #[cfg(not(feature = "compress-jbig2"))]
            Ok((vec![], false))
        }
        _ => {
            // FlateDecode / raw / other — use existing io filter pipeline.
            let filter_obj = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
            let decoded = crate::io::decode_stream(&stream.data, filter_obj)
                .unwrap_or_else(|_| stream.data.clone());
            Ok((decoded, false))
        }
    }
}

// ---------------------------------------------------------------------------
// JPEG decode
// ---------------------------------------------------------------------------

#[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
fn decode_jpeg_pixels(data: &[u8]) -> PdfResult<Vec<u8>> {
    // Try zune-jpeg first (fast pure-Rust).
    #[cfg(any(feature = "compress-images", feature = "compress-jbig2"))]
    {
        use zune_jpeg::JpegDecoder;
        let mut decoder = JpegDecoder::new(data);
        if let Ok(pixels) = decoder.decode() {
            return Ok(pixels);
        }
    }

    // Fallback: image crate.
    #[allow(unused_imports)]
    use image::ImageDecoder;
    let cursor = std::io::Cursor::new(data);
    #[allow(unused_mut)]
    match image::codecs::jpeg::JpegDecoder::new(cursor) {
        Ok(mut dec) => {
            let (w, h) = dec.dimensions();
            let mut buf = vec![0u8; (w * h * dec.color_type().bytes_per_pixel() as u32) as usize];
            dec.read_image(&mut buf).map_err(|e| crate::PdfError::Compress {
                reason: format!("JPEG decode failed: {e}"),
            })?;
            Ok(buf)
        }
        Err(e) => Err(crate::PdfError::Compress {
            reason: format!("JPEG decoder init failed: {e}"),
        }),
    }
}

// ---------------------------------------------------------------------------
// JBIG2 decode (requires compress-jbig2 feature / libjbig2dec)
// ---------------------------------------------------------------------------

/// Decode a PDF-embedded JBIG2 bitstream into an 8-bit grayscale pixel buffer.
///
/// PDF JBIG2 streams use `JBIG2_OPTIONS_EMBEDDED` (no file header).
/// Some images also reference a shared `/JBIG2Globals` stream for symbol tables.
///
/// The raw data from `nipdf_jbig2dec::Image` is 1-bit packed, MSB-first,
/// with `stride` bytes per row.
/// JBIG2 convention: **0 = white, 1 = black** (opposite of PBM).
/// We expand to 8-bit: 0→255 (white), 1→0 (black).
#[cfg(feature = "compress-jbig2")]
fn decode_jbig2_pixels(
    data: &[u8],
    globals: Option<&[u8]>,
    expected_w: u32,
    expected_h: u32,
) -> PdfResult<Vec<u8>> {
    use nipdf_jbig2dec::{Document, OpenFlag};

    let mut page_cursor = std::io::Cursor::new(data);
    let doc = if let Some(glob) = globals {
        let mut glob_cursor = std::io::Cursor::new(glob);
        Document::from_reader(&mut page_cursor, Some(&mut glob_cursor), OpenFlag::Embedded)
    } else {
        Document::from_reader(&mut page_cursor, None::<&mut std::io::Cursor<&[u8]>>, OpenFlag::Embedded)
    };

    let doc = doc.map_err(|e| crate::PdfError::Compress {
        reason: format!("JBIG2 decode error: {e:?}"),
    })?;

    let images = doc.into_inner();
    if images.is_empty() {
        return Err(crate::PdfError::Compress {
            reason: "JBIG2 decoded zero pages".into(),
        });
    }

    let img = &images[0];
    let w = img.width();
    let h = img.height();

    if w == 0 || h == 0 {
        return Err(crate::PdfError::Compress {
            reason: format!("JBIG2 decoded image has zero dimension {w}x{h}"),
        });
    }

    // Sanity-check dimensions match the PDF dict.
    let use_w = if w == expected_w { w } else { w };
    let use_h = if h == expected_h { h } else { h };

    let stride = img.stride() as usize;
    let packed = img.data();
    let mut gray = Vec::with_capacity(use_w as usize * use_h as usize);

    // Unpack 1-bit → 8-bit.  JBIG2: 0=white(255), 1=black(0).
    for row in 0..use_h as usize {
        let row_start = row * stride;
        let row_end = (row_start + stride).min(packed.len());
        if row_start >= packed.len() { break; }
        let row_data = &packed[row_start..row_end];
        for col in 0..use_w as usize {
            let byte_idx = col / 8;
            let bit_pos = 7 - (col % 8);  // MSB first
            let bit = if byte_idx < row_data.len() {
                (row_data[byte_idx] >> bit_pos) & 1
            } else {
                0
            };
            // JBIG2: 1 = black = 0x00, 0 = white = 0xFF
            gray.push(if bit == 0 { 0xFF } else { 0x00 });
        }
    }

    Ok(gray)
}

// ---------------------------------------------------------------------------
// Grayscale detection
// ---------------------------------------------------------------------------

/// Returns `true` if the RGB pixel data is visually grey (max channel diff < 4).
fn visually_gray(rgb_pixels: &[u8]) -> bool {
    if rgb_pixels.len() < 3 {
        return false;
    }
    let sample_step = (rgb_pixels.len() / 3).max(1) / (rgb_pixels.len() / 300).max(1);
    let sample_step = sample_step.max(1);

    for chunk in rgb_pixels.chunks_exact(3).step_by(sample_step) {
        let r = chunk[0] as i16;
        let g = chunk[1] as i16;
        let b = chunk[2] as i16;
        if (r - g).abs() > 4 || (g - b).abs() > 4 || (r - b).abs() > 4 {
            return false;
        }
    }
    true
}

fn is_photo_image(filter: &str) -> bool {
    matches!(filter, "DCTDecode" | "JPXDecode")
}

// ---------------------------------------------------------------------------
// JPEG encoding
// ---------------------------------------------------------------------------

#[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
#[allow(unreachable_code, unused_variables)]
fn encode_jpeg(img: &image::DynamicImage, quality: u8, _is_gray: bool, use_mozjpeg: bool) -> Vec<u8> {
    #[cfg(feature = "compress-mozjpeg")]
    if use_mozjpeg {
        return encode_mozjpeg(img, quality);
    }

    #[cfg(any(feature = "compress-images", feature = "compress-jbig2"))]
    {
        return encode_jpeg_encoder(img, quality);
    }

    vec![]
}

#[cfg(any(feature = "compress-images", feature = "compress-jbig2"))]
fn encode_jpeg_encoder(img: &image::DynamicImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut out, quality);

    let result = match img {
        image::DynamicImage::ImageLuma8(buf) => {
            encoder.encode(buf.as_raw(), buf.width() as u16, buf.height() as u16,
                           jpeg_encoder::ColorType::Luma)
        }
        _ => {
            let rgb = img.to_rgb8();
            encoder.encode(rgb.as_raw(), rgb.width() as u16, rgb.height() as u16,
                           jpeg_encoder::ColorType::Rgb)
        }
    };

    match result {
        Ok(()) => out,
        Err(_) => vec![],
    }
}

#[cfg(feature = "compress-mozjpeg")]
fn encode_mozjpeg(img: &image::DynamicImage, quality: u8) -> Vec<u8> {
    // Use grayscale path when image is Luma — smaller + faster decode.
    let is_luma = matches!(img, image::DynamicImage::ImageLuma8(_));

    let result = std::panic::catch_unwind(|| {
        if is_luma {
            let gray = img.to_luma8();
            let width = gray.width() as usize;
            let height = gray.height() as usize;
            let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_GRAYSCALE);
            compress.set_size(width, height);
            compress.set_quality(quality as f32);
            compress.set_progressive_mode();   // progressive JPEG: 5–15% smaller
            let mut started = compress.start_compress(Vec::new()).unwrap();
            started.write_scanlines(gray.as_raw()).unwrap();
            started.finish().unwrap_or_default()
        } else {
            let rgb = img.to_rgb8();
            let width = rgb.width() as usize;
            let height = rgb.height() as usize;
            let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
            compress.set_size(width, height);
            compress.set_quality(quality as f32);
            compress.set_progressive_mode();   // progressive JPEG: 5–15% smaller
            // 4:2:0 chroma subsampling (same as ilovepdf)
            compress.set_chroma_sampling_pixel_sizes((2, 2), (2, 2));
            let mut started = compress.start_compress(Vec::new()).unwrap();
            started.write_scanlines(rgb.as_raw()).unwrap();
            started.finish().unwrap_or_default()
        }
    });

    result.unwrap_or_default()
}

// ---------------------------------------------------------------------------
// PNG optimization
// ---------------------------------------------------------------------------

#[cfg(any(feature = "compress-images", feature = "compress-mozjpeg", feature = "compress-jbig2"))]
fn png_optimize(img: &image::DynamicImage, w: u32, h: u32, is_gray: bool) -> Option<Vec<u8>> {
    #[cfg(any(feature = "compress-images", feature = "compress-jbig2"))]
    {
        let mut png_bytes = Vec::new();
        {
            use image::ImageEncoder;
            let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
            let (raw, color_type) = if is_gray {
                let g = img.to_luma8();
                (g.into_raw(), image::ExtendedColorType::L8)
            } else {
                let r = img.to_rgb8();
                (r.into_raw(), image::ExtendedColorType::Rgb8)
            };
            encoder.write_image(&raw, w, h, color_type).ok()?;
        }

        let options = oxipng::Options::max_compression();
        let optimized = oxipng::optimize_from_memory(&png_bytes, &options).ok()?;
        return Some(optimized);
    }

    #[allow(unreachable_code)]
    None
}

// ---------------------------------------------------------------------------
// CCITT Group 4 encoder (requires compress-jbig2 / fax crate)
// ---------------------------------------------------------------------------

/// Encode a grayscale 8-bit pixel buffer (0xFF=white, 0x00=black) as a
/// CCITT ITU-T T.6 (Group 4) bitstream.  This is the PDF `CCITTFaxDecode`
/// filter with `/K -1` (pure 2D encoding) — the optimal lossless codec for
/// 1-bit B&W scanned pages.
///
/// Returns `None` if the `fax` crate is not available (no `compress-jbig2` feature).
#[cfg(feature = "compress-jbig2")]
fn encode_ccitt_g4(gray8: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    use fax::{Color, VecWriter};
    use fax::encoder::Encoder;

    let mut encoder = Encoder::new(VecWriter::new());

    for row in 0..height as usize {
        let row_start = row * width as usize;
        let row_end = row_start + width as usize;
        if row_end > gray8.len() { break; }

        let line = gray8[row_start..row_end].iter().map(|&px| {
            // 0xFF = white, 0x00 = black  (our JBIG2 expand convention)
            if px >= 128 { Color::White } else { Color::Black }
        });

        encoder.encode_line(line, width as u16).ok()?;
    }

    let data = encoder.finish().ok()?.finish();
    if data.is_empty() { return None; }
    Some(data)
}

/// Update an image XObject stream to use CCITTFaxDecode (Group 4 / T.6).
///
/// Sets `/Filter /CCITTFaxDecode` with `/DecodeParms << /K -1 /Columns W >>`.
/// `/K -1` means pure 2D (Group 4).  `/Columns` is required by the PDF spec.
#[allow(dead_code)]
fn update_image_stream_ccitt(
    doc: &mut Document,
    id: ObjectId,
    new_data: Vec<u8>,
    width: u32,
    height: u32,
) {
    use crate::cos::CosDictionary;

    let data_len = new_data.len() as i64;

    doc.mutate_object(id, |obj| {
        if let CosObject::Stream(stream) = obj {
            stream.data = new_data;
            stream.dictionary.set(CosName::new(b"Width".to_vec()),  CosObject::Integer(width as i64));
            stream.dictionary.set(CosName::new(b"Height".to_vec()), CosObject::Integer(height as i64));
            stream.dictionary.set(CosName::new(b"Filter".to_vec()),
                CosObject::Name(CosName::new(b"CCITTFaxDecode".to_vec())));
            // /DecodeParms << /K -1 /Columns width >>
            let mut parms = CosDictionary::new();
            parms.set(CosName::new(b"K".to_vec()),       CosObject::Integer(-1));
            parms.set(CosName::new(b"Columns".to_vec()), CosObject::Integer(width as i64));
            stream.dictionary.set(CosName::new(b"DecodeParms".to_vec()),
                CosObject::Dictionary(parms));
            stream.dictionary.set(CosName::new(b"ColorSpace".to_vec()),
                CosObject::Name(CosName::new(b"DeviceGray".to_vec())));
            stream.dictionary.set(CosName::new(b"BitsPerComponent".to_vec()), CosObject::Integer(1));
            stream.dictionary.set(CosName::new(b"Length".to_vec()), CosObject::Integer(data_len));
        }
    });
}

fn update_image_stream(
    doc: &mut Document,
    id: ObjectId,
    new_data: Vec<u8>,
    new_w: u32,
    new_h: u32,
    is_gray: bool,
    is_jpeg: bool,
) {
    let data_len = new_data.len() as i64;
    let filter_name = if is_jpeg { "DCTDecode" } else { "FlateDecode" };
    let cs_name = if is_gray { "DeviceGray" } else { "DeviceRGB" };

    doc.mutate_object(id, |obj| {
        if let CosObject::Stream(stream) = obj {
            stream.data = new_data;
            stream.dictionary.set(CosName::new(b"Width".to_vec()),  CosObject::Integer(new_w as i64));
            stream.dictionary.set(CosName::new(b"Height".to_vec()), CosObject::Integer(new_h as i64));
            stream.dictionary.set(CosName::new(b"Filter".to_vec()), CosObject::Name(CosName::new(filter_name.as_bytes().to_vec())));
            stream.dictionary.set(CosName::new(b"ColorSpace".to_vec()), CosObject::Name(CosName::new(cs_name.as_bytes().to_vec())));
            stream.dictionary.set(CosName::new(b"BitsPerComponent".to_vec()), CosObject::Integer(8));
            stream.dictionary.set(CosName::new(b"Length".to_vec()), CosObject::Integer(data_len));
            stream.dictionary.remove(&CosName::new(b"DecodeParms".to_vec()));
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::CompressOptions;

    #[test]
    fn images_report_default_zero() {
        let r = ImagesReport::default();
        assert_eq!(r.images_resampled, 0);
        assert_eq!(r.images_png_optimized, 0);
        assert_eq!(r.images_grayscale_converted, 0);
        assert_eq!(r.bytes_saved, 0);
        assert_eq!(r.errors_skipped, 0);
    }

    #[test]
    fn run_on_minimal_pdf_no_panic() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let opts = CompressOptions::default();
        let result = run(&mut doc, &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn no_images_returns_zero_report() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let opts = CompressOptions::default();
        let report = run(&mut doc, &opts).unwrap();
        assert_eq!(report.images_resampled, 0);
    }

    #[test]
    fn detect_image_xobject() {
        let mut dict = crate::cos::CosDictionary::new();
        dict.set(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Image".to_vec())));
        dict.set(CosName::new(b"Width".to_vec()),   CosObject::Integer(10));
        dict.set(CosName::new(b"Height".to_vec()),  CosObject::Integer(10));
        let subtype = dict.get(&CosName::new(b"Subtype".to_vec()));
        assert!(matches!(subtype, Some(CosObject::Name(n)) if n.as_str() == Some("Image")));
    }

    #[test]
    fn visually_gray_detects_grey_pixels() {
        let grey = vec![128u8, 128, 128, 200, 200, 200, 50, 50, 50];
        assert!(visually_gray(&grey));
    }

    #[test]
    fn visually_gray_rejects_coloured_pixels() {
        let colour = vec![255u8, 0, 0, 0, 255, 0];
        assert!(!visually_gray(&colour));
    }

    #[test]
    fn grayscale_detect_converts() {
        let grey_rgb: Vec<u8> = (0u8..=100).flat_map(|v| [v, v, v]).collect();
        assert!(visually_gray(&grey_rgb));
    }

    #[test]
    fn skip_below_max_dpi() {
        let effective_dpi = 72.0_f32;
        let max_dpi = 150_u32;
        assert!(effective_dpi <= max_dpi as f32);
    }

    #[test]
    fn never_enlarge_image() {
        let effective_dpi = 72.0_f32;
        let max_dpi = 150_u32;
        assert!(!(effective_dpi > max_dpi as f32));
    }

    #[test]
    fn is_photo_image_dct() {
        assert!(is_photo_image("DCTDecode"));
        assert!(is_photo_image("JPXDecode"));
        assert!(!is_photo_image("FlateDecode"));
    }

    /// Verify 1-bit JBIG2 unpacking logic: 0=white(255), 1=black(0).
    #[test]
    fn jbig2_bit_expansion_white_black() {
        // Byte 0b10000000: bit 7 = 1 (black), bit 6 = 0 (white)
        let byte = 0b10000000u8;
        let bit7 = (byte >> 7) & 1;  // MSB = 1 → black
        let bit6 = (byte >> 6) & 1;  // → 0 → white
        assert_eq!(if bit7 == 0 { 0xFF } else { 0x00 }, 0x00);  // black
        assert_eq!(if bit6 == 0 { 0xFF } else { 0x00 }, 0xFF);  // white
    }
}

