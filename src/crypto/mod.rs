//! Encryption handlers and permission checks.
//!
//! Only compiled when the `crypto` crate feature is enabled (default).

pub mod handlers;
pub mod permissions;
pub mod rc4;

#[cfg(feature = "crypto")]
pub mod aes;
#[cfg(feature = "crypto")]
pub mod aes_encrypt;
#[cfg(feature = "crypto")]
pub mod md5;
#[cfg(feature = "crypto")]
pub mod rev56;

pub use handlers::{AuthResult, EncryptionDict, StandardSecurityHandler};
pub use permissions::Permissions;
pub use rc4::Rc4;

#[cfg(feature = "crypto")]
pub use aes::aes_cbc_decrypt;
