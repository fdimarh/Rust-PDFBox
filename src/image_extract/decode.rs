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

        let channels = match self.effective_color_space() {
            Some("DeviceGray") => 1usize,
            Some("DeviceRGB") => 3usize,
            Some("DeviceCMYK") => 4usize,
            Some("Indexed") => 1usize,
            _ => {
                return Err(PdfError::Unsupported {
                    feature: "image decode supports only DeviceGray/DeviceRGB/DeviceCMYK/Indexed/ICCBased in this phase",
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

        if self.color_space.as_deref() == Some("Indexed") {
            return self.expand_indexed(decoded);
        }

        Ok(decoded)
    }

    fn expand_indexed(&self, indices: Vec<u8>) -> PdfResult<Vec<u8>> {
        let Some(cs_obj) = self.color_space_obj.as_ref() else {
            return Err(PdfError::Unsupported {
                feature: "Indexed decode requires ColorSpace array",
            });
        };

        let cs_arr = match cs_obj {
            crate::cos::CosObject::Array(values) => values,
            _ => {
                return Err(PdfError::Unsupported {
                    feature: "Indexed decode requires ColorSpace array",
                });
            }
        };

        if cs_arr.len() < 4 {
            return Err(PdfError::Parse {
                offset: None,
                context: "Indexed ColorSpace array is too short".to_string(),
            });
        }

        let base = cs_arr[1].as_name().and_then(|n| n.as_str());
        if base != Some("DeviceRGB") {
            return Err(PdfError::Unsupported {
                feature: "Indexed decode currently supports only DeviceRGB base color space",
            });
        }

        let hival = cs_arr[2].as_integer().ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "Indexed ColorSpace hival must be an integer".to_string(),
        })?;
        if hival < 0 {
            return Err(PdfError::Parse {
                offset: None,
                context: "Indexed ColorSpace hival must be non-negative".to_string(),
            });
        }
        let hival = hival as usize;

        let lookup = match &cs_arr[3] {
            crate::cos::CosObject::String(v) | crate::cos::CosObject::HexString(v) => v.as_slice(),
            _ => {
                return Err(PdfError::Unsupported {
                    feature: "Indexed decode currently supports only string/hex lookup tables",
                });
            }
        };

        let palette_len = (hival + 1) * 3;
        if lookup.len() < palette_len {
            return Err(PdfError::Parse {
                offset: None,
                context: format!(
                    "Indexed lookup table too short: got {}, need at least {}",
                    lookup.len(),
                    palette_len
                ),
            });
        }

        let pixel_count = self.width as usize * self.height as usize;
        if indices.len() < pixel_count {
            return Err(PdfError::Parse {
                offset: None,
                context: format!(
                    "decoded indexed buffer too short: got {}, expected at least {}",
                    indices.len(),
                    pixel_count
                ),
            });
        }

        let mut out = Vec::with_capacity(pixel_count * 3);
        for &idx_u8 in indices.iter().take(pixel_count) {
            let idx = idx_u8 as usize;
            if idx > hival {
                return Err(PdfError::Parse {
                    offset: None,
                    context: format!("indexed pixel value {} exceeds hival {}", idx, hival),
                });
            }
            let p = idx * 3;
            out.extend_from_slice(&lookup[p..p + 3]);
        }

        Ok(out)
    }
}

