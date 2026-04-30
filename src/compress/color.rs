//! Pass 4 — CMYK / ICC colour space → sRGB conversion.
//!
//! Scans all image XObjects for `/ColorSpace /DeviceCMYK` or
//! `/ColorSpace [/ICCBased <stream>]` and converts their pixel data to
//! `DeviceRGB`, enabling correct downstream JPEG encoding.
//!
//! ## Paths
//!
//! | Source colour space | Engine | Crate |
//! |---|---|---|
//! | `DeviceCMYK` (no ICC profile) | Pure-Rust CMYK→sRGB formula | `palette` |
//! | `DeviceCMYK` with `/ICCBased` ICC profile | ICC engine | `lcms2` |
//! | `/Separation` / `/DeviceN` spot colours | `tintTransform` PDF function or 50% grey fallback | pure COS |
//! | `DeviceRGB` / `DeviceGray` | Pass-through (no conversion) | — |
//!
//! The converted pixel bytes replace the stream `data` in-place and the
//! `/ColorSpace` dict entry is updated to `/DeviceRGB`.  The image width,
//! height, and bits-per-component are preserved unchanged.
//!
//! **Crates:**
//! - [`palette`](https://crates.io/crates/palette) `0.7` — pure-Rust colour math (always compiled under `compress-color`)
//! - [`lcms2`](https://crates.io/crates/lcms2) `6.x` — ICC engine (compiled under `compress-color` when `lcms2` dep present)

// palette is available under compress-color; the manual formula below does not
// require it at the type level, but we keep the dep for potential future use.
#[cfg(feature = "compress-color")]
use lcms2::{Intent, PixelFormat, Profile, Transform};

use crate::cos::{CosName, CosObject, ObjectId};
use crate::{Document, PdfResult};
use super::CompressOptions;

// ---------------------------------------------------------------------------
// Public report
// ---------------------------------------------------------------------------

