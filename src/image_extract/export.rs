use std::path::Path;

use image::ImageFormat;

use crate::io;
use crate::{PdfError, PdfResult};

use super::{ImageExportFormat, PdImage};

impl PdImage {
    /// Saves this extracted image to a file in the requested format.
    ///
    /// Current support:
    /// - `ImageExportFormat::Png`/`ImageExportFormat::Tiff` for decodable 8-bit Gray/RGB/Indexed/CMYK images
    /// - `ImageExportFormat::Jpeg` passthrough when source filter is DCTDecode
    pub fn save_as<P: AsRef<Path>>(&self, path: P, format: ImageExportFormat) -> PdfResult<()> {
        match format {
            ImageExportFormat::Png => self.save_png(path),
            ImageExportFormat::Tiff => self.save_tiff(path),
            ImageExportFormat::Jpeg => self.save_jpeg_passthrough(path),
        }
    }

    fn save_png<P: AsRef<Path>>(&self, path: P) -> PdfResult<()> {
        let (pixels, color) = self.prepare_raster_export_buffer()?;

        image::save_buffer_with_format(
            path,
            &pixels,
            self.width(),
            self.height(),
            color,
            ImageFormat::Png,
        )
        .map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("PNG export failed: {e}"),
        })
    }

    fn save_tiff<P: AsRef<Path>>(&self, path: P) -> PdfResult<()> {
        let (pixels, color) = self.prepare_raster_export_buffer()?;

        image::save_buffer_with_format(
            path,
            &pixels,
            self.width(),
            self.height(),
            color,
            ImageFormat::Tiff,
        )
        .map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("TIFF export failed: {e}"),
        })
    }

    fn save_jpeg_passthrough<P: AsRef<Path>>(&self, path: P) -> PdfResult<()> {
        let is_dct = self
            .filter_names()
            .iter()
            .any(|f| matches!(f.as_str(), "DCTDecode" | "DCT"));
        if !is_dct {
            return Err(PdfError::Unsupported {
                feature: "JPEG export currently supports only DCTDecode passthrough",
            });
        }

        std::fs::write(path, self.encoded_bytes()).map_err(PdfError::Io)
    }

    fn decode_smask_alpha(&self) -> PdfResult<Option<Vec<u8>>> {
        let Some(mask) = self.smask.as_ref() else {
            return Ok(None);
        };

        if mask.bits_per_component != 8 {
            return Err(PdfError::Unsupported {
                feature: "SMask export currently supports only 8-bit masks",
            });
        }
        if mask.width != self.width() || mask.height != self.height() {
            return Err(PdfError::Parse {
                offset: None,
                context: "SMask dimensions do not match image dimensions".to_string(),
            });
        }

        let decoded = io::decode_stream(&mask.data, mask.filter.as_ref()).map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("SMask decode failed: {e}"),
        })?;

        let expected = (self.width() * self.height()) as usize;
        if decoded.len() < expected {
            return Err(PdfError::Parse {
                offset: None,
                context: format!(
                    "decoded SMask too short: got {}, expected at least {}",
                    decoded.len(),
                    expected
                ),
            });
        }

        Ok(Some(decoded[..expected].to_vec()))
    }

    fn prepare_raster_export_buffer(&self) -> PdfResult<(Vec<u8>, image::ColorType)> {
        let pixels = self.decode_pixels()?;
        let alpha = self.decode_smask_alpha()?;
        let Some(color_space) = self.effective_color_space() else {
            return Err(PdfError::Unsupported {
                feature: "raster export requires explicit DeviceGray/DeviceRGB/Indexed/DeviceCMYK/ICCBased color space",
            });
        };

        let (pixels, color) = match color_space {
            "DeviceGray" => {
                if let Some(alpha) = alpha.as_ref() {
                    (interleave_luma_alpha(&pixels, alpha), image::ColorType::La8)
                } else {
                    (pixels, image::ColorType::L8)
                }
            }
            "DeviceRGB" | "Indexed" => {
                if let Some(alpha) = alpha.as_ref() {
                    (interleave_rgb_alpha(&pixels, alpha), image::ColorType::Rgba8)
                } else {
                    (pixels, image::ColorType::Rgb8)
                }
            }
            "DeviceCMYK" => {
                let rgb = cmyk_to_rgb8_for_png(&pixels);
                if let Some(alpha) = alpha.as_ref() {
                    (interleave_rgb_alpha(&rgb, alpha), image::ColorType::Rgba8)
                } else {
                    (rgb, image::ColorType::Rgb8)
                }
            }
            _ => {
                return Err(PdfError::Unsupported {
                    feature: "raster export supports only DeviceGray/DeviceRGB/Indexed/DeviceCMYK/ICCBased fallback in this phase",
                });
            }
        };

        Ok((pixels, color))
    }
}

fn interleave_luma_alpha(luma: &[u8], alpha: &[u8]) -> Vec<u8> {
    let count = luma.len().min(alpha.len());
    let mut out = Vec::with_capacity(count * 2);
    for i in 0..count {
        out.push(luma[i]);
        out.push(alpha[i]);
    }
    out
}

fn interleave_rgb_alpha(rgb: &[u8], alpha: &[u8]) -> Vec<u8> {
    let pixels = (rgb.len() / 3).min(alpha.len());
    let mut out = Vec::with_capacity(pixels * 4);
    for i in 0..pixels {
        let p = i * 3;
        out.extend_from_slice(&[rgb[p], rgb[p + 1], rgb[p + 2], alpha[i]]);
    }
    out
}

fn cmyk_to_rgb8_for_png(cmyk: &[u8]) -> Vec<u8> {
    let pixels = cmyk.len() / 4;
    let mut out = Vec::with_capacity(pixels * 3);

    for chunk in cmyk.chunks_exact(4) {
        let c = chunk[0] as u16;
        let m = chunk[1] as u16;
        let y = chunk[2] as u16;
        let k = chunk[3] as u16;

        // Device-CMYK approximation: RGB = (1-C)*(1-K), etc.
        let r = (((255 - c) * (255 - k) + 127) / 255) as u8;
        let g = (((255 - m) * (255 - k) + 127) / 255) as u8;
        let b = (((255 - y) * (255 - k) + 127) / 255) as u8;
        out.extend_from_slice(&[r, g, b]);
    }

    out
}

