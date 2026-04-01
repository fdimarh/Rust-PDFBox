//! Top-level COS object enum.
//!
//! Maps to Java PDFBox `COSBase` / `COSObject` hierarchy. Every value
//! that can appear in a PDF file is represented by a variant of
//! [`CosObject`].

use std::fmt;

use super::dictionary::CosDictionary;
use super::name::CosName;
use super::object_id::ObjectId;
use super::stream::CosStream;

/// A single PDF COS-level value.
///
/// This enum covers all eight basic PDF object types plus indirect
/// references. Indirect object *definitions* are stored separately in
/// the object store; `CosObject::Reference` is the in-tree pointer.
#[derive(Debug, Clone, PartialEq)]
pub enum CosObject {
    /// The PDF `null` object.
    Null,
    /// A boolean value (`true` / `false`).
    Bool(bool),
    /// An integer number.
    Integer(i64),
    /// A real (floating-point) number.
    Real(f64),
    /// A byte-string (literal or hex-encoded in syntax).
    String(Vec<u8>),
    /// A name object (decoded bytes, no leading `/`).
    Name(CosName),
    /// An ordered array of objects.
    Array(Vec<CosObject>),
    /// A dictionary (name → object mapping).
    Dictionary(CosDictionary),
    /// A stream (dictionary + raw byte data).
    Stream(CosStream),
    /// An indirect reference to another object by ID.
    Reference(ObjectId),
}

impl CosObject {
    // ---- Type query helpers ----

