//! Stable indirect object identifier (`object_number`, `generation`).
//!
//! Maps to Java PDFBox `COSObjectKey`.

use std::fmt;

/// Unique key for an indirect PDF object within a file.
///
/// Two objects share identity when both `object_number` and `generation` match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    pub object_number: u32,
    pub generation: u16,
}

impl ObjectId {
    /// Creates a new object identifier.
    #[inline]
    pub fn new(object_number: u32, generation: u16) -> Self {
        Self {
            object_number,
            generation,
        }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} R", self.object_number, self.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format() {
        let id = ObjectId::new(12, 0);
        assert_eq!(id.to_string(), "12 0 R");
    }

    #[test]
    fn equality_and_hashing() {
        use std::collections::HashSet;

        let a = ObjectId::new(1, 0);
        let b = ObjectId::new(1, 0);
        let c = ObjectId::new(2, 0);

        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

    #[test]
    fn ordering() {
        let a = ObjectId::new(1, 0);
        let b = ObjectId::new(2, 0);
        let c = ObjectId::new(2, 1);
        assert!(a < b);
        assert!(b < c);
    }
}

