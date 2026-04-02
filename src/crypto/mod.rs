//! Encryption handlers and permission checks.
//!
//! Only compiled when the `crypto` crate feature is enabled (default).

pub mod permissions;
pub mod rc4;
pub mod handlers;

#[cfg(feature = "crypto")]
pub mod aes;
#[cfg(feature = "crypto")]
pub mod md5;

pub use permissions::Permissions;
pub use rc4::Rc4;
pub use handlers::{AuthResult, EncryptionDict, StandardSecurityHandler};

#[cfg(feature = "crypto")]
pub use aes::aes_cbc_decrypt;
