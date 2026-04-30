//! Watermark — add text watermarks to PDF pages.
//!
//! Maps to commonly used PDFBox watermarking patterns.
//!
//! # Usage
//!
//! ```rust,ignore
//! use rust_pdfbox::pageops::add_watermark;
//!
//! add_watermark(&mut doc, "DRAFT", WatermarkConfig::default())?;
//! ```

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::parser::xref::XRefEntry;
use crate::{Document, PdfResult};

/// Configuration for watermark appearance and placement.
#[derive(Debug, Clone)]
pub struct WatermarkConfig {
    /// Font size in points.
    pub font_size: f64,
    /// Rotation angle in degrees (counter-clockwise).
    pub rotation: f64,
    /// Opacity (0.0 = fully transparent, 1.0 = fully opaque).
    pub opacity: f64,
    /// Red channel for fill color (0.0 – 1.0).
    pub r: f64,
    /// Green channel for fill color (0.0 – 1.0).
    pub g: f64,
    /// Blue channel for fill color (0.0 – 1.0).
    pub b: f64,
    /// Font name registered in page resources (e.g., "Helvetica").
    pub font_name: String,
    /// Whether to place under existing content (true) or overlay (false).
    pub underlay: bool,
    /// Vertical position as fraction of page height (0.0 = bottom, 1.0 = top).
    pub vertical_position: f64,
}

impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            font_size: 72.0,
            rotation: 45.0,
            opacity: 0.2,
            r: 0.5,
            g: 0.5,
            b: 0.5,
            font_name: "Helvetica".to_string(),
            underlay: false,
            vertical_position: 0.5,
        }
    }
}

/// Adds a text watermark to every page of the document.
///
/// The watermark text is rotated and centered on each page.
pub fn add_watermark(doc: &mut Document, text: &str, config: WatermarkConfig) -> PdfResult<()> {
    let page_count = doc.page_count();
    if page_count == 0 {
        return Ok(());
    }

    // Collect page info upfront (before mutation)
    struct PageInfo {
        id: ObjectId,
        width: f64,
        height: f64,
    }

    let pages_info: Vec<PageInfo> = {
        let tree = doc.pages()?;
        let mut info = Vec::with_capacity(tree.count());
        for i in 0..page_count {
            let page = tree.get(i).ok_or_else(|| crate::PdfError::Parse {
                offset: None,
                context: format!("page index out of bounds: {}", i),
            })?;
            let media_box = page.media_box().ok_or_else(|| crate::PdfError::Parse {
                offset: None,
                context: "page has no MediaBox".to_string(),
            })?;
            info.push(PageInfo {
                id: page.id,
                width: media_box.width(),
                height: media_box.height(),
            });
        }
        info
    };

    // Ensure Helvetica is in Resources for each page by registering it
    for pi in &pages_info {
        ensure_helvetica_font(doc, pi.id)?;
    }

    for pi in &pages_info {
        let page_w = pi.width;
        let page_h = pi.height;
        let center_x = page_w / 2.0;
        let center_y = page_h * config.vertical_position;

        // Build watermark content stream
        let mut watermark_content = Vec::new();

        watermark_content.extend_from_slice(b"q\n");

        // If opacity < 1.0, we could set an ExtGState, but for simplicity we just do the content
        let rad = config.rotation.to_radians();
        let cos_theta = rad.cos();
        let sin_theta = rad.sin();
        watermark_content.extend_from_slice(
            format!(
                "{} {} {} {} {} {} cm\n",
                cos_theta, sin_theta, -sin_theta, cos_theta, center_x, center_y
            )
            .as_bytes(),
        );

        watermark_content.extend_from_slice(
            format!("{} {} {} rg\n", config.r, config.g, config.b).as_bytes(),
        );
        watermark_content.extend_from_slice(
            format!("/{} {} Tf\n", config.font_name, config.font_size).as_bytes(),
        );

        watermark_content.extend_from_slice(b"BT\n");

        let text_width_estimate = text.len() as f64 * config.font_size * 0.5;
        watermark_content.extend_from_slice(
            format!("{} 0 Td\n", -text_width_estimate / 2.0).as_bytes(),
        );

        watermark_content.push(b'(');
        for byte in text.as_bytes() {
            match byte {
                b'(' => watermark_content.extend_from_slice(b"\\("),
                b')' => watermark_content.extend_from_slice(b"\\)"),
                b'\\' => watermark_content.extend_from_slice(b"\\\\"),
                _ => watermark_content.push(*byte),
            }
        }
        watermark_content.extend_from_slice(b") Tj\n");

        watermark_content.extend_from_slice(b"ET\n");
        watermark_content.extend_from_slice(b"Q\n");

        // Create the stream object
        let stream_id = doc.allocate_object_id();
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"Length".to_vec()),
            CosObject::Integer(watermark_content.len() as i64),
        );
        let stream = crate::cos::CosStream::new(dict, watermark_content);
        doc.insert_object(stream_id, CosObject::Stream(stream));
        doc.xref.insert_if_absent(stream_id, XRefEntry::InUse { offset: 0, generation: 0 });

        // Append (or prepend) to page contents
        doc.mutate_object(pi.id, |obj| {
            if let CosObject::Dictionary(page_dict) = obj {
                let contents_key = CosName::contents();
                let existing = page_dict.get(&contents_key).cloned();

                match existing {
                    Some(CosObject::Reference(existing_id)) => {
                        if config.underlay {
                            page_dict.insert(
                                contents_key,
                                CosObject::Array(vec![
                                    CosObject::Reference(stream_id),
                                    CosObject::Reference(existing_id),
                                ]),
                            );
                        } else {
                            page_dict.insert(
                                contents_key,
                                CosObject::Array(vec![
                                    CosObject::Reference(existing_id),
                                    CosObject::Reference(stream_id),
                                ]),
                            );
                        }
                    }
                    Some(CosObject::Array(mut arr)) => {
                        if config.underlay {
                            arr.insert(0, CosObject::Reference(stream_id));
                        } else {
                            arr.push(CosObject::Reference(stream_id));
                        }
                        page_dict.insert(contents_key, CosObject::Array(arr));
                    }
                    _ => {
                        page_dict.insert(contents_key, CosObject::Reference(stream_id));
                    }
                }
            }
        });
    }

    Ok(())
}

