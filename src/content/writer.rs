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

