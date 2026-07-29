use crate::cos::{CosDictionary, CosName, CosObject, ObjectId};
use crate::{Document, PdfError};
use crate::signing::{
    acroform, appearance, ltv, next_free_object_id, page_object_id, patch_byte_range,
    resolve_anchor_rect, SignOptions, SignatureFormat, PadesLevel
};
use crate::writer::{IncrementalWriter, serializer::Serializer};
use std::collections::BTreeMap;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct PreparedSignature {
    pub pdf_bytes: Vec<u8>,
    pub content_to_sign: Vec<u8>,
    pub contents_offset: usize,
    pub contents_hex_len: usize,
    pub reserved_size: usize,
    pub opts: SignOptions,
    pub sub_filter_bytes: Vec<u8>,
    pub date_str: String,
    pub hash_to_sign: Vec<u8>,
    pub file_encryption_key: Option<Vec<u8>>,
    pub sig_id: ObjectId,
}
