use crate::io;
use crate::{PdfError, PdfResult};

use super::PdImage;

impl PdImage {
    /// Returns raw pixel bytes for simple image forms.
    ///
    /// Supported in this baseline:
    /// - No filter, 8-bit `DeviceGray`/`DeviceRGB`/`DeviceCMYK`
    /// - `FlateDecode`, 8-bit `DeviceGray`/`DeviceRGB`/`DeviceCMYK`
    pub fn decode_pixels(&self) -> PdfResult<Vec<u8>> {
        if self.bits_per_component != 8 {
            return Err(PdfError::Unsupported {
                feature: "image decode supports only 8 bits/component in this phase",
            });
        }

        if self
            .filter_names
            .iter()
            .any(|f| matches!(f.as_str(), "DCTDecode" | "DCT" | "JPXDecode" | "CCITTFaxDecode"))
        {
            return Err(PdfError::Unsupported {
                feature: "pixel decode for DCT/JPX/CCITT images is not implemented yet",
            });
        }

        let decoded = io::decode_stream(&self.data, self.filter.as_ref()).map_err(|e| PdfError::Parse {
            offset: None,
            context: format!("image stream decode failed: {e}"),
        })?;

        let channels = match self.color_space.as_deref() {
            Some("DeviceGray") => 1usize,
            Some("DeviceRGB") => 3usize,
            Some("DeviceCMYK") => 4usize,
            _ => {
                return Err(PdfError::Unsupported {
                    feature: "image decode supports only DeviceGray/DeviceRGB/DeviceCMYK in this phase",
                });
            }
        };

        let expected = self.width as usize * self.height as usize * channels;
        if decoded.len() < expected {
            return Err(PdfError::Parse {
                offset: None,
                context: format!(
                    "decoded image buffer too short: got {}, expected at least {}",
                    decoded.len(),
                    expected
                ),
            });
        }

        Ok(decoded)
    }
}