    /// Returns `true` if this is `Null`.
    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns the boolean value if this is a `Bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns the integer value if this is an `Integer`.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the float value if this is a `Real`.
    pub fn as_real(&self) -> Option<f64> {
        match self {
            Self::Real(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the numeric value as `f64`, accepting both `Integer` and `Real`.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Integer(n) => Some(*n as f64),
            Self::Real(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns a reference to the byte-string if this is a `String`.
    pub fn as_string(&self) -> Option<&[u8]> {
        match self { Self::String(s) => Some(s), _ => None, }
    }

    /// Returns the string bytes decoded as lossy UTF-8, or `None` if not a String.
    pub fn as_string_lossy(&self) -> Option<std::borrow::Cow<'_, str>> {
        match self {
            Self::String(s) => Some(String::from_utf8_lossy(s)),
            _ => None,
        }
    }

    /// Returns a reference to the name if this is a `Name`.
    pub fn as_name(&self) -> Option<&CosName> {
        match self {
            Self::Name(n) => Some(n),
            _ => None,
        }
    }

    /// Returns a reference to the array if this is an `Array`.
    pub fn as_array(&self) -> Option<&[CosObject]> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Returns a reference to the dictionary if this is a `Dictionary`.
    pub fn as_dictionary(&self) -> Option<&CosDictionary> {
        match self {
            Self::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    /// Returns a mutable reference to the dictionary if this is a `Dictionary`.
    pub fn as_dictionary_mut(&mut self) -> Option<&mut CosDictionary> {
        match self {
            Self::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner dictionary if this is a `Dictionary`.
    pub fn into_dictionary(self) -> Option<CosDictionary> {
        match self {
            Self::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    /// Returns a reference to the stream if this is a `Stream`.
    pub fn as_stream(&self) -> Option<&CosStream> {
        match self {
            Self::Stream(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the object ID if this is a `Reference`.
    pub fn as_reference(&self) -> Option<ObjectId> {
        match self {
            Self::Reference(id) => Some(*id),
            _ => None,
        }
    }

    /// Returns a short string tag for the object type (useful for diagnostics).
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Bool(_) => "Bool",
            Self::Integer(_) => "Integer",
            Self::Real(_) => "Real",
            Self::String(_) => "String",
            Self::Name(_) => "Name",
            Self::Array(_) => "Array",
            Self::Dictionary(_) => "Dictionary",
            Self::Stream(_) => "Stream",
            Self::Reference(_) => "Reference",
        }
    }
}

impl fmt::Display for CosObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Integer(n) => write!(f, "{n}"),
            Self::Real(n) => write!(f, "{n}"),
            Self::String(s) => {
                // Hex-string display for simplicity.
                write!(f, "<")?;
                for byte in s {
                    write!(f, "{byte:02X}")?;
                }
                write!(f, ">")
            }
            Self::Name(n) => write!(f, "{n}"),
            Self::Array(arr) => {
                write!(f, "[ ")?;
                for item in arr {
                    write!(f, "{item} ")?;
                }
                write!(f, "]")
            }
            Self::Dictionary(d) => write!(f, "{d}"),
            Self::Stream(s) => write!(f, "{s}"),
            Self::Reference(id) => write!(f, "{id}"),
        }
    }
}

impl From<bool> for CosObject {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for CosObject {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for CosObject {
    fn from(value: f64) -> Self {
        Self::Real(value)
    }
}

impl From<CosName> for CosObject {
    fn from(value: CosName) -> Self {
        Self::Name(value)
    }
}

impl From<CosDictionary> for CosObject {
    fn from(value: CosDictionary) -> Self {
        Self::Dictionary(value)
    }
}

impl From<CosStream> for CosObject {
    fn from(value: CosStream) -> Self {
        Self::Stream(value)
    }
}

impl From<Vec<CosObject>> for CosObject {
    fn from(value: Vec<CosObject>) -> Self {
        Self::Array(value)
    }
}

impl From<ObjectId> for CosObject {
    fn from(value: ObjectId) -> Self {
        Self::Reference(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_display() {
        assert_eq!(CosObject::Null.to_string(), "null");
        assert!(CosObject::Null.is_null());
    }

    #[test]
    fn bool_conversion() {
        let obj = CosObject::from(true);
        assert_eq!(obj.as_bool(), Some(true));
        assert_eq!(obj.type_name(), "Bool");
    }

    #[test]
    fn integer_conversion() {
        let obj = CosObject::from(42_i64);
        assert_eq!(obj.as_integer(), Some(42));
        assert_eq!(obj.as_number(), Some(42.0));
    }

    #[test]
    fn real_conversion() {
        let obj = CosObject::from(3.14_f64);
        assert_eq!(obj.as_real(), Some(3.14));
        assert_eq!(obj.as_number(), Some(3.14));
    }

    #[test]
    fn string_display_hex() {
        let obj = CosObject::String(b"Hi".to_vec());
        assert_eq!(obj.to_string(), "<4869>");
    }

    #[test]
    fn name_conversion() {
        let obj = CosObject::from(CosName::type_name());
        assert_eq!(obj.as_name(), Some(&CosName::type_name()));
    }

    #[test]
    fn array_conversion() {
        let arr = vec![CosObject::Integer(1), CosObject::Integer(2)];
        let obj = CosObject::from(arr);
        assert_eq!(obj.as_array().unwrap().len(), 2);
    }

    #[test]
    fn dictionary_conversion() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::type_name(), CosObject::Name(CosName::page()));
        let obj = CosObject::from(dict.clone());
        assert_eq!(obj.as_dictionary(), Some(&dict));
    }

    #[test]
    fn reference_conversion() {
        let id = ObjectId::new(7, 0);
        let obj = CosObject::from(id);
        assert_eq!(obj.as_reference(), Some(id));
        assert_eq!(obj.to_string(), "7 0 R");
    }

    #[test]
    fn type_names() {
        assert_eq!(CosObject::Null.type_name(), "Null");
        assert_eq!(CosObject::Bool(true).type_name(), "Bool");
        assert_eq!(CosObject::Integer(0).type_name(), "Integer");
        assert_eq!(CosObject::Real(0.0).type_name(), "Real");
        assert_eq!(CosObject::String(vec![]).type_name(), "String");
        assert_eq!(CosObject::Name(CosName::type_name()).type_name(), "Name");
        assert_eq!(CosObject::Array(vec![]).type_name(), "Array");
        assert_eq!(
            CosObject::Dictionary(CosDictionary::new()).type_name(),
            "Dictionary"
        );
        assert_eq!(
            CosObject::Stream(CosStream::empty()).type_name(),
            "Stream"
        );
        assert_eq!(
            CosObject::Reference(ObjectId::new(1, 0)).type_name(),
            "Reference"
        );
    }
}

