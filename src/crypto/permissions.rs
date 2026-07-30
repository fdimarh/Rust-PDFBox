//! PDF permission flags for the Standard Security Handler.
//!
//! Maps to Java PDFBox `AccessPermission`.
//!
//! Permission bits are stored in the /P integer of the encryption dictionary.
//! Bits are numbered 1-based (bit 1 = LSB). Bits 1-2 are reserved (0).
//! Bits 7-8 are reserved (1). PDF §7.6.3.2, Table 22.

/// PDF access permission flags (stored in encryption dict /P).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Permissions(u32);

impl Permissions {
    // Bit masks (0-based)
    pub const PRINT: u32 = 1 << 2; // bit 3
    pub const MODIFY_CONTENT: u32 = 1 << 3; // bit 4
    pub const COPY: u32 = 1 << 4; // bit 5
    pub const MODIFY_ANNOTATIONS: u32 = 1 << 5; // bit 6
    pub const FILL_FORMS: u32 = 1 << 8; // bit 9
    pub const EXTRACT_ACCESSIBILITY: u32 = 1 << 9; // bit 10
    pub const ASSEMBLE: u32 = 1 << 10; // bit 11
    pub const PRINT_HIGH_QUALITY: u32 = 1 << 11; // bit 12

    /// All user-controllable permission bits.
    const ALL_USER_BITS: u32 = Self::PRINT
        | Self::MODIFY_CONTENT
        | Self::COPY
        | Self::MODIFY_ANNOTATIONS
        | Self::FILL_FORMS
        | Self::EXTRACT_ACCESSIBILITY
        | Self::ASSEMBLE
        | Self::PRINT_HIGH_QUALITY;

    /// Creates a `Permissions` value from the raw signed /P integer.
    pub fn from_bits_p(p: i32) -> Self {
        Self(p as u32)
    }

    /// Returns the raw signed /P value for storage in the encryption dict.
    ///
    /// Reserved bits 0-1 are cleared; bits 6-7 are set to 1 per spec.
    pub fn to_bits_p(self) -> i32 {
        let raw = self.0;
        let forced = (raw & !0b11) | 0b1100_0000;
        forced as i32
    }

    /// All permissions granted (owner-level access).
    pub fn all_allowed() -> Self {
        Self(Self::ALL_USER_BITS)
    }

    /// No user permissions (most restrictive).
    pub fn none_allowed() -> Self {
        Self(0)
    }

    fn has(&self, flag: u32) -> bool {
        self.0 & flag != 0
    }

    pub fn can_print(&self) -> bool {
        self.has(Self::PRINT)
    }
    pub fn can_modify_content(&self) -> bool {
        self.has(Self::MODIFY_CONTENT)
    }
    pub fn can_copy(&self) -> bool {
        self.has(Self::COPY)
    }
    pub fn can_modify_annotations(&self) -> bool {
        self.has(Self::MODIFY_ANNOTATIONS)
    }
    pub fn can_fill_forms(&self) -> bool {
        self.has(Self::FILL_FORMS)
    }
    pub fn can_extract_for_accessibility(&self) -> bool {
        self.has(Self::EXTRACT_ACCESSIBILITY)
    }
    pub fn can_assemble(&self) -> bool {
        self.has(Self::ASSEMBLE)
    }
    pub fn can_print_high_quality(&self) -> bool {
        self.has(Self::PRINT_HIGH_QUALITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_bits_p() {
        let perms = Permissions(Permissions::PRINT | Permissions::COPY);
        let p = perms.to_bits_p();
        let recovered = Permissions::from_bits_p(p);
        assert!(recovered.can_print());
        assert!(recovered.can_copy());
    }

    #[test]
    fn can_print_false_when_not_set() {
        let perms = Permissions(Permissions::COPY);
        assert!(!perms.can_print());
        assert!(perms.can_copy());
    }

    #[test]
    fn all_allowed_contains_print() {
        let perms = Permissions::all_allowed();
        assert!(perms.can_print());
        assert!(perms.can_copy());
        assert!(perms.can_modify_content());
    }

    #[test]
    fn none_allowed_denies_all() {
        let perms = Permissions::none_allowed();
        assert!(!perms.can_print());
        assert!(!perms.can_copy());
        assert!(!perms.can_fill_forms());
    }

    #[test]
    fn forced_reserved_bits() {
        let perms = Permissions(Permissions::PRINT);
        let p = perms.to_bits_p();
        assert_eq!(p & 0b11, 0, "bits 0-1 must be 0");
        assert_eq!(p & 0b1100_0000, 0b1100_0000, "bits 6-7 must be 1");
    }

    #[test]
    fn can_modify_content() {
        let perms = Permissions(Permissions::MODIFY_CONTENT);
        assert!(perms.can_modify_content());
        assert!(!perms.can_print());
    }

    #[test]
    fn can_modify_annotations() {
        let perms = Permissions(Permissions::MODIFY_ANNOTATIONS);
        assert!(perms.can_modify_annotations());
    }

    #[test]
    fn can_fill_forms() {
        let perms = Permissions(Permissions::FILL_FORMS);
        assert!(perms.can_fill_forms());
        assert!(!perms.can_print());
    }

    #[test]
    fn can_extract_accessibility() {
        let perms = Permissions(Permissions::EXTRACT_ACCESSIBILITY);
        assert!(perms.can_extract_for_accessibility());
    }

    #[test]
    fn can_assemble() {
        let perms = Permissions(Permissions::ASSEMBLE);
        assert!(perms.can_assemble());
    }

    #[test]
    fn can_print_high_quality() {
        let perms = Permissions(Permissions::PRINT_HIGH_QUALITY);
        assert!(perms.can_print_high_quality());
        assert!(!perms.can_print());
    }

    #[test]
    fn round_trip_with_forced_bits() {
        let perms = Permissions(Permissions::PRINT
            | Permissions::COPY
            | Permissions::FILL_FORMS);
        let p = perms.to_bits_p();
        let recovered = Permissions::from_bits_p(p);
        assert!(recovered.can_print());
        assert!(recovered.can_copy());
        assert!(recovered.can_fill_forms());
        // Reserved bits forced
        assert_eq!(p as u32 & 0b11, 0);
        assert_eq!(p as u32 & 0b1100_0000, 0b1100_0000);
    }

    #[test]
    fn permissions_debug_format() {
        let p = Permissions::all_allowed();
        let _ = format!("{:?}", p);
    }

    #[test]
    fn permissions_clone_eq() {
        let a = Permissions::all_allowed();
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn permissions_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Permissions::all_allowed());
        set.insert(Permissions::none_allowed());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn permissions_default_is_none() {
        let p = Permissions::default();
        assert!(!p.can_print());
        assert!(!p.can_copy());
    }

    #[test]
    fn can_extract_false_when_not_set() {
        let p = Permissions(Permissions::PRINT);
        assert!(!p.can_extract_for_accessibility());
    }

    #[test]
    fn can_assemble_false_when_not_set() {
        let p = Permissions(Permissions::PRINT);
        assert!(!p.can_assemble());
    }

    #[test]
    fn can_print_high_quality_false_when_only_print() {
        let p = Permissions(Permissions::PRINT);
        assert!(!p.can_print_high_quality());
    }

    #[test]
    fn can_modify_annotations_false_when_not_set() {
        let p = Permissions(Permissions::PRINT);
        assert!(!p.can_modify_annotations());
    }

    #[test]
    fn can_modify_content_only() {
        let p = Permissions(Permissions::MODIFY_CONTENT);
        assert!(p.can_modify_content());
        assert!(!p.can_copy());
        assert!(!p.can_assemble());
    }
}
