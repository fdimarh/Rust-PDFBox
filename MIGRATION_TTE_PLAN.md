# MIGRATION PLAN: ENCRYPTED TTE (PASSWORD-PROTECTED SIGNATURES)
**Status:** Initiated
**Source:** `rust_pdf_signing` (Deprecated for Encrypted TTE)
**Destination:** `rust-pdfbox` (Main Engine)

## 1. Latar Belakang (Background)
Implementasi Tanda Tangan Elektronik (TTE) PAdES pada dokumen PDF yang terenkripsi (Password-Protected) tidak dapat diakomodasi oleh pustaka `lopdf` (yang menjadi dasar `rust_pdf_signing`). Hal ini dikarenakan:
1. `lopdf` tidak memiliki kapabilitas *Save Pipeline* dengan enkripsi *on-the-fly* (AES/RC4).
2. Memaksa enkripsi di tingkat memori (Memory Bypass) menghancurkan akurasi `ByteRange` hash dan merusak tabel referensi silang (`xref`), yang berakibat penolakan/invalidasi signature oleh Adobe Acrobat dan Foxit.
3. Spesifikasi ISO 32000-1 mewajibkan bahwa penambahan TTE pada PDF terenkripsi **HARUS** dilakukan melalui **Incremental Update**. 

Oleh karena itu, seluruh infrastruktur TTE dipindahkan ke `rust-pdfbox` yang mewarisi arsitektur *CosDocument*, *SecurityHandler*, dan *Incremental Writer* kelas *Enterprise* dari Java Apache PDFBox.

## 2. Arsitektur Tujuan (Target Architecture)
Proses TTE Berpassword di `rust-pdfbox` akan mengikuti alur standar:
```rust
// 1. Load the encrypted document using rust-pdfbox parser
let mut doc = Document::load("protected_file.pdf")?;
doc.decrypt("admin123")?; // Uses rust-pdfbox SecurityHandler

// 2. Prepare the CMS Signer (extracted from rust_pdf_signing)
let signer = CmsSigner::new(cert_chain, private_key);

// 3. Add signature placeholder and compute ByteRange
let options = SignOptions::pades_lts();
doc.add_signature(&signer, options)?;

// 4. Save via Incremental Writer (Preserves original encryption!)
doc.save_incremental("final_signed_protected.pdf")?;
```

## 3. Fase Eksekusi Migrasi (Migration Phases)

### Phase 1: Ekstraksi Modul Kriptografi & PKI
- [x] Ekstrak logika pembangkitan sertifikat (`x509_certificate`) dan CMS/PKCS#7 (`cryptographic_message_syntax`) dari repositori `rust_pdf_signing`.
- [x] Pindahkan logika pembuatan atribut PAdES (CRL/OCSP revocation info, RFC 3161 Timestamp) ke modul `rust-pdfbox/src/signing/`.
- [x] Pastikan tidak ada dependensi `lopdf` yang terbawa. Ubah semuanya agar beroperasi menggunakan struktur `CosObject` milik PDFBox.

### Phase 2: Kematangan SecurityHandler & Parser
- [x] Validasi fungsi `Document::decrypt` di `rust-pdfbox` mampu melakukan dekripsi penuh terhadap AES-256 (Revision 6) — **SELESAI (Algorithm 2.B K1 pipeline 64x & /UE recovery fixed).**
- [ ] Lanjutkan validasi AES-128 dan RC4.
- [x] Pastikan *parser* dapat membaca dan memodifikasi *trailer* dari dokumen yang terenkripsi tanpa kehilangan *Encryption Dictionary* (`/Encrypt`).

### Phase 3: Pengembangan Incremental Writer
- [x] Sempurnakan modul `rust-pdfbox/src/writer/incremental.rs`.
- [x] *Writer* wajib mampu mengkalkulasi celah byte (ByteRange Gap) secara akurat saat merender revisi dokumen (Multi-Signature incremental patch solved).
- [x] *Writer* tidak boleh mengenkripsi *field* `/Contents` pada kamus *Signature*, tetapi harus mengenkripsi revisi objek lain sesuai dengan state `SecurityHandler` aktif dokumen tersebut. (Raw gap bypass solved).

### Phase 4: Integrasi API dan CLI
- [ ] Buat API publik `sign_pdf` di `rust-pdfbox` yang menerima `SignOptions` dan otorisasi *password*.
- [ ] Porting CLI `verify_pdf` dan `sign_doc` menjadi utilitas bawaan `rust-pdfbox`.

---
*Dokumen ini adalah acuan resmi. Dilarang kembali menggunakan pendekatan hack/memory bypass dengan lopdf.*