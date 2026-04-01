//! Benchmarks for rust-pdfbox core paths.
//!
//! Run with:  cargo bench
//!
//! Tracks:
//! - Document load latency (parse + xref + object store)
//! - Page tree traversal
//! - Content stream tokenization
//! - Text extraction
//! - Full-rewrite save
//! - Incremental save

use rust_pdfbox::Document;
use rust_pdfbox::content::parse_content_stream;
use rust_pdfbox::text::extract_text;
use std::hint::black_box;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Minimal test fixture — same two-page PDF used in unit tests
// ---------------------------------------------------------------------------

fn two_page_pdf() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let p1_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n");
    let p2_off = pdf.len() as u64;
    pdf.extend_from_slice(b"4 0 obj\n<< /Type /Page /MediaBox [0 0 595 842] >>\nendobj\n");
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    let e1 = format!("{:010} 00000 n \r\n", cat_off);
    let e2 = format!("{:010} 00000 n \r\n", pages_off);
    let e3 = format!("{:010} 00000 n \r\n", p1_off);
    let e4 = format!("{:010} 00000 n \r\n", p2_off);
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(e1.as_bytes()); pdf.extend_from_slice(e2.as_bytes());
    pdf.extend_from_slice(e3.as_bytes()); pdf.extend_from_slice(e4.as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn content_stream() -> Vec<u8> {
    b"BT /F1 12 Tf 72 720 Td (Hello World) Tj 0 -14 Td (Second line) Tj ET".to_vec()
}

// ---------------------------------------------------------------------------
// Simple benchmark harness (no external crate needed)
// ---------------------------------------------------------------------------

fn bench<F: Fn()>(name: &str, iterations: u32, f: F) {
    // Warmup
    for _ in 0..10 { f(); }

    let start = Instant::now();
    for _ in 0..iterations { f(); }
    let elapsed = start.elapsed();

    let per_iter = elapsed / iterations;
    let throughput_us = per_iter.as_nanos() as f64 / 1_000.0;
    println!("{name:45} {iterations:6} iters  {throughput_us:8.2} µs/iter  total={:.2}ms",
        elapsed.as_secs_f64() * 1000.0);
}

fn main() {
    let pdf_bytes = two_page_pdf();
    let cs_bytes  = content_stream();

    println!("\n=== rust-pdfbox benchmarks ===\n");

    // --- Load ---
    bench("Document::load_from_bytes (2-page PDF)", 10_000, || {
        black_box(Document::load_from_bytes(black_box(&pdf_bytes)).unwrap());
    });

    // --- Page tree ---
    let doc = Document::load_from_bytes(&pdf_bytes).unwrap();
    bench("Document::pages() + iter (2 pages)", 50_000, || {
        let tree = black_box(doc.pages().unwrap());
        black_box(tree.iter().count());
    });

    // --- Content stream tokenization ---
    bench("parse_content_stream (Tj sequence)", 50_000, || {
        let _ = black_box(parse_content_stream(black_box(&cs_bytes)));
    });

    // --- Text extraction ---
    bench("extract_text (Tj sequence, no CMap)", 50_000, || {
        black_box(extract_text(black_box(&cs_bytes), None));
    });

    // --- Full-rewrite save ---
    bench("Document::save_to (2-page PDF)", 10_000, || {
        let mut buf = std::io::Cursor::new(Vec::with_capacity(1024));
        black_box(doc.save_to(black_box(&mut buf)).unwrap());
    });

    // --- Incremental save ---
    use rust_pdfbox::cos::{ObjectId, CosObject};
    let mut changed = std::collections::BTreeMap::new();
    changed.insert(ObjectId::new(5, 0), CosObject::Integer(1));
    bench("Document::save_incremental (1 changed object)", 10_000, || {
        let mut out = Vec::with_capacity(pdf_bytes.len() + 256);
        black_box(doc.save_incremental(black_box(&pdf_bytes), black_box(&changed), &mut out).unwrap());
    });

    println!("\nDone.");
}

