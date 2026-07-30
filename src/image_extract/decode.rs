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

        if self.filter_names.iter().any(|f| {
            matches!(
                f.as_str(),
                "DCTDecode" | "DCT" | "JPXDecode" | "CCITTFaxDecode"
            )
        }) {
            return Err(PdfError::Unsupported {
                feature: "pixel decode for DCT/JPX/CCITT images is not implemented yet",
            });
        }

        let decoded =
            io::decode_stream(&self.data, self.filter.as_ref()).map_err(|e| PdfError::Parse {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosObject};

    fn make_rgb_palette() -> Vec<u8> {
        // 4-color palette: red, green, blue, white
        vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]
    }

    fn make_image(
        width: u32,
        height: u32,
        color_space: Option<&str>,
        data: Vec<u8>,
        cs_obj: Option<CosObject>,
    ) -> PdImage {
        PdImage {
            object_id: None,
            resource_name: "Im0".to_string(),
            width,
            height,
            bits_per_component: 8,
            color_space: color_space.map(|s| s.to_string()),
            color_space_obj: cs_obj,
            smask: None,
            filter_names: vec![],
            data,
            filter: None,
        }
    }

    #[test]
    fn decode_pixels_rgb_no_filter() {
        let img = make_image(2, 2, Some("DeviceRGB"), vec![
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255,
        ], None);
        let pixels = img.decode_pixels().unwrap();
        assert_eq!(pixels.len(), 12);
    }

    #[test]
    fn decode_pixels_gray_no_filter() {
        let img = make_image(2, 1, Some("DeviceGray"), vec![128, 200], None);
        let pixels = img.decode_pixels().unwrap();
        assert_eq!(pixels.len(), 2);
    }

    #[test]
    fn decode_pixels_unsupported_bpp() {
        let mut img = make_image(1, 1, Some("DeviceRGB"), vec![255, 0, 0], None);
        img.bits_per_component = 1;
        assert!(img.decode_pixels().is_err());
    }

    #[test]
    fn decode_pixels_dct_unsupported() {
        let mut img = make_image(1, 1, Some("DeviceRGB"), vec![255, 0, 0], None);
        img.filter_names = vec!["DCTDecode".to_string()];
        assert!(img.decode_pixels().is_err());
    }

    #[test]
    fn decode_pixels_buffer_too_short() {
        let img = make_image(10, 10, Some("DeviceRGB"), vec![0; 50], None);
        assert!(img.decode_pixels().is_err());
    }

    // expand_indexed tests
    #[test]
    fn expand_indexed_basic() {
        let palette = make_rgb_palette();
        let cs_obj = CosObject::Array(vec![
            CosObject::Name(crate::cos::CosName::new(b"Indexed".to_vec())),
            CosObject::Name(crate::cos::CosName::new(b"DeviceRGB".to_vec())),
            CosObject::Integer(3), // hival = 3 -> 4 entries (0..3)
            CosObject::String(palette),
        ]);
        let img = make_image(2, 2, Some("Indexed"), vec![0, 1, 2, 3], Some(cs_obj));
        let expanded = img.decode_pixels().unwrap();
        // 4 pixels x 3 channels = 12 bytes
        assert_eq!(expanded.len(), 12);
        assert_eq!(&expanded[0..3], &[255, 0, 0]); // red
        assert_eq!(&expanded[3..6], &[0, 255, 0]); // green
        assert_eq!(&expanded[6..9], &[0, 0, 255]); // blue
        assert_eq!(&expanded[9..12], &[255, 255, 255]); // white
    }

    #[test]
    fn expand_indexed_no_cs_obj() {
        let img = make_image(1, 1, Some("Indexed"), vec![0], None);
        assert!(img.decode_pixels().is_err());
    }

    #[test]
    fn expand_indexed_non_rgb_base() {
        let cs_obj = CosObject::Array(vec![
            CosObject::Name(crate::cos::CosName::new(b"Indexed".to_vec())),
            CosObject::Name(crate::cos::CosName::new(b"DeviceGray".to_vec())),
            CosObject::Integer(0),
            CosObject::String(vec![0, 0, 0]),
        ]);
        let img = make_image(1, 1, Some("Indexed"), vec![0], Some(cs_obj));
        assert!(img.decode_pixels().is_err());
    }

    #[test]
    fn expand_indexed_too_short_lookup() {
        let cs_obj = CosObject::Array(vec![
            CosObject::Name(crate::cos::CosName::new(b"Indexed".to_vec())),
            CosObject::Name(crate::cos::CosName::new(b"DeviceRGB".to_vec())),
            CosObject::Integer(3),
            CosObject::String(vec![255, 0, 0]), // only 3 bytes for 4 entries
        ]);
        let img = make_image(1, 1, Some("Indexed"), vec![0], Some(cs_obj));
        assert!(img.decode_pixels().is_err());
    }

    #[test]
    fn expand_indexed_pixel_exceeds_hival() {
        let palette = make_rgb_palette();
        let cs_obj = CosObject::Array(vec![
            CosObject::Name(crate::cos::CosName::new(b"Indexed".to_vec())),
            CosObject::Name(crate::cos::CosName::new(b"DeviceRGB".to_vec())),
            CosObject::Integer(1), // hival = 1 -> only 2 entries
            CosObject::String(palette),
        ]);
        let img = make_image(1, 1, Some("Indexed"), vec![3], Some(cs_obj));
        assert!(img.decode_pixels().is_err());
    }

    #[test]
    fn expand_indexed_negative_hival() {
        let cs_obj = CosObject::Array(vec![
            CosObject::Name(crate::cos::CosName::new(b"Indexed".to_vec())),
            CosObject::Name(crate::cos::CosName::new(b"DeviceRGB".to_vec())),
            CosObject::Integer(-1),
            CosObject::String(vec![0, 0, 0]),
        ]);
        let img = make_image(1, 1, Some("Indexed"), vec![0], Some(cs_obj));
        assert!(img.decode_pixels().is_err());
    }

    #[test]
    fn expand_indexed_cs_array_too_short() {
        let cs_obj = CosObject::Array(vec![
            CosObject::Name(crate::cos::CosName::new(b"Indexed".to_vec())),
            CosObject::Name(crate::cos::CosName::new(b"DeviceRGB".to_vec())),
        ]);
        let img = make_image(1, 1, Some("Indexed"), vec![0], Some(cs_obj));
        assert!(img.decode_pixels().is_err());
    }
}
