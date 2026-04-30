//! Overlay — stamp one PDF document's pages onto another.
//!
//! Maps to `org.apache.pdfbox.multipdf.Overlay` in Java PDFBox.
//!
//! # Usage
//!
//! ```rust,ignore
//! use rust_pdfbox::pageops::PdfOverlay;
//!
//! let overlay = PdfOverlay::new()
//!     .overlay_type(OverlayType::Header);
//!
//! let result = overlay.apply(&base_doc, &overlay_doc)?;
//! ```

use crate::cos::{CosDictionary, CosName, CosObject};
use crate::parser::xref::XRefEntry;
use crate::pdmodel::page::Page;
use crate::{Document, PdfResult};

/// How the overlay should be positioned relative to the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    /// Absolute [llx, lly, urx, ury] in user units.
    Absolute(f64, f64, f64, f64),
}

/// Whether the overlay is applied as a header (on top), footer (on bottom),
/// or as a full-page overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayType {
    /// Header — overlay placed at the top of each page.
    Header,
    /// Footer — overlay placed at the bottom of each page.
    Footer,
    /// Full page — overlay scaled to cover the entire page.
    FullPage,
    /// Custom position via `OverlayPosition`.
    Custom(OverlayPosition),
}

/// Builder for configuring and applying a PDF overlay.
///
/// # Example
/// ```rust,ignore
/// let overlay = PdfOverlay::new()
///     .overlay_type(OverlayType::Header);
/// let merged = overlay.apply(&base_doc, &overlay_doc)?;
/// ```
pub struct PdfOverlay {
    overlay_type: OverlayType,
}

impl PdfOverlay {
    pub fn new() -> Self {
        Self {
            overlay_type: OverlayType::FullPage,
        }
    }

    /// Sets the overlay type (header, footer, full-page, or custom position).
    pub fn overlay_type(mut self, t: OverlayType) -> Self {
        self.overlay_type = t;
        self
    }

