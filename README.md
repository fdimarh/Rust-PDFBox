# rust-pdfbox

This is a partial Rust port of Apache PDFBox, focused on providing specific PDF manipulation capabilities. It is not a full-featured PDF library but is designed to support specific use cases, such as digital signing and encryption.

## Features

- **PDF Parsing:** Low-level support for parsing PDF documents, including objects, streams, and cross-reference tables.
- **Incremental Writing:** Support for creating incremental updates to existing PDFs.
- **Encryption and Decryption:**
    - Decrypts password-protected PDFs (RC4, AES-128, AES-256).
    - **New:** Encrypts unencrypted PDFs using `Document::protect()` with AES-256 (Revision 6) encryption.
- **Digital Signatures:** Provides hooks for creating digital signature placeholders and embedding CMS signatures.

## Usage

### Encrypting a PDF

To encrypt a PDF, you can use the `Document::protect()` method. This will set up the necessary encryption dictionary and key, which will be used when the document is saved.

```rust,ignore
use rust_pdfbox::Document;
use rust_pdfbox::protection::StandardProtectionPolicy;
use rust_pdfbox::crypto::Permissions;

// Load an unencrypted PDF
let mut doc = Document::load("unencrypted.pdf")?;

// Create a protection policy
let policy = StandardProtectionPolicy::new(
    "owner-password",
    "user-password",
    Permissions::all_allowed(),
);

// Apply the protection policy
doc.protect(&policy)?;

// Save the encrypted document
let mut file = std::fs::File::create("encrypted.pdf")?;
doc.save_encrypted(&mut file)?;
```
