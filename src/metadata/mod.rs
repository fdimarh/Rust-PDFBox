use std::borrow::Cow;

use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, PdfError, PdfResult};

pub mod xmp;
pub use xmp::XmpMetadata;

pub struct DocumentInfo<'a> {
    dict: Option<&'a CosDictionary>,
}

impl<'a> DocumentInfo<'a> {
    fn get_text(&self, key: &[u8]) -> Option<Cow<'a, str>> {
        let value = self.dict?.get(&CosName::new(key.to_vec()))?;
        match value {
            CosObject::String(bytes) | CosObject::HexString(bytes) => {
                Some(String::from_utf8_lossy(bytes))
            }
            _ => None,
        }
    }

    pub fn title(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"Title")
    }

    pub fn author(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"Author")
    }

    pub fn subject(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"Subject")
    }

    pub fn keywords(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"Keywords")
    }

    pub fn creator(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"Creator")
    }

    pub fn producer(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"Producer")
    }

    pub fn creation_date(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"CreationDate")
    }

    pub fn mod_date(&self) -> Option<Cow<'a, str>> {
        self.get_text(b"ModDate")
    }
}

pub struct DocumentInfoMut<'a> {
    doc: &'a mut Document,
    info_id: ObjectId,
}

impl<'a> DocumentInfoMut<'a> {
    fn dict_mut(&mut self) -> PdfResult<&mut CosDictionary> {
        let Some(obj) = self.doc.objects.get_mut(&self.info_id) else {
            return Err(PdfError::Xref {
                object_id: Some(self.info_id),
            });
        };
        obj.as_dictionary_mut().ok_or_else(|| PdfError::Parse {
            offset: None,
            context: format!("/Info object {:?} is not a dictionary", self.info_id),
        })
    }

    fn set_text(&mut self, key: &[u8], value: &str) -> PdfResult<()> {
        let dict = self.dict_mut()?;
        dict.insert(
            CosName::new(key.to_vec()),
            CosObject::String(value.as_bytes().to_vec()),
        );
        Ok(())
    }

    pub fn set_title(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"Title", value)
    }

    pub fn set_author(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"Author", value)
    }
}

impl Document {
    pub fn document_info(&self) -> DocumentInfo<'_> {
        let dict = self
            .info_id()
            .and_then(|id| self.objects.get(&id))
            .and_then(|o| o.as_dictionary());
        DocumentInfo { dict }
    }

    pub fn document_info_mut(&mut self) -> PdfResult<DocumentInfoMut<'_>> {
        let info_name = CosName::new(b"Info".to_vec());

        let info_id = match self.xref.trailer.get(&info_name).cloned() {
            Some(CosObject::Reference(id)) => {
                if self
                    .objects
                    .get(&id)
                    .and_then(|o| o.as_dictionary())
                    .is_none()
                {
                    self.insert_object(id, CosObject::Dictionary(CosDictionary::new()));
                }
                id
            }
            Some(CosObject::Dictionary(dict)) => {
                let id = self.allocate_object_id();
                self.insert_object(id, CosObject::Dictionary(dict));
                self.xref.trailer.insert(info_name.clone(), CosObject::Reference(id));
                id
            }
            Some(_) | None => {
                let id = self.allocate_object_id();
                self.insert_object(id, CosObject::Dictionary(CosDictionary::new()));
                self.xref.trailer.insert(info_name.clone(), CosObject::Reference(id));
                id
            }
        };

        Ok(DocumentInfoMut { doc: self, info_id })
    }

    pub fn xmp_metadata(&self) -> Option<XmpMetadata> {
        let catalog = self.catalog()?;
        let metadata_obj = catalog.get(&CosName::new(b"Metadata".to_vec()))?;

        let stream = match metadata_obj {
            CosObject::Reference(id) => self.objects.get(id)?.as_stream()?,
            CosObject::Stream(s) => s,
            _ => return None,
        };

        let filter = stream
            .dictionary
            .get(&CosName::new(b"Filter".to_vec()));
        let decoded = crate::io::decode_stream(&stream.data, filter).ok()?;
        XmpMetadata::from_bytes(&decoded)
    }
}

