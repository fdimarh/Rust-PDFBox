//! Core COS object model and low-level PDF primitives.
//!
//! This module defines the canonical PDF object types as specified in
//! ISO 32000-1. Every parsed PDF element is represented as a [`CosObject`],
//! and indirect references are tracked via [`ObjectId`].
//!
//! # Java PDFBox mapping
//!
//! | Java class | Rust type |
//! |---|---|
//! | `COSNull` | [`CosObject::Null`] |
//! | `COSBoolean` | [`CosObject::Bool`] |
//! | `COSInteger` | [`CosObject::Integer`] |
//! | `COSFloat` | [`CosObject::Real`] |
//! | `COSString` | [`CosObject::String`] |
//! | `COSName` | [`CosName`] / [`CosObject::Name`] |
//! | `COSArray` | [`CosObject::Array`] |
//! | `COSDictionary` | [`CosDictionary`] / [`CosObject::Dictionary`] |
//! | `COSStream` | [`CosStream`] / [`CosObject::Stream`] |
//! | `COSObjectKey` | [`ObjectId`] |
//! | indirect reference | [`CosObject::Reference`] |

mod dictionary;
mod name;
mod object;
mod object_id;
mod stream;

pub use dictionary::CosDictionary;
pub use name::CosName;
pub use object::CosObject;
pub use object_id::ObjectId;
pub use stream::CosStream;