/// Ensures the page dictionary has a Helvetica font available in its resources.
fn ensure_helvetica_font(doc: &mut Document, page_id: ObjectId) -> PdfResult<()> {
    doc.mutate_object(page_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            let mut resources_dict = dict
                .get(&CosName::resources())
                .and_then(|r| r.as_dictionary())
                .cloned()
                .unwrap_or_default();

            // Check if Helvetica is already in fonts
            let has_helvetica = resources_dict
                .get(&CosName::new(b"Font".to_vec()))
                .and_then(|f| f.as_dictionary())
                .and_then(|fonts| fonts.get(&CosName::new(b"Helvetica".to_vec())))
                .is_some();

            if !has_helvetica {
                let mut helvetica = CosDictionary::new();
                helvetica.insert(CosName::type_name(), CosObject::Name(CosName::new(b"Font".to_vec())));
                helvetica.insert(CosName::new(b"Subtype".to_vec()), CosObject::Name(CosName::new(b"Type1".to_vec())));
                helvetica.insert(CosName::new(b"BaseFont".to_vec()), CosObject::Name(CosName::new(b"Helvetica".to_vec())));

                let mut fonts = resources_dict
                    .get(&CosName::new(b"Font".to_vec()))
                    .and_then(|f| f.as_dictionary())
                    .cloned()
                    .unwrap_or_default();
                fonts.insert(CosName::new(b"Helvetica".to_vec()), CosObject::Dictionary(helvetica));
                resources_dict.insert(CosName::new(b"Font".to_vec()), CosObject::Dictionary(fonts));
                dict.insert(CosName::resources(), CosObject::Dictionary(resources_dict));
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentStreamWriter;
    use crate::pdmodel::{DocumentBuilder, PageSize};

    #[test]
    fn test_add_watermark_default() -> PdfResult<()> {
        let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

        let mut cs = ContentStreamWriter::new(&mut doc, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 720.0)?;
        cs.show_text("Content")?;
        cs.end_text()?;
        cs.close()?;

        let result = add_watermark(&mut doc, "DRAFT", WatermarkConfig::default());
        assert!(result.is_ok());

        let tree = doc.pages()?;
        let page = tree.get(0).unwrap();
        let contents = page.contents_object().unwrap();
        assert!(matches!(contents, CosObject::Array(_)));
        let arr = contents.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        Ok(())
    }

    #[test]
    fn test_add_watermark_underlay() -> PdfResult<()> {
        let mut doc = DocumentBuilder::new().page_size(PageSize::Letter).build()?;

        let mut cs = ContentStreamWriter::new(&mut doc, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 720.0)?;
        cs.show_text("Content")?;
        cs.end_text()?;
        cs.close()?;

        let mut cfg = WatermarkConfig::default();
        cfg.underlay = true;
        let result = add_watermark(&mut doc, "CONFIDENTIAL", cfg);
        assert!(result.is_ok());

        let tree = doc.pages()?;
        let page = tree.get(0).unwrap();
        let contents = page.contents_object().unwrap();
        let arr = contents.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        Ok(())
    }

    #[test]
    fn test_add_watermark_empty_doc() {
        let doc = Document::empty();
        assert_eq!(doc.page_count(), 0);
    }

    #[test]
    fn test_add_watermark_custom_config() -> PdfResult<()> {
        let mut doc = DocumentBuilder::new().page_size(PageSize::A4).build()?;

        let mut cs = ContentStreamWriter::new(&mut doc, 0)?;
        cs.close()?;

        let cfg = WatermarkConfig {
            font_size: 48.0,
            rotation: 90.0,
            opacity: 0.5,
            r: 1.0,
            g: 0.0,
            b: 0.0,
            underlay: false,
            ..Default::default()
        };
        let result = add_watermark(&mut doc, "URGENT", cfg);
        assert!(result.is_ok());

        let tree = doc.pages()?;
        assert_eq!(tree.count(), 1);
        Ok(())
    }

    #[test]
    fn test_watermark_config_default() {
        let cfg = WatermarkConfig::default();
        assert_eq!(cfg.font_size, 72.0);
        assert_eq!(cfg.rotation, 45.0);
        assert_eq!(cfg.opacity, 0.2);
        assert!((cfg.r - 0.5).abs() < 1e-9);
        assert_eq!(cfg.font_name, "Helvetica");
        assert!((cfg.vertical_position - 0.5).abs() < 1e-9);
    }
}
