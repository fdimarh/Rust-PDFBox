use std::borrow::Cow;

use crate::cos::{CosDictionary, CosName, CosObject, CosStream, ObjectId};
use crate::{Document, PdfError, PdfResult};

pub mod xmp;
pub use xmp::XmpMetadata;
pub mod sync;
pub use sync::SyncPolicy;

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

    pub fn set_subject(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"Subject", value)
    }

    pub fn set_keywords(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"Keywords", value)
    }

    pub fn set_creator(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"Creator", value)
    }

    pub fn set_producer(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"Producer", value)
    }

    pub fn set_creation_date(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"CreationDate", value)
    }

    pub fn set_mod_date(&mut self, value: &str) -> PdfResult<()> {
        self.set_text(b"ModDate", value)
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
                self.xref
                    .trailer
                    .insert(info_name.clone(), CosObject::Reference(id));
                id
            }
            Some(_) | None => {
                let id = self.allocate_object_id();
                self.insert_object(id, CosObject::Dictionary(CosDictionary::new()));
                self.xref
                    .trailer
                    .insert(info_name.clone(), CosObject::Reference(id));
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

        let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
        let decoded = crate::io::decode_stream(&stream.data, filter).ok()?;
        XmpMetadata::from_bytes(&decoded)
    }

    pub fn set_xmp_metadata_raw(&mut self, xml: &str) -> PdfResult<()> {
        let catalog_id = self.catalog_id().ok_or_else(|| PdfError::Parse {
            offset: None,
            context: "cannot resolve catalog object id".to_string(),
        })?;

        let metadata_name = CosName::new(b"Metadata".to_vec());
        let metadata_id = self
            .objects
            .get(&catalog_id)
            .and_then(|o| o.as_dictionary())
            .and_then(|d| d.get(&metadata_name))
            .and_then(|v| v.as_reference())
            .unwrap_or_else(|| self.allocate_object_id());

        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"Type".to_vec()),
            CosObject::Name(CosName::new(b"Metadata".to_vec())),
        );
        dict.insert(
            CosName::new(b"Subtype".to_vec()),
            CosObject::Name(CosName::new(b"XML".to_vec())),
        );
        dict.insert(CosName::length(), CosObject::Integer(xml.len() as i64));

        self.insert_object(
            metadata_id,
            CosObject::Stream(CosStream::new(dict, xml.as_bytes().to_vec())),
        );

        self.mutate_object(catalog_id, |obj| {
            if let Some(cat) = obj.as_dictionary_mut() {
                cat.insert(metadata_name.clone(), CosObject::Reference(metadata_id));
            }
        });

        Ok(())
    }

    pub fn sync_docinfo_to_xmp(&mut self) -> PdfResult<()> {
        self.sync_docinfo_to_xmp_with(SyncPolicy::default())
    }

    pub fn sync_docinfo_to_xmp_with(&mut self, policy: SyncPolicy) -> PdfResult<()> {
        sync::sync_docinfo_to_xmp(self, policy)
    }

    pub fn sync_xmp_to_docinfo(&mut self) -> PdfResult<()> {
        self.sync_xmp_to_docinfo_with(SyncPolicy::default())
    }

    pub fn sync_xmp_to_docinfo_with(&mut self, policy: SyncPolicy) -> PdfResult<()> {
        sync::sync_xmp_to_docinfo(self, policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cos::{CosDictionary, CosName, CosObject};

    fn test_doc_with_info(info_dict: CosDictionary) -> Document {
        let info_id = crate::cos::ObjectId::new(1, 0);
        let catalog_id = crate::cos::ObjectId::new(2, 0);
        let pages_id = crate::cos::ObjectId::new(3, 0);

        let mut doc = Document::empty();
        doc.insert_object(
            catalog_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Catalog".to_vec())),
                );
                d.insert(
                    CosName::new(b"Pages".to_vec()),
                    CosObject::Reference(pages_id),
                );
                d
            }),
        );
        doc.insert_object(
            pages_id,
            CosObject::Dictionary({
                let mut d = CosDictionary::new();
                d.insert(
                    CosName::new(b"Type".to_vec()),
                    CosObject::Name(CosName::new(b"Pages".to_vec())),
                );
                d.insert(CosName::new(b"Count".to_vec()), CosObject::Integer(0));
                d.insert(CosName::new(b"Kids".to_vec()), CosObject::Array(vec![]));
                d
            }),
        );
        doc.insert_object(info_id, CosObject::Dictionary(info_dict));
        doc.xref.trailer.insert(
            CosName::new(b"Info".to_vec()),
            CosObject::Reference(info_id),
        );
        doc.xref.trailer.insert(
            CosName::new(b"Root".to_vec()),
            CosObject::Reference(catalog_id),
        );
        doc
    }

    #[test]
    fn test_document_info_getters() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"Title".to_vec()),
            CosObject::String(b"My Title".to_vec()),
        );
        dict.insert(
            CosName::new(b"Author".to_vec()),
            CosObject::String(b"Dimar".to_vec()),
        );
        dict.insert(
            CosName::new(b"Subject".to_vec()),
            CosObject::String(b"Test".to_vec()),
        );
        dict.insert(
            CosName::new(b"Keywords".to_vec()),
            CosObject::String(b"pdf,rust".to_vec()),
        );
        dict.insert(
            CosName::new(b"Creator".to_vec()),
            CosObject::String(b"rust-pdfbox".to_vec()),
        );
        dict.insert(
            CosName::new(b"Producer".to_vec()),
            CosObject::String(b"rust-pdfbox 0.1".to_vec()),
        );
        dict.insert(
            CosName::new(b"CreationDate".to_vec()),
            CosObject::String(b"D:20250101000000Z".to_vec()),
        );
        dict.insert(
            CosName::new(b"ModDate".to_vec()),
            CosObject::String(b"D:20250102000000Z".to_vec()),
        );

        let doc = test_doc_with_info(dict);
        let info = doc.document_info();

        assert_eq!(info.title().as_deref(), Some("My Title"));
        assert_eq!(info.author().as_deref(), Some("Dimar"));
        assert_eq!(info.subject().as_deref(), Some("Test"));
        assert_eq!(info.keywords().as_deref(), Some("pdf,rust"));
        assert_eq!(info.creator().as_deref(), Some("rust-pdfbox"));
        assert_eq!(info.producer().as_deref(), Some("rust-pdfbox 0.1"));
        assert_eq!(info.creation_date().as_deref(), Some("D:20250101000000Z"));
        assert_eq!(info.mod_date().as_deref(), Some("D:20250102000000Z"));
    }

    #[test]
    fn test_document_info_missing_fields() {
        let dict = CosDictionary::new();
        let doc = test_doc_with_info(dict);
        let info = doc.document_info();
        assert!(info.title().is_none());
        assert!(info.author().is_none());
    }

    #[test]
    fn test_document_info_mut_setters() {
        let dict = CosDictionary::new();
        let mut doc = test_doc_with_info(dict);

        {
            let mut info = doc.document_info_mut().unwrap();
            info.set_title("New Title").unwrap();
            info.set_author("New Author").unwrap();
            info.set_subject("New Subject").unwrap();
            info.set_keywords("a,b,c").unwrap();
        }

        let info = doc.document_info();
        assert_eq!(info.title().as_deref(), Some("New Title"));
        assert_eq!(info.author().as_deref(), Some("New Author"));
        assert_eq!(info.subject().as_deref(), Some("New Subject"));
        assert_eq!(info.keywords().as_deref(), Some("a,b,c"));
    }

    #[test]
    fn test_xmp_metadata_none() {
        let dict = CosDictionary::new();
        let doc = test_doc_with_info(dict);
        assert!(doc.xmp_metadata().is_none());
    }

    #[test]
    fn test_set_xmp_metadata_raw() {
        let dict = CosDictionary::new();
        let mut doc = test_doc_with_info(dict);
        let xml = r#"<?xpacket begin='' id='W5M0MpCehiHzreSzNTczkc9d'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/'>
 <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>
  <rdf:Description rdf:about=''
   xmlns:dc='http://purl.org/dc/elements/1.1/'>
   <dc:title>XMP Title</dc:title>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end='w'?>"#;

        doc.set_xmp_metadata_raw(xml).unwrap();
        let xmp = doc.xmp_metadata();
        assert!(xmp.is_some());
        assert_eq!(xmp.unwrap().dc_title(), Some("XMP Title"));
    }

    #[test]
    fn test_pdf_date_conversion() {
        assert_eq!(
            sync::pdf_date_to_xmp("D:20250101120000Z"),
            Some("2025-01-01T12:00:00Z".to_string())
        );
        assert_eq!(
            sync::pdf_date_to_xmp("D:20250101120000+05'30'"),
            Some("2025-01-01T12:00:00+05:30".to_string())
        );
        assert!(sync::pdf_date_to_xmp("2025").is_some());
        assert!(sync::pdf_date_to_xmp("").is_none());
    }

    #[test]
    fn test_document_info_empty_dict() {
        let dict = CosDictionary::new();
        let info = DocumentInfo { dict: Some(&dict) };
        assert!(info.title().is_none());
        assert!(info.author().is_none());
        assert!(info.subject().is_none());
        assert!(info.keywords().is_none());
        assert!(info.creator().is_none());
        assert!(info.producer().is_none());
        assert!(info.creation_date().is_none());
        assert!(info.mod_date().is_none());
    }

    #[test]
    fn test_document_info_get_text_string() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"Title".to_vec()),
            CosObject::String(b"Test Title".to_vec()),
        );
        let info = DocumentInfo { dict: Some(&dict) };
        assert_eq!(info.title(), Some(Cow::Borrowed("Test Title")));
    }

    #[test]
    fn test_document_info_get_text_hex_string() {
        let mut dict = CosDictionary::new();
        dict.insert(
            CosName::new(b"Author".to_vec()),
            CosObject::HexString(b"417574686f72".to_vec()), // "Author" in hex
        );
        let info = DocumentInfo { dict: Some(&dict) };
        // HexString is not decoded, just treated as raw bytes
        assert_eq!(info.author(), Some(Cow::Borrowed("417574686f72")));
    }

    #[test]
    fn test_document_info_none_dict() {
        let info = DocumentInfo { dict: None };
        assert!(info.title().is_none());
    }
}
