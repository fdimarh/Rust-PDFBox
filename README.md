# rust-pdfbox

A Rust port of Apache PDFBox components, focused on PDF parsing, incremental writing, encryption (AES-256 Rev 6), and digital signatures with long-term validation (LTV / DSS).

## Features

- **PDF Parsing & Traversal:** Low-level support for parsing PDF objects, streams, cross-reference tables (classic and streams), and trailers.
- **Incremental Writing:** Support for creating incremental updates (`IncrementalWriter`) to existing PDF files.
- **Encryption & Protection:**
    - Decrypts password-protected PDFs (RC4, AES-128, AES-256).
    - Encrypts unencrypted PDFs using `Document::protect()` with standard AES-256 (Revision 6) protection policies.
- **Digital Signatures & LTV Support:**
    - Supports PKCS#7 (`adbe.pkcs7.detached`) and PAdES (`ETSI.CAdES.detached`) formats (B-B, B-T, B-LT, B-LTA).
    - Supports DocMDP certification signatures with permission locking (`/Perms`).
    - Custom external signer callback integration (e.g. BSRE remote signing).
    - **LTV / DSS Dictionary:** Incremental updating of `/DSS` (`/CRLs`, `/OCSPs`, `/Certs`, `/VRI`) on both unencrypted and encrypted password-protected PDFs.

## Usage

### 1. Encrypting an Unencrypted PDF

To encrypt a PDF, use `Document::protect()` with a `StandardProtectionPolicy`:

```rust,ignore
use rust_pdfbox::Document;
use rust_pdfbox::protection::StandardProtectionPolicy;
use rust_pdfbox::crypto::Permissions;

// Load an unencrypted PDF
let mut doc = Document::load("unencrypted.pdf")?;

// Create a protection policy (AES-256 Rev 6)
let policy = StandardProtectionPolicy::new(
    "owner-password",
    "user-password",
    Permissions::all_allowed(),
);

// Apply protection policy
doc.protect(&policy)?;

// Save the encrypted document
let mut file = std::fs::File::create("encrypted.pdf")?;
doc.save_to(&mut file)?;
```

### 2. Digital Signing with Custom Signer Callback

To sign a PDF (including password-protected PDFs):

```rust,ignore
use rust_pdfbox::signing::{self, SignOptions, SignatureFormat, PadesLevel};

let pdf_bytes = std::fs::read("document.pdf")?;
let opts = SignOptions {
    format: SignatureFormat::PAdES,
    pades_level: PadesLevel::B_LT,
    signer_name: "John Doe".to_string(),
    reason: "Approval".to_string(),
    reserved_size: 25_000,
    ..Default::default()
};

// Sign using a custom callback (e.g., remote API, HSM, or local key)
let signed_bytes = signing::sign_pdf_with_signer(
    &pdf_bytes,
    Some("optional-pdf-password"),
    &opts,
    |signed_content| {
        // Compute SHA-256 hash over signed_content, request CMS signature, return DER bytes
        let digest = sha2::Sha256::digest(signed_content);
        let cms_der_bytes = fetch_cms_signature(&digest)?;
        Ok(cms_der_bytes)
    },
)?;

std::fs::write("signed_output.pdf", signed_bytes)?;
```

### 3. Validating PDF Digital Signatures

Validate signatures and inspect LTV/DSS status:

```rust,ignore
use rust_pdfbox::signing::validator::SignatureValidator;

let pdf_bytes = std::fs::read("signed_output.pdf")?;
let results = SignatureValidator::validate(&pdf_bytes, Some("optional-pdf-password"))?;

for r in &results {
    println!("Signer: {:?}", r.signer_name);
    println!("Signature valid: {}", r.is_valid());
    println!("Digest match: {}", r.digest_match);
    println!("DSS present: {}", r.has_dss);
    println!("LTV enabled: {}", r.is_ltv_enabled);
}
```

## License

MIT / Apache 2.0
