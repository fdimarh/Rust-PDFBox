//! PDF dictionary object.
//!
//! Maps to Java PDFBox `COSDictionary`. A dictionary is an associative
//! mapping of [`CosName`] keys to [`CosObject`] values, preserving
//! insertion order.

use std::fmt;

use super::name::CosName;
use super::object::CosObject;

/// An ordered mapping of name keys to COS object values.
///
/// Preserves insertion order for deterministic serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct CosDictionary {
    entries: Vec<(CosName, CosObject)>,
}

impl CosDictionary {
    /// Creates an empty dictionary.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Creates a dictionary with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Returns the number of key-value pairs.
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the dictionary has no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Inserts a key-value pair into the dictionary.
    ///
    /// If the key already exists, replaces the value and returns the old one.
    pub fn insert(&mut self, key: CosName, value: CosObject) -> Option<CosObject> {
        for entry in &mut self.entries {
            if entry.0 == key {
                let old = std::mem::replace(&mut entry.1, value);
                return Some(old);
            }
        }
        self.entries.push((key, value));
        None
    }

    /// Looks up a value by name key.
    pub fn get(&self, key: &CosName) -> Option<&CosObject> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Looks up a value by name key (mutable).
    pub fn get_mut(&mut self, key: &CosName) -> Option<&mut CosObject> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Removes a key and returns its value if present.
    pub fn remove(&mut self, key: &CosName) -> Option<CosObject> {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == key) {
            Some(self.entries.remove(pos).1)
        } else {
            None
        }
    }

    /// Returns `true` if the dictionary contains the given key.
    pub fn contains_key(&self, key: &CosName) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// Iterates over key-value pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&CosName, &CosObject)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    /// Returns a mutable iterator over key-value pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&CosName, &mut CosObject)> {
        self.entries.iter_mut().map(|(k, v)| (&*k, v))
    }

    /// Returns an iterator over the keys.
    pub fn keys(&self) -> impl Iterator<Item = &CosName> {
        self.entries.iter().map(|(k, _)| k)
    }

    /// Returns an iterator over the values.
    pub fn values(&self) -> impl Iterator<Item = &CosObject> {
        self.entries.iter().map(|(_, v)| v)
    }

    /// Alias for `insert` — sets (or replaces) a key-value pair.
    pub fn set(&mut self, key: CosName, value: CosObject) {
        self.insert(key, value);
    }

    /// Gets a numeric value (Integer or Real) as `f64` for a given key.
    pub fn get_number(&self, key: &CosName) -> Option<f64> {
        self.get(key)?.as_number()
    }

    // ---- Convenience typed getters ----

    /// Gets a name value for a given key.
    pub fn get_name(&self, key: &CosName) -> Option<&CosName> {
        match self.get(key)? {
            CosObject::Name(n) => Some(n),
            _ => None,
        }
    }

    /// Gets an integer value for a given key.
    pub fn get_int(&self, key: &CosName) -> Option<i64> {
        match self.get(key)? {
            CosObject::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// Gets a boolean value for a given key.
    pub fn get_bool(&self, key: &CosName) -> Option<bool> {
        match self.get(key)? {
            CosObject::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Gets a string value (raw bytes) for a given key.
    pub fn get_string(&self, key: &CosName) -> Option<&[u8]> {
        match self.get(key)? {
            CosObject::String(s) => Some(s.as_slice()),
            _ => None,
        }
    }

    /// Gets an array reference for a given key.
    pub fn get_array(&self, key: &CosName) -> Option<&[CosObject]> {
        match self.get(key)? {
            CosObject::Array(arr) => Some(arr.as_slice()),
            _ => None,
        }
    }

    /// Gets a sub-dictionary reference for a given key.
    pub fn get_dictionary(&self, key: &CosName) -> Option<&CosDictionary> {
        match self.get(key)? {
            CosObject::Dictionary(d) => Some(d),
            _ => None,
        }
    }
}

impl Default for CosDictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CosDictionary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<< ")?;
        for (key, value) in self.iter() {
            write!(f, "{key} {value} ")?;
        }
        write!(f, ">>")
    }
}

impl FromIterator<(CosName, CosObject)> for CosDictionary {
    fn from_iter<T: IntoIterator<Item = (CosName, CosObject)>>(iter: T) -> Self {
        let entries: Vec<_> = iter.into_iter().collect();
        Self { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::type_name(), CosObject::Name(CosName::page()));
        assert_eq!(
            dict.get(&CosName::type_name()),
            Some(&CosObject::Name(CosName::page()))
        );
    }

    #[test]
    fn insert_replaces() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::type_name(), CosObject::Bool(true));
        let old = dict.insert(CosName::type_name(), CosObject::Bool(false));
        assert_eq!(old, Some(CosObject::Bool(true)));
        assert_eq!(dict.get_bool(&CosName::type_name()), Some(false));
    }

    #[test]
    fn remove() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::type_name(), CosObject::Integer(42));
        let removed = dict.remove(&CosName::type_name());
        assert_eq!(removed, Some(CosObject::Integer(42)));
        assert!(dict.is_empty());
    }

    #[test]
    fn contains_key() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::length(), CosObject::Integer(100));
        assert!(dict.contains_key(&CosName::length()));
        assert!(!dict.contains_key(&CosName::type_name()));
    }

    #[test]
    fn typed_getters() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::type_name(), CosObject::Name(CosName::catalog()));
        dict.insert(CosName::count(), CosObject::Integer(5));
        dict.insert(CosName::new(b"Flag".to_vec()), CosObject::Bool(true));
        dict.insert(
            CosName::new(b"Title".to_vec()),
            CosObject::String(b"Hello".to_vec()),
        );

        assert_eq!(dict.get_name(&CosName::type_name()), Some(&CosName::catalog()));
        assert_eq!(dict.get_int(&CosName::count()), Some(5));
        assert_eq!(dict.get_bool(&CosName::new(b"Flag".to_vec())), Some(true));
        assert_eq!(
            dict.get_string(&CosName::new(b"Title".to_vec())),
            Some(b"Hello".as_slice())
        );
    }

    #[test]
    fn display_format() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::type_name(), CosObject::Name(CosName::page()));
        let s = dict.to_string();
        assert!(s.starts_with("<<"));
        assert!(s.ends_with(">>"));
        assert!(s.contains("/Type"));
    }

    #[test]
    fn from_iterator() {
        let dict: CosDictionary = vec![
            (CosName::type_name(), CosObject::Name(CosName::page())),
            (CosName::count(), CosObject::Integer(3)),
        ]
        .into_iter()
        .collect();
        assert_eq!(dict.len(), 2);
    }

    #[test]
    fn preserves_insertion_order() {
        let mut dict = CosDictionary::new();
        dict.insert(CosName::new(b"B".to_vec()), CosObject::Null);
        dict.insert(CosName::new(b"A".to_vec()), CosObject::Null);
        dict.insert(CosName::new(b"C".to_vec()), CosObject::Null);

        let keys: Vec<_> = dict.keys().collect();
        assert_eq!(keys[0].as_str(), Some("B"));
        assert_eq!(keys[1].as_str(), Some("A"));
        assert_eq!(keys[2].as_str(), Some("C"));
    }
}