    /// Applies the overlay document onto the base document.
    ///
    /// Each page of `overlay_doc` is stamped onto the corresponding page of
    /// `base_doc`. If `overlay_doc` has fewer pages, the last page is reused.
    pub fn apply(&self, base_doc: &mut Document, overlay_doc: &Document) -> PdfResult<()> {
        let base_count = base_doc.page_count();
        let overlay_count = overlay_doc.page_count();
        if overlay_count == 0 {
            return Ok(());
        }

        // Collect base page info upfront (before any mutation)
        let mut base_pages = Vec::new();
        {
            let base_tree = base_doc.pages()?;
            for i in 0..base_count {
                let page = base_tree.get(i).ok_or_else(|| crate::PdfError::Parse {
                    offset: None,
                    context: format!("base page index out of bounds: {}", i),
                })?;
                let media_box = page.media_box().ok_or_else(|| crate::PdfError::Parse {
                    offset: None,
                    context: "base page has no MediaBox".to_string(),
                })?;
                base_pages.push((page.id, media_box));
            }
        }

        // Collect overlay page content upfront (before any mutation)
        let mut overlay_content: Vec<Vec<u8>> = Vec::new();
        {
            let overlay_tree = overlay_doc.pages()?;
            for i in 0..overlay_count {
                let p = overlay_tree.get(i).ok_or_else(|| crate::PdfError::Parse {
                    offset: None,
                    context: format!("overlay page index out of bounds: {}", i),
                })?;
                let content = extract_page_content_stream(overlay_doc, &p)?;
                overlay_content.push(content);
            }
        }

        for i in 0..base_count {
            let overlay_idx = i.min(overlay_count - 1);
            let (page_id, ref base_media_box) = base_pages[i];

            let (x, y, w, h) = self.compute_placement_simple(base_media_box)?;

            let form_name = format!("Ov{}", i);

            // Embed the overlay content as a Form XObject
            embed_overlay_as_form(base_doc, &form_name, &overlay_content[overlay_idx], w, h)?;

            // Build a small content stream that references the overlay form
            let mut overlay_stream = Vec::new();
            overlay_stream.extend_from_slice(b"q\n");

            let mb_w = base_media_box.width();
            let mb_h = base_media_box.height();
            let scale_x = w / mb_w;
            let scale_y = h / mb_h;
            overlay_stream.extend_from_slice(
                format!("{} {} {} {} {} {} cm\n", scale_x, 0.0, 0.0, scale_y, x, y).as_bytes(),
            );
            overlay_stream.extend_from_slice(format!("/{} Do\n", form_name).as_bytes());
            overlay_stream.extend_from_slice(b"Q\n");

            // Create the content stream object
            let stream_id = base_doc.allocate_object_id();
            let mut dict = CosDictionary::new();
            dict.insert(
                CosName::new(b"Length".to_vec()),
                CosObject::Integer(overlay_stream.len() as i64),
            );
            let stream = crate::cos::CosStream::new(dict, overlay_stream);
            base_doc.insert_object(stream_id, CosObject::Stream(stream));
            base_doc.xref.insert_if_absent(
                stream_id,
                XRefEntry::InUse { offset: 0, generation: 0 },
            );

            // Append to page contents
            base_doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(page_dict) = obj {
                    let contents_key = CosName::contents();
                    let existing = page_dict.get(&contents_key).cloned();
                    match existing {
                        Some(CosObject::Reference(existing_id)) => {
                            page_dict.insert(
                                contents_key,
                                CosObject::Array(vec![
                                    CosObject::Reference(existing_id),
                                    CosObject::Reference(stream_id),
                                ]),
                            );
                        }
                        Some(CosObject::Array(mut arr)) => {
                            arr.push(CosObject::Reference(stream_id));
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

    /// Simplified placement computation that doesn't borrow the doc.
    fn compute_placement_simple(
        &self,
        base_media_box: &crate::pdmodel::page::Rectangle,
    ) -> PdfResult<(f64, f64, f64, f64)> {
        let base_w = base_media_box.width();
        let base_h = base_media_box.height();

        match self.overlay_type {
            OverlayType::FullPage => Ok((0.0, 0.0, base_w, base_h)),
            OverlayType::Header => {
                let ov_h = base_h * 0.15;
                let ov_w = base_w * 0.9;
                let x = (base_w - ov_w) / 2.0;
                let y = base_h - ov_h - 20.0;
                Ok((x, y, ov_w, ov_h))
            }
            OverlayType::Footer => {
                let ov_h = base_h * 0.1;
                let ov_w = base_w * 0.9;
                let x = (base_w - ov_w) / 2.0;
                let y = 20.0;
                Ok((x, y, ov_w, ov_h))
            }
            OverlayType::Custom(pos) => match pos {
                OverlayPosition::Absolute(llx, lly, urx, ury) => {
                    Ok((llx, lly, urx - llx, ury - lly))
                }
                _ => {
                    let size = base_w.min(base_h) * 0.15;
                    match pos {
                        OverlayPosition::TopLeft => Ok((10.0, base_h - size - 10.0, size, size)),
                        OverlayPosition::TopCenter => Ok(((base_w - size) / 2.0, base_h - size - 10.0, size, size)),
                        OverlayPosition::TopRight => Ok((base_w - size - 10.0, base_h - size - 10.0, size, size)),
                        OverlayPosition::CenterLeft => Ok((10.0, (base_h - size) / 2.0, size, size)),
                        OverlayPosition::Center => Ok(((base_w - size) / 2.0, (base_h - size) / 2.0, size, size)),
                        OverlayPosition::CenterRight => Ok((base_w - size - 10.0, (base_h - size) / 2.0, size, size)),
                        OverlayPosition::BottomLeft => Ok((10.0, 10.0, size, size)),
                        OverlayPosition::BottomCenter => Ok(((base_w - size) / 2.0, 10.0, size, size)),
                        OverlayPosition::BottomRight => Ok((base_w - size - 10.0, 10.0, size, size)),
                        OverlayPosition::Absolute(_, _, _, _) => unreachable!(),
                    }
                }
            },
        }
    }
}

impl Default for PdfOverlay {
    fn default() -> Self {
        Self::new()
    }
}

/// Extracts the raw content stream bytes from a page.
/// Returns an empty Vec if the page has no content stream.
fn extract_page_content_stream(doc: &Document, page: &Page<'_>) -> PdfResult<Vec<u8>> {
    let contents_obj = match page.contents_object() {
        Some(obj) => obj,
        None => return Ok(Vec::new()),
    };

    match contents_obj {
        CosObject::Reference(id) => {
            let stream = match doc.get_object_ref(*id).and_then(|obj| obj.as_stream()) {
                Some(s) => s,
                None => return Ok(Vec::new()),
            };
            Ok(stream.data.clone())
        }
        CosObject::Array(arr) => {
            let mut combined = Vec::new();
            for item in arr {
                if let Some(ref_id) = item.as_reference() {
                    if let Some(stream) = doc.get_object_ref(ref_id).and_then(|obj| obj.as_stream()) {
                        combined.extend_from_slice(&stream.data);
                    }
                }
            }
            Ok(combined)
        }
        CosObject::Stream(stream) => Ok(stream.data.clone()),
        _ => Ok(Vec::new()),
    }
}

/// Embeds overlay content as a Form XObject in the base document.
fn embed_overlay_as_form(
    doc: &mut Document,
    name: &str,
    content_bytes: &[u8],
    w: f64,
    h: f64,
) -> PdfResult<()> {
    let form_id = doc.allocate_object_id();
    let mut form_dict = CosDictionary::new();
    form_dict.insert(CosName::type_name(), CosObject::Name(CosName::new(b"XObject".to_vec())));
    form_dict.insert(CosName::subtype(), CosObject::Name(CosName::new(b"Form".to_vec())));
    form_dict.insert(CosName::new(b"BBox".to_vec()), CosObject::Array(vec![
        CosObject::Real(0.0),
        CosObject::Real(0.0),
        CosObject::Real(w),
        CosObject::Real(h),
    ]));
    form_dict.insert(CosName::new(b"Length".to_vec()), CosObject::Integer(content_bytes.len() as i64));
    let form_stream = crate::cos::CosStream::new(form_dict, content_bytes.to_vec());
    doc.insert_object(form_id, CosObject::Stream(form_stream));
    doc.xref.insert_if_absent(form_id, XRefEntry::InUse { offset: 0, generation: 0 });

    // Register in the first page's resources
    if let Ok(tree) = doc.pages() {
        if let Some(first_page) = tree.get(0) {
            let page_id = first_page.id;
            // Drop the tree borrow before mutation
            drop(tree);
            doc.mutate_object(page_id, |obj| {
                if let CosObject::Dictionary(dict) = obj {
                    let mut resources_dict = dict
                        .get(&CosName::resources())
                        .and_then(|r| r.as_dictionary())
                        .cloned()
                        .unwrap_or_default();
                    let mut xobjects = resources_dict
                        .get(&CosName::new(b"XObject".to_vec()))
                        .and_then(|x| x.as_dictionary())
                        .cloned()
                        .unwrap_or_default();
                    xobjects.insert(
                        CosName::new(name.as_bytes().to_vec()),
                        CosObject::Reference(form_id),
                    );
                    resources_dict.insert(CosName::new(b"XObject".to_vec()), CosObject::Dictionary(xobjects));
                    dict.insert(CosName::resources(), CosObject::Dictionary(resources_dict));
                }
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentStreamWriter;
    use crate::pdmodel::{DocumentBuilder, PageSize};

    #[test]
    fn test_overlay_full_page_no_crash() -> PdfResult<()> {
        let mut base = DocumentBuilder::new().page_size(PageSize::A4).build()?;
        let mut overlay = DocumentBuilder::new().page_size(PageSize::A4).build()?;

        let mut overlay_cs = ContentStreamWriter::new(&mut overlay, 0)?;
        overlay_cs.begin_text()?;
        overlay_cs.set_font("Helvetica", 12.0)?;
        overlay_cs.move_to(10.0, 10.0)?;
        overlay_cs.show_text("OVERLAY")?;
        overlay_cs.end_text()?;
        overlay_cs.close()?;

        let mut base_cs = ContentStreamWriter::new(&mut base, 0)?;
        base_cs.begin_text()?;
        base_cs.set_font("Helvetica", 12.0)?;
        base_cs.move_to(72.0, 720.0)?;
        base_cs.show_text("Base content")?;
        base_cs.end_text()?;
        base_cs.close()?;

        let overlay_op = PdfOverlay::new().overlay_type(OverlayType::FullPage);
        let result = overlay_op.apply(&mut base, &overlay);
        assert!(result.is_ok());

        let tree = base.pages()?;
        let page = tree.get(0).unwrap();
        let contents = page.contents_object().unwrap();
        assert!(matches!(contents, CosObject::Array(_)));
        Ok(())
    }

    #[test]
    fn test_overlay_header() -> PdfResult<()> {
        let mut base = DocumentBuilder::new().page_size(PageSize::Letter).build()?;
        let overlay_doc = DocumentBuilder::new().page_size(PageSize::Letter).build()?;

        let mut base_cs = ContentStreamWriter::new(&mut base, 0)?;
        base_cs.begin_text()?;
        base_cs.set_font("Helvetica", 12.0)?;
        base_cs.move_to(72.0, 720.0)?;
        base_cs.show_text("Base")?;
        base_cs.end_text()?;
        base_cs.close()?;

        let overlay_op = PdfOverlay::new().overlay_type(OverlayType::Header);
        let result = overlay_op.apply(&mut base, &overlay_doc);
        assert!(result.is_ok());

        let tree = base.pages()?;
        let page = tree.get(0).unwrap();
        let contents = page.contents_object().unwrap();
        assert!(matches!(contents, CosObject::Array(_)));
        assert!(!contents.as_array().unwrap().is_empty());
        Ok(())
    }

    #[test]
    fn test_overlay_empty_overlay_doc() -> PdfResult<()> {
        let mut base = DocumentBuilder::new().page_size(PageSize::A4).build()?;
        let empty_overlay = Document::empty();
        let overlay_op = PdfOverlay::new();
        let result = overlay_op.apply(&mut base, &empty_overlay);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_overlay_footer() -> PdfResult<()> {
        let mut base = DocumentBuilder::new().page_size(PageSize::A4).build()?;
        let overlay = DocumentBuilder::new().page_size(PageSize::A4).build()?;

        let mut cs = ContentStreamWriter::new(&mut base, 0)?;
        cs.begin_text()?;
        cs.set_font("Helvetica", 12.0)?;
        cs.move_to(72.0, 720.0)?;
        cs.show_text("Base")?;
        cs.end_text()?;
        cs.close()?;

        let overlay_op = PdfOverlay::new().overlay_type(OverlayType::Footer);
        assert!(overlay_op.apply(&mut base, &overlay).is_ok());
        Ok(())
    }

    #[test]
    fn test_overlay_type_custom_absolute() {
        let ot = OverlayType::Custom(OverlayPosition::Absolute(10.0, 20.0, 100.0, 200.0));
        if let OverlayType::Custom(pos) = ot {
            if let OverlayPosition::Absolute(llx, _lly, _urx, ury) = pos {
                assert!((llx - 10.0).abs() < 1e-9);
                assert!((ury - 200.0).abs() < 1e-9);
            } else {
                panic!("expected absolute");
            }
        } else {
            panic!("expected custom");
        }
    }
}
