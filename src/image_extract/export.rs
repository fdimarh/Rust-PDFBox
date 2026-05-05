use std::path::Path;

use image::ImageFormat;

use crate::{PdfError, PdfResult};

use super::{ImageExportFormat, PdImage};

impl PdImage {
    /// Saves this extracted image to a file in the requested format.
    ///
    /// Current support:
    /// - `ImageExportFormat::Png` for decodable 8-bit Gray/RGB images
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
                feature: "PNG export requires explicit DeviceGray/DeviceRGB color space",
            });
        };

        let color = match color_space {
            "DeviceGray" => image::ColorType::L8,
            "DeviceRGB" => image::ColorType::Rgb8,
            "DeviceCMYK" => {
                return Err(PdfError::Unsupported {
                    feature: "PNG export does not support DeviceCMYK in this phase",
                });
            }
            _ => {
                return Err(PdfError::Unsupported {
                    feature: "PNG export supports only DeviceGray/DeviceRGB in this phase",
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

