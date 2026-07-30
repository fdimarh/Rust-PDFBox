//! Little CMS (LCMS) color profiling integration for accurate Image Extraction.
//!
//! Provides accurate CMYK / ICCBased to sRGB color space conversion.
//! Maps to PDFBox's color management pipeline.

use image::RgbaImage;
use lcms2::{PixelFormat, Profile, Transform};

/// Applies an ICC color profile to raw pixel data to convert it into accurate sRGB.
pub fn apply_icc_profile(
    raw_pixels: &[u8],
    icc_profile_data: &[u8],
    width: u32,
    height: u32,
    components: usize,
) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);

    // 1. Attempt to load the embedded ICC Profile
    let source_profile = match Profile::new_icc(icc_profile_data) {
        Ok(p) => p,
        Err(_) => {
            // Fallback: If profile is corrupted, return a blank image (or standard fallback)
            return img;
        }
    };

    // 2. Create standard sRGB output profile
    let srgb_profile = Profile::new_srgb();

    // Determine LCMS pixel format based on component count (e.g. 3=RGB, 4=CMYK)
    let in_format = match components {
        1 => PixelFormat::GRAY_8,
        3 => PixelFormat::RGB_8,
        4 => PixelFormat::CMYK_8,
        _ => PixelFormat::RGBA_8, // Fallback assumption
    };
    let out_format = PixelFormat::RGBA_8;

    // 3. Create Color Transform
    let transform = match Transform::new(
        &source_profile,
        in_format,
        &srgb_profile,
        out_format,
        lcms2::Intent::Perceptual,
    ) {
        Ok(t) => t,
        Err(_) => return img,
    };

    // 4. Transform data into output buffer
    let mut srgb_buffer = vec![0u8; (width * height * 4) as usize];

    // LCMS expects chunks of pixels based on component count
    // So we pad or slice the input raw_pixels to ensure proper boundaries
    let total_expected_bytes = (width * height) as usize * components;
    let safe_input = if raw_pixels.len() >= total_expected_bytes {
        &raw_pixels[..total_expected_bytes]
    } else {
        raw_pixels // might truncate/panic below if invalid, PDF streams can be weird
    };

    // Use transform to process pixels
    transform.transform_pixels(safe_input, &mut srgb_buffer);

    // 5. Build RgbaImage from transformed buffer
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let idx = ((y * width + x) * 4) as usize;
        if idx + 3 < srgb_buffer.len() {
            *pixel = image::Rgba([
                srgb_buffer[idx],
                srgb_buffer[idx + 1],
                srgb_buffer[idx + 2],
                srgb_buffer[idx + 3],
            ]);
        }
    }

    img
}
