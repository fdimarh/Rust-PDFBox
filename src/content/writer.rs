use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, PdfResult};

pub struct ContentStreamWriter<'a> {
    doc: &'a mut Document,
    page_id: ObjectId,
    buffer: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextShowElement<'a> {
    Text(&'a str),
    Adjust(f64),
}

impl<'a> ContentStreamWriter<'a> {
    pub fn new(doc: &'a mut Document, page_index: usize) -> PdfResult<Self> {
        let tree = doc.pages()?;
        let page = tree.get(page_index).ok_or_else(|| crate::PdfError::Parse {
            offset: None,
            context: format!("page index out of bounds: {}", page_index),
        })?;
        let page_id = page.id;

        Ok(Self {
            doc,
            page_id,
            buffer: Vec::new(),
        })
    }

    pub fn begin_text(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"BT\n");
        Ok(())
    }

    pub fn end_text(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"ET\n");
        Ok(())
    }

    pub fn set_font(&mut self, font_name: &str, size: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("/{} {} Tf\n", font_name, size).as_bytes());
        Ok(())
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} Td\n", x, y).as_bytes());
        Ok(())
    }

    pub fn move_to_next_line(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"T*\n");
        Ok(())
    }

    pub fn set_text_matrix(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} {} {} Tm\n", a, b, c, d, e, f).as_bytes());
        Ok(())
    }

    pub fn set_char_spacing(&mut self, value: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} Tc\n", value).as_bytes());
        Ok(())
    }

    pub fn set_word_spacing(&mut self, value: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} Tw\n", value).as_bytes());
        Ok(())
    }

    pub fn set_horizontal_scaling(&mut self, value: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} Tz\n", value).as_bytes());
        Ok(())
    }

    pub fn set_text_leading(&mut self, value: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} TL\n", value).as_bytes());
        Ok(())
    }

    pub fn set_text_rise(&mut self, value: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} Ts\n", value).as_bytes());
        Ok(())
    }

    pub fn show_text(&mut self, text: &str) -> PdfResult<()> {
        Self::push_pdf_literal_string(&mut self.buffer, text);
        self.buffer.extend_from_slice(b" Tj\n");
        Ok(())
    }

    pub fn show_text_next_line(&mut self, text: &str) -> PdfResult<()> {
        Self::push_pdf_literal_string(&mut self.buffer, text);
        self.buffer.extend_from_slice(b" '\n");
        Ok(())
    }

    pub fn show_text_next_line_with_spacing(
        &mut self,
        word_spacing: f64,
        char_spacing: f64,
        text: &str,
    ) -> PdfResult<()> {
        self.buffer
            .extend_from_slice(format!("{} {} ", word_spacing, char_spacing).as_bytes());
        Self::push_pdf_literal_string(&mut self.buffer, text);
        self.buffer.extend_from_slice(b" \"\n");
        Ok(())
    }

    pub fn show_text_positioned<'b>(&mut self, elements: &[TextShowElement<'b>]) -> PdfResult<()> {
        self.buffer.push(b'[');
        let mut first = true;
        for element in elements {
            if !first {
                self.buffer.push(b' ');
            }
            first = false;
            match element {
                TextShowElement::Text(text) => Self::push_pdf_literal_string(&mut self.buffer, text),
                TextShowElement::Adjust(amount) => {
                    self.buffer.extend_from_slice(format!("{}", amount).as_bytes());
                }
            }
        }
        self.buffer.extend_from_slice(b"] TJ\n");
        Ok(())
    }

    fn push_pdf_literal_string(buffer: &mut Vec<u8>, text: &str) {
        buffer.push(b'(');
        for byte in text.as_bytes() {
            match byte {
                b'(' => buffer.extend_from_slice(b"\\("),
                b')' => buffer.extend_from_slice(b"\\)"),
                b'\\' => buffer.extend_from_slice(b"\\\\"),
                _ => buffer.push(*byte),
            }
        }
        buffer.push(b')');
    }

    pub fn move_to_point(&mut self, x: f64, y: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} m\n", x, y).as_bytes());
        Ok(())
    }

    pub fn line_to(&mut self, x: f64, y: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} l\n", x, y).as_bytes());
        Ok(())
    }

    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} {} {} c\n", x1, y1, x2, y2, x3, y3).as_bytes());
        Ok(())
    }

    /// Smooth curve using `v` operator — first control point is current point.
    pub fn curve_to_final(&mut self, x2: f64, y2: f64, x3: f64, y3: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} v\n", x2, y2, x3, y3).as_bytes());
        Ok(())
    }

    /// Smooth curve using `y` operator — second control point is destination.
    pub fn curve_to_initial(&mut self, x1: f64, y1: f64, x3: f64, y3: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} y\n", x1, y1, x3, y3).as_bytes());
        Ok(())
    }

    pub fn add_rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} re\n", x, y, w, h).as_bytes());
        Ok(())
    }

    pub fn close_path(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"h\n");
        Ok(())
    }

    pub fn stroke(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"S\n");
        Ok(())
    }

    pub fn fill(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"f\n");
        Ok(())
    }

    pub fn fill_even_odd(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"f*\n");
        Ok(())
    }

    pub fn stroke_and_close(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"s\n");
        Ok(())
    }

    pub fn fill_and_stroke(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"B\n");
        Ok(())
    }

    pub fn fill_and_stroke_even_odd(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"B*\n");
        Ok(())
    }

    /// Closes, fills, and strokes the path (winding fill rule).
    pub fn close_fill_and_stroke(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"b\n");
        Ok(())
    }

    /// Closes, fills (even-odd), and strokes the path.
    pub fn close_fill_and_stroke_even_odd(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"b*\n");
        Ok(())
    }

    pub fn clip(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"W\n");
        Ok(())
    }

    pub fn clip_even_odd(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"W*\n");
        Ok(())
    }

    pub fn end_path(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"n\n");
        Ok(())
    }

    pub fn set_line_width(&mut self, width: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} w\n", width).as_bytes());
        Ok(())
    }

    pub fn set_line_cap(&mut self, style: u8) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} J\n", style).as_bytes());
        Ok(())
    }

    pub fn set_line_join(&mut self, style: u8) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} j\n", style).as_bytes());
        Ok(())
    }

    pub fn set_miter_limit(&mut self, value: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} M\n", value).as_bytes());
        Ok(())
    }

    /// Sets the line dash pattern via the `d` operator.
    /// `dash_array` — sequence of alternating on/off lengths; `dash_phase` — starting offset.
    pub fn set_line_dash(&mut self, dash_array: &[i64], dash_phase: i64) -> PdfResult<()> {
        self.buffer.push(b'[');
        let mut first = true;
        for val in dash_array {
            if !first {
                self.buffer.push(b' ');
            }
            first = false;
            self.buffer.extend_from_slice(format!("{}", val).as_bytes());
        }
        self.buffer.extend_from_slice(format!("] {} d\n", dash_phase).as_bytes());
        Ok(())
    }

    /// Sets the rendering intent via the `ri` operator.
    pub fn set_rendering_intent(&mut self, intent: &str) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("/{} ri\n", intent).as_bytes());
        Ok(())
    }

    /// Sets the flatness tolerance via the `i` operator.
    pub fn set_flatness(&mut self, flatness: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} i\n", flatness).as_bytes());
        Ok(())
    }

    /// Sets the graphics state parameters from a named ExtGState dictionary via the `gs` operator.
    pub fn set_graphics_state(&mut self, state_name: &str) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("/{} gs\n", state_name).as_bytes());
        Ok(())
    }

    pub fn set_stroke_color(&mut self, r: f64, g: f64, b: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} RG\n", r, g, b).as_bytes());
        Ok(())
    }

    pub fn set_fill_color(&mut self, r: f64, g: f64, b: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} rg\n", r, g, b).as_bytes());
        Ok(())
    }

    pub fn set_fill_gray(&mut self, gray: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} g\n", gray).as_bytes());
        Ok(())
    }

    pub fn set_stroke_gray(&mut self, gray: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} G\n", gray).as_bytes());
        Ok(())
    }

    pub fn set_fill_color_cmyk(&mut self, c: f64, m: f64, y: f64, k: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} k\n", c, m, y, k).as_bytes());
        Ok(())
    }

    pub fn set_stroke_color_cmyk(&mut self, c: f64, m: f64, y: f64, k: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} K\n", c, m, y, k).as_bytes());
        Ok(())
    }

    /// Sets the non-stroking color space via the `cs` operator.
    pub fn set_fill_color_space(&mut self, name: &str) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("/{} cs\n", name).as_bytes());
        Ok(())
    }

    /// Sets the stroking color space via the `CS` operator.
    pub fn set_stroke_color_space(&mut self, name: &str) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("/{} CS\n", name).as_bytes());
        Ok(())
    }

    /// Sets the non-stroking color using the `sc` operator (for color space with variable components).
    pub fn set_fill_color_custom(&mut self, components: &[f64]) -> PdfResult<()> {
        let mut out = String::new();
        for (i, c) in components.iter().enumerate() {
            if i > 0 { out.push(' '); }
            out.push_str(&format!("{}", c));
        }
        out.push_str(" sc\n");
        self.buffer.extend_from_slice(out.as_bytes());
        Ok(())
    }

    /// Sets the stroking color using the `SC` operator (for color space with variable components).
    pub fn set_stroke_color_custom(&mut self, components: &[f64]) -> PdfResult<()> {
        let mut out = String::new();
        for (i, c) in components.iter().enumerate() {
            if i > 0 { out.push(' '); }
            out.push_str(&format!("{}", c));
        }
        out.push_str(" SC\n");
        self.buffer.extend_from_slice(out.as_bytes());
        Ok(())
    }

    pub fn save_state(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"q\n");
        Ok(())
    }

    pub fn restore_state(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"Q\n");
        Ok(())
    }

    pub fn transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} {} {} {} {} {} cm\n", a, b, c, d, e, f).as_bytes());
        Ok(())
    }

    pub fn draw_image(&mut self, name: &str, x: f64, y: f64, width: f64, height: f64) -> PdfResult<()> {
        self.save_state()?;
        self.transform(width, 0.0, 0.0, height, x, y)?;
        self.buffer.extend_from_slice(format!("/{} Do\n", name).as_bytes());
        self.restore_state()?;
        Ok(())
    }

    /// Draws a Form XObject (or Image XObject) by its registered name.
    /// Applies a transformation matrix via `cm` before the `Do` invocation.
    pub fn draw_xobject(&mut self, name: &str, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> PdfResult<()> {
        let is_identity = (a - 1.0).abs() < f64::EPSILON
            && b.abs() < f64::EPSILON
            && c.abs() < f64::EPSILON
            && (d - 1.0).abs() < f64::EPSILON
            && e.abs() < f64::EPSILON
            && f.abs() < f64::EPSILON;
        if !is_identity {
            self.transform(a, b, c, d, e, f)?;
        }
        self.buffer.extend_from_slice(format!("/{} Do\n", name).as_bytes());
        Ok(())
    }

    pub fn register_image_xobject_rgb(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        rgb_data: &[u8],
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        Self::validate_raw_image_length(width, height, 3, rgb_data.len(), "RGB")?;

        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            rgb_data,
            None,
            CosObject::Name(CosName::new(b"DeviceRGB".to_vec())),
            None,
            None,
        )
    }

    pub fn register_image_xobject_gray8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        gray_data: &[u8],
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        Self::validate_raw_image_length(width, height, 1, gray_data.len(), "gray")?;

        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            gray_data,
            None,
            CosObject::Name(CosName::new(b"DeviceGray".to_vec())),
            None,
            None,
        )
    }

    pub fn register_image_xobject_cmyk8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        cmyk_data: &[u8],
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        Self::validate_raw_image_length(width, height, 4, cmyk_data.len(), "CMYK")?;

        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            cmyk_data,
            None,
            CosObject::Name(CosName::new(b"DeviceCMYK".to_vec())),
            None,
            None,
        )
    }

    /// Registers a Form XObject (content stream chunk) into the page's `/Resources`
    /// and returns a unique name for it. The Form XObject content is provided as raw bytes.
    ///
    /// The resulting `name` can be passed to [`draw_xobject`] or [`draw_image`].
    pub fn register_form_xobject(
        &mut self,
        name_hint: Option<&str>,
        content_bytes: Vec<u8>,
        bbox: (f64, f64, f64, f64),
    ) -> PdfResult<String> {
        use crate::cos::CosStream;

        let mut page_dict = self.page_dictionary_clone()?;
        let resources_binding = self.resolve_page_resources_binding(&page_dict);
        let mut resources_dict = match resources_binding {
            PageResourcesBinding::Inline => page_dict
                .get(&CosName::resources())
                .and_then(|v| v.as_dictionary())
                .cloned()
                .unwrap_or_default(),
            PageResourcesBinding::Indirect(resources_id) => self
                .doc
                .get_object_ref(resources_id)
                .and_then(|obj| obj.as_dictionary())
                .cloned()
                .unwrap_or_default(),
            PageResourcesBinding::Missing => self.inherited_resources_dict().unwrap_or_default(),
        };

        let base_name = Self::normalize_xobject_name_hint(name_hint);
        let form_name = Self::make_unique_xobject_name(&resources_dict, &base_name);

        let form_id = self.doc.allocate_object_id();

        let mut form_dict = CosDictionary::new();
        form_dict.insert(CosName::type_name(), CosObject::Name(CosName::new(b"XObject".to_vec())));
        form_dict.insert(CosName::subtype(), CosObject::Name(CosName::new(b"Form".to_vec())));
        form_dict.insert(CosName::new(b"BBox".to_vec()), CosObject::Array(vec![
            CosObject::Real(bbox.0),
            CosObject::Real(bbox.1),
            CosObject::Real(bbox.2),
            CosObject::Real(bbox.3),
        ]));
        form_dict.insert(CosName::new(b"Length".to_vec()), CosObject::Integer(content_bytes.len() as i64));

        let form_stream = CosStream::new(form_dict, content_bytes);
        self.doc.insert_object(form_id, CosObject::Stream(form_stream));
        self.doc.xref.insert_if_absent(
            form_id,
            crate::parser::xref::XRefEntry::InUse { offset: 0, generation: 0 },
        );

        Self::insert_xobject_resource(&mut resources_dict, &form_name, form_id);

        match resources_binding {
            PageResourcesBinding::Inline | PageResourcesBinding::Missing => {
                page_dict.insert(CosName::resources(), CosObject::Dictionary(resources_dict));
                self.doc.insert_object(self.page_id, CosObject::Dictionary(page_dict));
            }
            PageResourcesBinding::Indirect(resources_id) => {
                self.doc.insert_object(resources_id, CosObject::Dictionary(resources_dict));
            }
        }

        Ok(form_name)
    }

    pub fn register_image_xobject_dct_rgb8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        dct_data: &[u8],
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        if dct_data.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "encoded image payload must not be empty".to_string(),
            });
        }
        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            dct_data,
            Some(CosName::new(b"DCTDecode".to_vec())),
            CosObject::Name(CosName::new(b"DeviceRGB".to_vec())),
            None,
            None,
        )
    }

    pub fn register_image_xobject_dct_gray8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        dct_data: &[u8],
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        if dct_data.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "encoded image payload must not be empty".to_string(),
            });
        }
        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            dct_data,
            Some(CosName::new(b"DCTDecode".to_vec())),
            CosObject::Name(CosName::new(b"DeviceGray".to_vec())),
            None,
            None,
        )
    }

    pub fn register_image_xobject_dct_cmyk8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        dct_data: &[u8],
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        if dct_data.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "encoded image payload must not be empty".to_string(),
            });
        }
        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            dct_data,
            Some(CosName::new(b"DCTDecode".to_vec())),
            CosObject::Name(CosName::new(b"DeviceCMYK".to_vec())),
            None,
            None,
        )
    }

    pub fn register_image_xobject_flate_rgb8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        flate_data: &[u8],
    ) -> PdfResult<String> {
        self.register_image_xobject_flate_rgb8_with_decode_parms(
            name_hint,
            width,
            height,
            flate_data,
            None,
        )
    }

    pub fn register_image_xobject_flate_rgb8_with_decode_parms(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        flate_data: &[u8],
        decode_parms: Option<CosDictionary>,
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        if flate_data.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "encoded image payload must not be empty".to_string(),
            });
        }
        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            flate_data,
            Some(CosName::new(b"FlateDecode".to_vec())),
            CosObject::Name(CosName::new(b"DeviceRGB".to_vec())),
            decode_parms,
            None,
        )
    }

    pub fn register_image_xobject_flate_gray8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        flate_data: &[u8],
    ) -> PdfResult<String> {
        self.register_image_xobject_flate_gray8_with_decode_parms(
            name_hint,
            width,
            height,
            flate_data,
            None,
        )
    }

    pub fn register_image_xobject_flate_gray8_with_decode_parms(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        flate_data: &[u8],
        decode_parms: Option<CosDictionary>,
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        if flate_data.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "encoded image payload must not be empty".to_string(),
            });
        }
        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            flate_data,
            Some(CosName::new(b"FlateDecode".to_vec())),
            CosObject::Name(CosName::new(b"DeviceGray".to_vec())),
            decode_parms,
            None,
        )
    }

    pub fn register_image_xobject_flate_cmyk8(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        flate_data: &[u8],
    ) -> PdfResult<String> {
        self.register_image_xobject_flate_cmyk8_with_decode_parms(
            name_hint,
            width,
            height,
            flate_data,
            None,
        )
    }

    pub fn register_image_xobject_flate_cmyk8_with_decode_parms(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        flate_data: &[u8],
        decode_parms: Option<CosDictionary>,
    ) -> PdfResult<String> {
        Self::validate_nonzero_dimensions(width, height)?;
        if flate_data.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "encoded image payload must not be empty".to_string(),
            });
        }
        self.register_image_xobject_core(
            name_hint,
            width,
            height,
            flate_data,
            Some(CosName::new(b"FlateDecode".to_vec())),
            CosObject::Name(CosName::new(b"DeviceCMYK".to_vec())),
            decode_parms,
            None,
        )
    }

    pub fn register_image_xobject_png(
        &mut self,
        name_hint: Option<&str>,
        png_data: &[u8],
    ) -> PdfResult<String> {
        if png_data.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "png payload must not be empty".to_string(),
            });
        }

        if let Some(indexed_name) = self.try_register_indexed_png(name_hint, png_data)? {
            return Ok(indexed_name);
        }

        let decoded = image::load_from_memory_with_format(png_data, image::ImageFormat::Png)
            .map_err(|err| crate::PdfError::Parse {
                offset: None,
                context: format!("failed to decode PNG payload: {}", err),
            })?;

        let color = decoded.color();
        if color.has_alpha() {
            if matches!(color, image::ColorType::La8 | image::ColorType::La16) {
                let gray_alpha = decoded.to_luma_alpha8();
                let width = gray_alpha.width();
                let height = gray_alpha.height();
                let mut gray = Vec::with_capacity((width as usize) * (height as usize));
                let mut alpha = Vec::with_capacity((width as usize) * (height as usize));
                for px in gray_alpha.pixels() {
                    gray.push(px[0]);
                    alpha.push(px[1]);
                }
                Self::validate_raw_image_length(width, height, 1, gray.len(), "gray")?;
                Self::validate_raw_image_length(width, height, 1, alpha.len(), "alpha")?;
                let smask = if alpha.iter().any(|a| *a != 255) {
                    Some(alpha)
                } else {
                    None
                };
                self.register_image_xobject_core(
                    name_hint,
                    width,
                    height,
                    &gray,
                    None,
                    CosObject::Name(CosName::new(b"DeviceGray".to_vec())),
                    None,
                    smask,
                )
            } else {
                let rgba = decoded.to_rgba8();
                let width = rgba.width();
                let height = rgba.height();
                let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
                let mut alpha = Vec::with_capacity((width as usize) * (height as usize));
                for px in rgba.pixels() {
                    rgb.push(px[0]);
                    rgb.push(px[1]);
                    rgb.push(px[2]);
                    alpha.push(px[3]);
                }
                Self::validate_raw_image_length(width, height, 3, rgb.len(), "RGB")?;
                Self::validate_raw_image_length(width, height, 1, alpha.len(), "alpha")?;
                let smask = if alpha.iter().any(|a| *a != 255) {
                    Some(alpha)
                } else {
                    None
                };
                self.register_image_xobject_core(
                    name_hint,
                    width,
                    height,
                    &rgb,
                    None,
                    CosObject::Name(CosName::new(b"DeviceRGB".to_vec())),
                    None,
                    smask,
                )
            }
        } else {
            match color {
                image::ColorType::L8 | image::ColorType::L16 => {
                    let gray = decoded.to_luma8();
                    self.register_image_xobject_gray8(
                        name_hint,
                        gray.width(),
                        gray.height(),
                        gray.as_raw(),
                    )
                }
                _ => {
                    let rgb = decoded.to_rgb8();
                    self.register_image_xobject_rgb(
                        name_hint,
                        rgb.width(),
                        rgb.height(),
                        rgb.as_raw(),
                    )
                }
            }
        }
    }

    pub fn draw_registered_image(
        &mut self,
        name: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> PdfResult<()> {
        Self::validate_xobject_name(name)?;
        if !x.is_finite() || !y.is_finite() || !width.is_finite() || !height.is_finite() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "image placement values must be finite numbers".to_string(),
            });
        }
        if width <= 0.0 || height <= 0.0 {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "image width/height must be greater than zero".to_string(),
            });
        }
        if !self.page_has_xobject(name) {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: format!("XObject '/{}' is not registered on this page", name),
            });
        }
        self.draw_image(name, x, y, width, height)
    }

    fn page_dictionary_clone(&self) -> PdfResult<CosDictionary> {
        self.doc
            .get_object_ref(self.page_id)
            .and_then(|obj| obj.as_dictionary())
            .cloned()
            .ok_or_else(|| crate::PdfError::Parse {
                offset: None,
                context: "page object is missing or not a dictionary".to_string(),
            })
    }

    fn register_image_xobject_core(
        &mut self,
        name_hint: Option<&str>,
        width: u32,
        height: u32,
        image_data: &[u8],
        filter: Option<CosName>,
        color_space: CosObject,
        decode_parms: Option<CosDictionary>,
        smask_data: Option<Vec<u8>>,
    ) -> PdfResult<String> {
        let mut page_dict = self.page_dictionary_clone()?;
        let resources_binding = self.resolve_page_resources_binding(&page_dict);
        let mut resources_dict = match resources_binding {
            PageResourcesBinding::Inline => page_dict
                .get(&CosName::resources())
                .and_then(|v| v.as_dictionary())
                .cloned()
                .unwrap_or_default(),
            PageResourcesBinding::Indirect(resources_id) => self
                .doc
                .get_object_ref(resources_id)
                .and_then(|obj| obj.as_dictionary())
                .cloned()
                .unwrap_or_default(),
            PageResourcesBinding::Missing => self.inherited_resources_dict().unwrap_or_default(),
        };

        let base_name = Self::normalize_xobject_name_hint(name_hint);
        let image_name = Self::make_unique_xobject_name(&resources_dict, &base_name);

        let image_id = self.doc.allocate_object_id();
        let smask_id = if let Some(alpha) = smask_data {
            Self::validate_raw_image_length(width, height, 1, alpha.len(), "alpha")?;
            let smask_id = self.doc.allocate_object_id();
            let mut smask_dict = CosDictionary::new();
            smask_dict.insert(
                CosName::type_name(),
                CosObject::Name(CosName::new(b"XObject".to_vec())),
            );
            smask_dict.insert(
                CosName::subtype(),
                CosObject::Name(CosName::new(b"Image".to_vec())),
            );
            smask_dict.insert(CosName::new(b"Width".to_vec()), CosObject::Integer(width as i64));
            smask_dict.insert(
                CosName::new(b"Height".to_vec()),
                CosObject::Integer(height as i64),
            );
            smask_dict.insert(
                CosName::new(b"ColorSpace".to_vec()),
                CosObject::Name(CosName::new(b"DeviceGray".to_vec())),
            );
            smask_dict.insert(
                CosName::new(b"BitsPerComponent".to_vec()),
                CosObject::Integer(8),
            );
            smask_dict.insert(CosName::length(), CosObject::Integer(alpha.len() as i64));
            let smask_stream = crate::cos::CosStream::new(smask_dict, alpha);
            self.doc.insert_object(smask_id, CosObject::Stream(smask_stream));
            self.doc.xref.insert_if_absent(
                smask_id,
                crate::parser::xref::XRefEntry::InUse {
                    offset: 0,
                    generation: 0,
                },
            );
            Some(smask_id)
        } else {
            None
        };

        let mut image_dict = CosDictionary::new();
        image_dict.insert(
            CosName::type_name(),
            CosObject::Name(CosName::new(b"XObject".to_vec())),
        );
        image_dict.insert(
            CosName::subtype(),
            CosObject::Name(CosName::new(b"Image".to_vec())),
        );
        image_dict.insert(
            CosName::new(b"Width".to_vec()),
            CosObject::Integer(width as i64),
        );
        image_dict.insert(
            CosName::new(b"Height".to_vec()),
            CosObject::Integer(height as i64),
        );
        image_dict.insert(
            CosName::new(b"ColorSpace".to_vec()),
            color_space,
        );
        image_dict.insert(
            CosName::new(b"BitsPerComponent".to_vec()),
            CosObject::Integer(8),
        );
        image_dict.insert(CosName::length(), CosObject::Integer(image_data.len() as i64));
        if let Some(filter_name) = &filter {
            image_dict.insert(CosName::filter(), CosObject::Name(filter_name.clone()));
        }
        if let Some(decode_parms_dict) = decode_parms {
            let is_flate = filter
                .as_ref()
                .and_then(|name| name.as_str())
                .map(|name| name == "FlateDecode")
                .unwrap_or(false);
            if !is_flate {
                return Err(crate::PdfError::Parse {
                    offset: None,
                    context: "DecodeParms are only supported for FlateDecode image helpers"
                        .to_string(),
                });
            }
            Self::validate_flate_decode_parms(&decode_parms_dict)?;
            image_dict.insert(
                CosName::new(b"DecodeParms".to_vec()),
                CosObject::Dictionary(decode_parms_dict),
            );
        }
        if let Some(mask_id) = smask_id {
            image_dict.insert(
                CosName::new(b"SMask".to_vec()),
                CosObject::Reference(mask_id),
            );
        }

        let image_stream = crate::cos::CosStream::new(image_dict, image_data.to_vec());
        self.doc.insert_object(image_id, CosObject::Stream(image_stream));
        self.doc.xref.insert_if_absent(
            image_id,
            crate::parser::xref::XRefEntry::InUse {
                offset: 0,
                generation: 0,
            },
        );

        Self::insert_xobject_resource(&mut resources_dict, &image_name, image_id);

        match resources_binding {
            PageResourcesBinding::Inline | PageResourcesBinding::Missing => {
                page_dict.insert(CosName::resources(), CosObject::Dictionary(resources_dict));
                self.doc
                    .insert_object(self.page_id, CosObject::Dictionary(page_dict));
            }
            PageResourcesBinding::Indirect(resources_id) => {
                self.doc
                    .insert_object(resources_id, CosObject::Dictionary(resources_dict));
            }
        }

        Ok(image_name)
    }

    fn try_register_indexed_png(
        &mut self,
        name_hint: Option<&str>,
        png_data: &[u8],
    ) -> PdfResult<Option<String>> {
        let cursor = std::io::Cursor::new(png_data);
        let decoder = png::Decoder::new(cursor);
        let mut reader = decoder.read_info().map_err(|err| crate::PdfError::Parse {
            offset: None,
            context: format!("failed to parse PNG header: {}", err),
        })?;

        let info = reader.info();
        if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
            return Ok(None);
        }
        if info.trns.is_some() {
            return Ok(None);
        }

        let Some(palette_bytes) = info.palette.clone() else {
            return Ok(None);
        };
        if palette_bytes.is_empty() || palette_bytes.len() % 3 != 0 {
            return Ok(None);
        }
        let palette_entries = palette_bytes.len() / 3;
        if palette_entries > 256 {
            return Ok(None);
        }

        let mut frame_data = vec![0u8; reader.output_buffer_size()];
        let frame = reader
            .next_frame(&mut frame_data)
            .map_err(|err| crate::PdfError::Parse {
                offset: None,
                context: format!("failed to decode indexed PNG pixels: {}", err),
            })?;
        let width = frame.width;
        let height = frame.height;
        let pixel_indices = &frame_data[..frame.buffer_size()];
        Self::validate_raw_image_length(width, height, 1, pixel_indices.len(), "indexed")?;

        let indexed_color_space = CosObject::Array(vec![
            CosObject::Name(CosName::new(b"Indexed".to_vec())),
            CosObject::Name(CosName::new(b"DeviceRGB".to_vec())),
            CosObject::Integer((palette_entries - 1) as i64),
            CosObject::String(palette_bytes.to_vec()),
        ]);

        let image_name = self.register_image_xobject_core(
            name_hint,
            width,
            height,
            pixel_indices,
            None,
            indexed_color_space,
            None,
            None,
        )?;

        Ok(Some(image_name))
    }

    fn validate_nonzero_dimensions(width: u32, height: u32) -> PdfResult<()> {
        if width == 0 || height == 0 {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "image dimensions must be greater than zero".to_string(),
            });
        }
        Ok(())
    }

    fn validate_raw_image_length(
        width: u32,
        height: u32,
        channels: usize,
        actual_len: usize,
        label: &str,
    ) -> PdfResult<()> {
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|px| px.checked_mul(channels))
            .ok_or_else(|| crate::PdfError::Parse {
                offset: None,
                context: format!(
                    "image dimensions overflow while validating {} buffer",
                    label
                ),
            })?;
        if actual_len != expected_len {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: format!(
                    "invalid {} buffer length: expected {}, got {}",
                    label, expected_len, actual_len
                ),
            });
        }
        Ok(())
    }

    fn validate_flate_decode_parms(parms: &CosDictionary) -> PdfResult<()> {
        Self::validate_decode_parm_int(parms, b"Predictor", Some(1), Some(15))?;
        Self::validate_decode_parm_int(parms, b"Colors", Some(1), None)?;
        Self::validate_decode_parm_int(parms, b"BitsPerComponent", Some(1), None)?;
        Self::validate_decode_parm_int(parms, b"Columns", Some(1), None)?;
        Ok(())
    }

    fn validate_decode_parm_int(
        parms: &CosDictionary,
        key: &[u8],
        min: Option<i64>,
        max: Option<i64>,
    ) -> PdfResult<()> {
        let key_name = CosName::new(key.to_vec());
        let Some(value_obj) = parms.get(&key_name) else {
            return Ok(());
        };
        let Some(value) = value_obj.as_integer() else {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: format!(
                    "DecodeParms '{}' must be an integer",
                    String::from_utf8_lossy(key)
                ),
            });
        };
        if let Some(min_value) = min {
            if value < min_value {
                return Err(crate::PdfError::Parse {
                    offset: None,
                    context: format!(
                        "DecodeParms '{}' must be >= {}",
                        String::from_utf8_lossy(key),
                        min_value
                    ),
                });
            }
        }
        if let Some(max_value) = max {
            if value > max_value {
                return Err(crate::PdfError::Parse {
                    offset: None,
                    context: format!(
                        "DecodeParms '{}' must be <= {}",
                        String::from_utf8_lossy(key),
                        max_value
                    ),
                });
            }
        }
        Ok(())
    }


    fn resolve_page_resources_binding(&self, page_dict: &CosDictionary) -> PageResourcesBinding {
        let resources_key = CosName::resources();
        match page_dict.get(&resources_key) {
            Some(CosObject::Dictionary(_)) => PageResourcesBinding::Inline,
            Some(CosObject::Reference(resources_id)) => {
                if self
                    .doc
                    .get_object_ref(*resources_id)
                    .and_then(|obj| obj.as_dictionary())
                    .is_some()
                {
                    PageResourcesBinding::Indirect(*resources_id)
                } else {
                    PageResourcesBinding::Missing
                }
            }
            _ => PageResourcesBinding::Missing,
        }
    }

    fn inherited_resources_dict(&self) -> Option<CosDictionary> {
        let mut current_id = self.page_id;
        for _ in 0..32 {
            let current_dict = self
                .doc
                .get_object_ref(current_id)
                .and_then(|obj| obj.as_dictionary())?;
            if let Some(resources_obj) = current_dict.get(&CosName::resources()) {
                return match resources_obj {
                    CosObject::Dictionary(dict) => Some(dict.clone()),
                    CosObject::Reference(resources_id) => self
                        .doc
                        .get_object_ref(*resources_id)
                        .and_then(|obj| obj.as_dictionary())
                        .cloned(),
                    _ => None,
                };
            }
            let parent_id = current_dict
                .get(&CosName::new(b"Parent".to_vec()))
                .and_then(|obj| obj.as_reference())?;
            current_id = parent_id;
        }
        None
    }

    fn insert_xobject_resource(resources_dict: &mut CosDictionary, image_name: &str, image_id: ObjectId) {
        let xobject_key = CosName::new(b"XObject".to_vec());
        let mut xobjects = resources_dict
            .get(&xobject_key)
            .and_then(|obj| obj.as_dictionary())
            .cloned()
            .unwrap_or_default();
        xobjects.insert(
            CosName::new(image_name.as_bytes().to_vec()),
            CosObject::Reference(image_id),
        );
        resources_dict.insert(xobject_key, CosObject::Dictionary(xobjects));
    }

    fn page_has_xobject(&self, name: &str) -> bool {
        let target = CosName::new(name.as_bytes().to_vec());
        let mut current_id = self.page_id;
        for _ in 0..32 {
            let Some(current_dict) = self
                .doc
                .get_object_ref(current_id)
                .and_then(|obj| obj.as_dictionary())
            else {
                return false;
            };

            if let Some(resources_obj) = current_dict.get(&CosName::resources()) {
                let found = match resources_obj {
                    CosObject::Dictionary(resources_dict) => resources_dict
                        .get(&CosName::new(b"XObject".to_vec()))
                        .and_then(|obj| obj.as_dictionary())
                        .map(|xobjects| xobjects.contains_key(&target))
                        .unwrap_or(false),
                    CosObject::Reference(resources_id) => self
                        .doc
                        .get_object_ref(*resources_id)
                        .and_then(|obj| obj.as_dictionary())
                        .and_then(|resources_dict| resources_dict.get(&CosName::new(b"XObject".to_vec())))
                        .and_then(|obj| obj.as_dictionary())
                        .map(|xobjects| xobjects.contains_key(&target))
                        .unwrap_or(false),
                    _ => false,
                };
                if found {
                    return true;
                }
            }

            let Some(parent_id) = current_dict
                .get(&CosName::new(b"Parent".to_vec()))
                .and_then(|obj| obj.as_reference())
            else {
                return false;
            };
            current_id = parent_id;
        }
        false
    }

    fn normalize_xobject_name_hint(name_hint: Option<&str>) -> String {
        let mut out = String::new();
        for byte in name_hint.unwrap_or("Im").bytes() {
            if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' {
                out.push(byte as char);
            }
        }
        if out.is_empty() {
            out.push_str("Im");
        }
        if !out.as_bytes()[0].is_ascii_alphabetic() {
            out.insert_str(0, "Im");
        }
        out
    }

    fn make_unique_xobject_name(resources_dict: &CosDictionary, base_name: &str) -> String {
        let xobject_key = CosName::new(b"XObject".to_vec());
        let xobjects = resources_dict
            .get(&xobject_key)
            .and_then(|obj| obj.as_dictionary());

        if let Some(xobjects) = xobjects {
            if !xobjects.contains_key(&CosName::new(base_name.as_bytes().to_vec())) {
                return base_name.to_string();
            }
            for idx in 1..=u32::MAX {
                let candidate = format!("{}{}", base_name, idx);
                if !xobjects.contains_key(&CosName::new(candidate.as_bytes().to_vec())) {
                    return candidate;
                }
            }
        }

        base_name.to_string()
    }

    fn validate_xobject_name(name: &str) -> PdfResult<()> {
        if name.is_empty() {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: "XObject name must not be empty".to_string(),
            });
        }
        let valid = name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !valid {
            return Err(crate::PdfError::Parse {
                offset: None,
                context: format!(
                    "invalid XObject name '{}': only ASCII letters, digits, '_' and '-' are allowed",
                    name
                ),
            });
        }
        Ok(())
    }

    pub fn close(self) -> PdfResult<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let stream_id = self.doc.allocate_object_id();
        let mut dict = CosDictionary::new();
        dict.insert(CosName::new(b"Length".to_vec()), CosObject::Integer(self.buffer.len() as i64));
        let stream = crate::cos::CosStream::new(dict, self.buffer);
        self.doc.insert_object(stream_id, CosObject::Stream(stream));
        self.doc.xref.insert_if_absent(stream_id, crate::parser::xref::XRefEntry::InUse { offset: 0, generation: 0 });

        let page_id = self.page_id;
        self.doc.mutate_object(page_id, |obj| {
            if let CosObject::Dictionary(page_dict) = obj {
                let contents_name = CosName::new(b"Contents".to_vec());
                if let Some(existing) = page_dict.get(&contents_name) {
                    match existing {
                        CosObject::Array(arr) => {
                            let mut new_arr = arr.clone();
                            new_arr.push(CosObject::Reference(stream_id));
                            page_dict.insert(contents_name, CosObject::Array(new_arr));
                        }
                        CosObject::Reference(existing_id) => {
                            // Convert single ref to array
                            let new_arr = vec![CosObject::Reference(*existing_id), CosObject::Reference(stream_id)];
                            page_dict.insert(contents_name, CosObject::Array(new_arr));
                        }
                        _ => {
                            // Overwrite if it's something weird
                            page_dict.insert(contents_name, CosObject::Reference(stream_id));
                        }
                    }
                } else {
                    page_dict.insert(contents_name, CosObject::Reference(stream_id));
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum PageResourcesBinding {
    Inline,
    Indirect(ObjectId),
    Missing,
}

