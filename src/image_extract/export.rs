use std::path::Path;

use image::ImageFormat;

use crate::{PdfError, PdfResult};

use super::{ImageExportFormat, PdImage};

impl PdImage {
    /// Saves this extracted image to a file in the requested format.
    ///
    /// Current support:
    /// - `ImageExportFormat::Png` for decodable 8-bit Gray/RGB/Indexed/CMYK images
    /// - `ImageExportFormat::Jpeg` passthrough when source filter is DCTDecode
    pub fn save_as<P: AsRef<Path>>(&self, path: P, format: ImageExportFormat) -> PdfResult<()> {
        match format {
            ImageExportFormat::Png => self.save_png(path),
            ImageExportFormat::Jpeg => self.save_jpeg_passthrough(path),
        }
    }

    fn save_png<P: AsRef<Path>>(&self, path: P) -> PdfResult<()> {
        let pixels = self.decode_pixels()?;
        let Some(color_space) = self.color_space() else {
            return Err(PdfError::Unsupported {
                feature: "PNG export requires explicit DeviceGray/DeviceRGB/Indexed/DeviceCMYK color space",
            });
        };

        let (pixels, color) = match color_space {
            "DeviceGray" => (pixels, image::ColorType::L8),
            "DeviceRGB" | "Indexed" => (pixels, image::ColorType::Rgb8),
            "DeviceCMYK" => (cmyk_to_rgb8_for_png(&pixels), image::ColorType::Rgb8),
            _ => {
                return Err(PdfError::Unsupported {
                    feature: "PNG export supports only DeviceGray/DeviceRGB/Indexed/DeviceCMYK in this phase",
                });
            }
        };

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

