//! Encryption handlers and permission checks.
//!
//! Implements the PDF Standard Security Handler (§7.6.3) with:
//! - RC4-40 (Revision 2)
//! - RC4-128 (Revision 3)
//! - RC4/AES-128 (Revision 4, AES decrypt stub)
//!
//! Maps to Java PDFBox `StandardSecurityHandler`, `AccessPermission`,
//! `ARCFourEncryption`, `PDEncryption`.

pub mod md5;
pub mod permissions;
pub mod rc4;
pub mod handlers;

pub use permissions::Permissions;
pub use rc4::Rc4;
pub use handlers::{AuthResult, EncryptionDict, StandardSecurityHandler};