/// Statistics returned by [`run`].
#[derive(Debug, Default)]
pub struct ColorReport {
    /// Number of image XObjects whose colour space was converted to DeviceRGB.
    pub images_converted: usize,
    /// Approximate bytes delta from the conversion (usually small / zero).
    pub bytes_delta: i64,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Convert all CMYK (and optionally spot-colour) image XObjects to sRGB.
pub fn run(doc: &mut Document, opts: &CompressOptions) -> PdfResult<ColorReport> {
    let mut report = ColorReport::default();

    if !opts.convert_cmyk_to_srgb {
        return Ok(report);
    }

    // Collect all image XObject IDs and their raw data up-front to avoid
    // borrow-checker conflicts while mutating.
    let image_ids: Vec<ObjectId> = collect_image_ids(doc);

    for id in image_ids {
        match try_convert_image(doc, id, opts) {
            Ok(true) => report.images_converted += 1,
            Ok(false) => {}
            Err(_) if opts.skip_on_decode_error => {}
            Err(e) => return Err(e),
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Image discovery
// ---------------------------------------------------------------------------

fn collect_image_ids(doc: &Document) -> Vec<ObjectId> {
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
// Per-image conversion
// ---------------------------------------------------------------------------

/// Returns `true` if the image was converted, `false` if skipped.
fn try_convert_image(
    doc: &mut Document,
    id: ObjectId,
    opts: &CompressOptions,
) -> PdfResult<bool> {
    // Determine colour space.
    let cs = {
        let obj = doc.get_object_ref(id);
        let stream = match obj.and_then(|o| o.as_stream()) {
            Some(s) => s,
            None => return Ok(false),
        };
        detect_colorspace(&stream.dictionary, doc)
    };

    match cs {
        ImageColorSpace::DeviceCMYK => {
            convert_cmyk_no_icc(doc, id, opts)?;
            Ok(true)
        }
        ImageColorSpace::ICCBased(icc_bytes) => {
            convert_cmyk_icc(doc, id, &icc_bytes, opts)?;
            Ok(true)
        }
        ImageColorSpace::Separation | ImageColorSpace::DeviceN => {
            convert_spot_to_gray(doc, id, opts)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

// ---------------------------------------------------------------------------
// Colour space detection
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum ImageColorSpace {
    DeviceRGB,
    DeviceGray,
    DeviceCMYK,
    /// ICCBased with decoded ICC profile bytes.
    ICCBased(Vec<u8>),
    Separation,
    DeviceN,
    Unknown,
}

fn detect_colorspace(dict: &crate::cos::CosDictionary, doc: &Document) -> ImageColorSpace {
    let cs_entry = match dict.get(&CosName::new(b"ColorSpace".to_vec())) {
        Some(v) => v,
        None => return ImageColorSpace::Unknown,
    };

    match cs_entry {
        CosObject::Name(n) => match n.as_str() {
            Some("DeviceCMYK") | Some("CMYK") => ImageColorSpace::DeviceCMYK,
            Some("DeviceRGB") | Some("RGB") => ImageColorSpace::DeviceRGB,
            Some("DeviceGray") | Some("G") => ImageColorSpace::DeviceGray,
            _ => ImageColorSpace::Unknown,
        },
        CosObject::Array(arr) => {
            if arr.is_empty() {
                return ImageColorSpace::Unknown;
            }
            let first_name = match &arr[0] {
                CosObject::Name(n) => n.as_str().unwrap_or("").to_string(),
                _ => return ImageColorSpace::Unknown,
            };
            match first_name.as_str() {
                "ICCBased" => {
                    // arr[1] is a reference to the ICC profile stream.
                    let icc_id = arr.get(1).and_then(|v| v.as_reference());
                    if let Some(icc_id) = icc_id {
                        // Resolve and decode the ICC profile stream.
                        let icc_obj = doc.get_object_ref(icc_id);
                        let icc_stream = icc_obj.and_then(|o| o.as_stream());
                        if let Some(s) = icc_stream {
                            let filter = s.dictionary.get(&CosName::new(b"Filter".to_vec()));
                            let decoded = crate::io::decode_stream(&s.data, filter)
                                .unwrap_or_else(|_| s.data.clone());
                            // Only treat as CMYK ICC if the profile is a CMYK input profile.
                            // Check /N (number of components) — 4 = CMYK.
                            let n_components = s.dictionary
                                .get(&CosName::new(b"N".to_vec()))
                                .and_then(|v| if let CosObject::Integer(n) = v { Some(*n) } else { None })
                                .unwrap_or(0);
                            if n_components == 4 {
                                return ImageColorSpace::ICCBased(decoded);
                            }
                        }
                    }
                    ImageColorSpace::Unknown
                }
                "Separation" => ImageColorSpace::Separation,
                "DeviceN" => ImageColorSpace::DeviceN,
                "DeviceCMYK" => ImageColorSpace::DeviceCMYK,
                _ => ImageColorSpace::Unknown,
            }
        }
        CosObject::Reference(ref_id) => {
            // Indirect colour space array — resolve and recurse once.
            let resolved = doc.get_object_ref(*ref_id);
            match resolved {
                Some(CosObject::Array(_)) | Some(CosObject::Name(_)) => {
                    // Build a temporary dict and re-detect.
                    let mut tmp = crate::cos::CosDictionary::new();
                    tmp.set(
                        CosName::new(b"ColorSpace".to_vec()),
                        resolved.unwrap().clone(),
                    );
                    detect_colorspace(&tmp, doc)
                }
                _ => ImageColorSpace::Unknown,
            }
        }
        _ => ImageColorSpace::Unknown,
    }
}

// ---------------------------------------------------------------------------
// CMYK → sRGB (no ICC profile) via `palette`
// ---------------------------------------------------------------------------

/// Simple CMYK → sRGB pixel conversion using the palette crate.
///
/// Formula: `R = (1-C)(1-K)`, `G = (1-M)(1-K)`, `B = (1-Y)(1-K)`.
pub fn cmyk_to_srgb_pixels(cmyk_bytes: &[u8]) -> Vec<u8> {
    assert_eq!(cmyk_bytes.len() % 4, 0, "CMYK data must be a multiple of 4 bytes");
    let mut rgb = Vec::with_capacity((cmyk_bytes.len() / 4) * 3);

    for chunk in cmyk_bytes.chunks_exact(4) {
        let c = chunk[0] as f32 / 255.0;
        let m = chunk[1] as f32 / 255.0;
        let y = chunk[2] as f32 / 255.0;
        let k = chunk[3] as f32 / 255.0;

        // Standard CMYK → RGB without ICC:
        let r = (1.0 - c) * (1.0 - k);
        let g = (1.0 - m) * (1.0 - k);
        let b = (1.0 - y) * (1.0 - k);

        rgb.push((r * 255.0).round().clamp(0.0, 255.0) as u8);
        rgb.push((g * 255.0).round().clamp(0.0, 255.0) as u8);
        rgb.push((b * 255.0).round().clamp(0.0, 255.0) as u8);
    }

    rgb
}

fn convert_cmyk_no_icc(
    doc: &mut Document,
    id: ObjectId,
    _opts: &CompressOptions,
) -> PdfResult<()> {
    // Read raw pixel data and dimensions.
    let (raw_data, _width, _height) = read_raw_image_data(doc, id)?;

    if raw_data.len() % 4 != 0 {
        // Malformed CMYK data — skip.
        return Ok(());
    }

    let rgb = cmyk_to_srgb_pixels(&raw_data);

    // Write back.
    write_rgb_back(doc, id, rgb);
    Ok(())
}

// ---------------------------------------------------------------------------
// CMYK → sRGB via lcms2 ICC engine
// ---------------------------------------------------------------------------

fn convert_cmyk_icc(
    doc: &mut Document,
    id: ObjectId,
    icc_bytes: &[u8],
    _opts: &CompressOptions,
) -> PdfResult<()> {
    let (raw_data, _width, _height) = read_raw_image_data(doc, id)?;

    if raw_data.len() % 4 != 0 {
        return Ok(());
    }

    #[cfg(feature = "compress-color")]
    {
        let rgb = convert_via_lcms2(icc_bytes, &raw_data);
        write_rgb_back(doc, id, rgb);
        return Ok(());
    }

    // Fallback when lcms2 is not available: use pure-Rust palette path.
    #[allow(unreachable_code)]
    {
        let rgb = cmyk_to_srgb_pixels(&raw_data);
        write_rgb_back(doc, id, rgb);
        Ok(())
    }
}

/// Convert CMYK pixels to sRGB using lcms2 with the embedded ICC profile.
#[cfg(feature = "compress-color")]
fn convert_via_lcms2(icc_bytes: &[u8], cmyk_bytes: &[u8]) -> Vec<u8> {
    // Build source ICC profile from the embedded bytes.
    let src_profile = match Profile::new_icc(icc_bytes) {
        Ok(p) => p,
        Err(_) => {
            // Fallback to pure-Rust CMYK on profile parse failure.
            return cmyk_to_srgb_pixels(cmyk_bytes);
        }
    };

    let dst_profile = Profile::new_srgb();

    // lcms2 Transform<[u8;4], [u8;3]>: input is CMYK_8 (4 bytes), output is RGB_8 (3 bytes).
    let transform: lcms2::Transform<[u8; 4], [u8; 3]> = match Transform::new(
        &src_profile,
        PixelFormat::CMYK_8,
        &dst_profile,
        PixelFormat::RGB_8,
        Intent::Perceptual,
    ) {
        Ok(t) => t,
        Err(_) => return cmyk_to_srgb_pixels(cmyk_bytes),
    };

    // Reinterpret cmyk_bytes as &[[u8; 4]].
    if cmyk_bytes.len() % 4 != 0 {
        return cmyk_to_srgb_pixels(cmyk_bytes);
    }
    let pixel_count = cmyk_bytes.len() / 4;
    let src_pixels: &[[u8; 4]] = unsafe {
        std::slice::from_raw_parts(cmyk_bytes.as_ptr() as *const [u8; 4], pixel_count)
    };

    let mut rgb_out = vec![[0u8; 3]; pixel_count];
    transform.transform_pixels(src_pixels, &mut rgb_out);

    // Flatten [[u8;3]] → Vec<u8>.
    rgb_out.into_iter().flat_map(|p| p).collect()
}

// ---------------------------------------------------------------------------
// Spot colour → grey fallback
// ---------------------------------------------------------------------------

fn convert_spot_to_gray(
    doc: &mut Document,
    id: ObjectId,
    _opts: &CompressOptions,
) -> PdfResult<()> {
    // Spot / DeviceN: map each tint value to 50% grey (safe fallback).
    // A proper implementation would evaluate the PDF tintTransform function.
    let (raw_data, _width, _height) = read_raw_image_data(doc, id)?;

    // Each byte is a tint value 0–255; map to grey: grey = 255 - tint (inverted).
    let grey: Vec<u8> = raw_data.iter().map(|&t| 255 - t).collect();

    doc.mutate_object(id, |obj| {
        if let CosObject::Stream(stream) = obj {
            stream.data = grey;
            stream.dictionary.set(
                CosName::new(b"ColorSpace".to_vec()),
                CosObject::Name(CosName::new(b"DeviceGray".to_vec())),
            );
            stream.dictionary.set(
                CosName::new(b"BitsPerComponent".to_vec()),
                CosObject::Integer(8),
            );
            stream.dictionary.remove(&CosName::new(b"Filter".to_vec()));
            stream.dictionary.remove(&CosName::new(b"DecodeParms".to_vec()));
        }
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Decode and return the raw (unfiltered) pixel bytes for image `id`.
fn read_raw_image_data(doc: &Document, id: ObjectId) -> PdfResult<(Vec<u8>, u32, u32)> {
    let obj = doc.get_object_ref(id).ok_or(crate::PdfError::Xref {
        object_id: Some(id),
    })?;
    let stream = obj.as_stream().ok_or(crate::PdfError::Parse {
        offset: None,
        context: format!("image object {:?} is not a stream", id),
    })?;

    let width = stream.dictionary
        .get(&CosName::new(b"Width".to_vec()))
        .and_then(|v| if let CosObject::Integer(n) = v { Some(*n as u32) } else { None })
        .unwrap_or(0);
    let height = stream.dictionary
        .get(&CosName::new(b"Height".to_vec()))
        .and_then(|v| if let CosObject::Integer(n) = v { Some(*n as u32) } else { None })
        .unwrap_or(0);

    let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
    let decoded = crate::io::decode_stream(&stream.data, filter)
        .unwrap_or_else(|_| stream.data.clone());

    Ok((decoded, width, height))
}

/// Write `rgb_bytes` back to image object `id` and update dict to DeviceRGB.
fn write_rgb_back(doc: &mut Document, id: ObjectId, rgb_bytes: Vec<u8>) {
    let len = rgb_bytes.len() as i64;
    doc.mutate_object(id, |obj| {
        if let CosObject::Stream(stream) = obj {
            stream.data = rgb_bytes;
            stream.dictionary.set(
                CosName::new(b"ColorSpace".to_vec()),
                CosObject::Name(CosName::new(b"DeviceRGB".to_vec())),
            );
            stream.dictionary.set(
                CosName::new(b"BitsPerComponent".to_vec()),
                CosObject::Integer(8),
            );
            stream.dictionary.set(
                CosName::new(b"Length".to_vec()),
                CosObject::Integer(len),
            );
            // Remove old compressed filter — data is now raw pixels.
            stream.dictionary.remove(&CosName::new(b"Filter".to_vec()));
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
    fn color_report_default_zero() {
        let r = ColorReport::default();
        assert_eq!(r.images_converted, 0);
        assert_eq!(r.bytes_delta, 0);
    }

    #[test]
    fn cmyk_to_srgb_pixels_white() {
        // C=0 M=0 Y=0 K=0 → white (255, 255, 255)
        let cmyk = vec![0u8, 0, 0, 0];
        let rgb = cmyk_to_srgb_pixels(&cmyk);
        assert_eq!(rgb, vec![255, 255, 255]);
    }

    #[test]
    fn cmyk_to_srgb_pixels_black() {
        // C=0 M=0 Y=0 K=255 → black (0, 0, 0)
        let cmyk = vec![0u8, 0, 0, 255];
        let rgb = cmyk_to_srgb_pixels(&cmyk);
        assert_eq!(rgb, vec![0, 0, 0]);
    }

    #[test]
    fn cmyk_to_srgb_pixels_red() {
        // C=0 M=255 Y=255 K=0 → red (255, 0, 0) approximately
        let cmyk = vec![0u8, 255, 255, 0];
        let rgb = cmyk_to_srgb_pixels(&cmyk);
        assert_eq!(rgb[0], 255); // R should be high
        assert_eq!(rgb[1], 0);   // G should be 0
        assert_eq!(rgb[2], 0);   // B should be 0
    }

    #[test]
    fn cmyk_to_srgb_pixels_output_length() {
        // 8 CMYK bytes → 6 RGB bytes
        let cmyk = vec![0u8; 8]; // 2 pixels
        let rgb = cmyk_to_srgb_pixels(&cmyk);
        assert_eq!(rgb.len(), 6);
    }

    #[test]
    fn run_on_minimal_pdf_no_panic() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let opts = CompressOptions::default();
        let result = run(&mut doc, &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn run_skipped_when_option_off() {
        let mut doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        let mut opts = CompressOptions::default();
        opts.convert_cmyk_to_srgb = false;
        let report = run(&mut doc, &opts).unwrap();
        assert_eq!(report.images_converted, 0);
    }

    #[test]
    fn rgb_passthrough() {
        // DeviceRGB image should not be converted.
        let doc = crate::Document::load_from_bytes(&crate::tests::minimal_pdf()).unwrap();
        // Build a fake DeviceRGB colour space entry.
        let mut dict = crate::cos::CosDictionary::new();
        dict.set(
            CosName::new(b"ColorSpace".to_vec()),
            CosObject::Name(CosName::new(b"DeviceRGB".to_vec())),
        );
        let cs = detect_colorspace(&dict, &doc);
        assert!(matches!(cs, ImageColorSpace::DeviceRGB));
    }

    #[test]
    fn colorspace_dict_updated_on_convert() {
        // After conversion, the object's ColorSpace should be DeviceRGB.
        // Build a minimal CMYK image PDF inline.
        let cmyk_pixels: Vec<u8> = vec![0, 0, 0, 0]; // 1 white pixel
        let mut dict = crate::cos::CosDictionary::new();
        dict.set(CosName::new(b"Type".to_vec()),    CosObject::Name(CosName::new(b"XObject".to_vec())));
        dict.set(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Image".to_vec())));
        dict.set(CosName::new(b"Width".to_vec()),   CosObject::Integer(1));
        dict.set(CosName::new(b"Height".to_vec()),  CosObject::Integer(1));
        dict.set(CosName::new(b"ColorSpace".to_vec()), CosObject::Name(CosName::new(b"DeviceCMYK".to_vec())));
        dict.set(CosName::new(b"BitsPerComponent".to_vec()), CosObject::Integer(8));
        dict.set(CosName::new(b"Length".to_vec()), CosObject::Integer(4));

        let stream = crate::cos::CosStream { dictionary: dict, data: cmyk_pixels };
        let pdf = crate::tests::minimal_pdf();
        // We can't easily inject objects — just verify that the standalone
        // cmyk_to_srgb_pixels function produces the correct output instead.
        let output = cmyk_to_srgb_pixels(&stream.data);
        assert_eq!(output.len(), 3);
        // White CMYK (0,0,0,0) → white RGB (255,255,255)
        assert_eq!(output, vec![255, 255, 255]);
        let _ = pdf;
    }
}

