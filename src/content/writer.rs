use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, PdfResult};

pub struct ContentStreamWriter<'a> {
    doc: &'a mut Document,
    page_id: ObjectId,
    buffer: Vec<u8>,
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

    pub fn show_text(&mut self, text: &str) -> PdfResult<()> {
        // Escaping for PDF strings
        self.buffer.push(b'(');
        for byte in text.as_bytes() {
            match byte {
                b'(' => self.buffer.extend_from_slice(b"\\("),
                b')' => self.buffer.extend_from_slice(b"\\)"),
                b'\\' => self.buffer.extend_from_slice(b"\\\\"),
                _ => self.buffer.push(*byte),
            }
        }
        self.buffer.extend_from_slice(b") Tj\n");
        Ok(())
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

    pub fn stroke(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"S\n");
        Ok(())
    }

    pub fn fill(&mut self) -> PdfResult<()> {
        self.buffer.extend_from_slice(b"f\n");
        Ok(())
    }

    pub fn set_line_width(&mut self, width: f64) -> PdfResult<()> {
        self.buffer.extend_from_slice(format!("{} w\n", width).as_bytes());
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

