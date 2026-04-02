//! Benchmarks for rust-pdfbox core paths.
//!
//! Run with:  cargo bench
//!
//! Tracks:
//! - First-page parse latency (header + xref + first page object only)
//! - Full-document parse throughput (bytes/sec, large PDFs)
//! - Content stream tokenization throughput
//! - Text extraction throughput (bytes of content stream / sec)
//! - Full-rewrite save speed (small and large PDFs)
//! - Incremental save speed
//! - Decoded stream cache sizing (StreamCache peak memory)

use rust_pdfbox::cos::{CosObject, ObjectId};
use rust_pdfbox::content::parse_content_stream;
use rust_pdfbox::text::extract_text;
use rust_pdfbox::Document;
use std::hint::black_box;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Fixture generators
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

/// Build a PDF with `n` pages — used for large-document throughput benchmarks.
fn n_page_pdf(n: usize) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut page_offsets: Vec<u64> = Vec::with_capacity(n);
    // Write page objects (obj 3..3+n)
    for i in 0..n {
        page_offsets.push(pdf.len() as u64);
        let obj_num = i + 3;
        let body = format!(
            "{obj_num} 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n"
        );
        pdf.extend_from_slice(body.as_bytes());
    }
    // Pages dict (obj 2)
    let pages_off = pdf.len() as u64;
    let kids: String = (3..3 + n).map(|i| format!("{i} 0 R")).collect::<Vec<_>>().join(" ");
    pdf.extend_from_slice(
        format!("2 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {n} >>\nendobj\n").as_bytes(),
    );
    // Catalog (obj 1)
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // XRef
    let xref_off = pdf.len();
    let total_objs = n + 3; // 0(free) + 1(catalog) + 2(pages) + n(page objs)
    pdf.extend_from_slice(format!("xref\n0 {total_objs}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    for off in &page_offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {total_objs} /Root 1 0 R >>\n").as_bytes());
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

/// A multi-line content stream with lots of text operators for throughput tests.
fn heavy_content_stream(line_count: usize) -> Vec<u8> {
    let mut cs = b"BT /F1 12 Tf 72 720 Td\n".to_vec();
    for i in 0..line_count {
        cs.extend_from_slice(
            format!("(Line {i:04}: The quick brown fox jumps over the lazy dog.) Tj\n0 -14 Td\n")
                .as_bytes(),
        );
    }
    cs.extend_from_slice(b"ET\n");
    cs
}

/// Try to load the real PDFBox-generated fixture from disk; fall back to
/// the in-memory generator if the file is absent.
fn fixture_bytes(path: &str) -> Vec<u8> {
    let full = format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&full).unwrap_or_else(|_| {
        // Derive page count from path suffix (e.g. "large/200_pages.pdf" → 200)
        if let Some(n) = path
            .split('/')
            .last()
            .and_then(|f| f.strip_suffix("_pages.pdf"))
            .and_then(|s| s.parse::<usize>().ok())
        {
            n_page_pdf(n)
        } else {
            two_page_pdf()
        }
    })
}

// ---------------------------------------------------------------------------
// Simple benchmark harness (no external crate needed)
// ---------------------------------------------------------------------------

struct BenchResult {
    name: String,
    iters: u32,
    per_iter_ns: f64,
    total_ms: f64,
    throughput: Option<String>,
}

impl BenchResult {
    fn print(&self) {
        let per_us = self.per_iter_ns / 1_000.0;
        let tp = self
            .throughput
            .as_deref()
            .unwrap_or("");
        println!(
            "  {:<50} {:>6} iters  {:>8.2} µs/iter  {:>8.2} ms total  {}",
            self.name, self.iters, per_us, self.total_ms, tp
        );
    }
}

fn bench<F: Fn()>(name: &str, iterations: u32, f: F) -> BenchResult {
    // Warmup
    for _ in 0..10 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();
    let per_iter_ns = elapsed.as_nanos() as f64 / iterations as f64;
    BenchResult {
        name: name.to_string(),
        iters: iterations,
        per_iter_ns,
        total_ms: elapsed.as_secs_f64() * 1_000.0,
        throughput: None,
    }
}

fn bench_throughput<F: Fn()>(
    name: &str,
    iterations: u32,
    bytes_per_iter: usize,
    f: F,
) -> BenchResult {
    let mut r = bench(name, iterations, f);
    let bytes_per_sec =
        (bytes_per_iter as f64 * iterations as f64) / (r.total_ms / 1_000.0);
    r.throughput = Some(format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0));
    r
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    println!("\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║           rust-pdfbox  Performance Benchmarks                   ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // ── 1. First-page parse latency ──────────────────────────────────────────
    println!("── 1. First-page parse latency ─────────────────────────────────────");
    println!("   Measures: header check + xref discovery + parse first page object");
    {
        let pdf = two_page_pdf();
        let r = bench_throughput(
            "load_from_bytes → first page (2-page PDF)",
            10_000,
            pdf.len(),
            || {
                let doc = black_box(Document::load_from_bytes(black_box(&pdf)).unwrap());
                let tree = doc.pages().unwrap();
                black_box(tree.get(0).unwrap());
            },
        );
        r.print();

        // Disk fixture — single Letter page (PDFBox-generated, PDF 1.6 with XRef stream)
        let disk_pdf = fixture_bytes("smoke/letter_single_page.pdf");
        let r2 = bench_throughput(
            "load_from_bytes → first page (real PDFBox PDF 1.6)",
            5_000,
            disk_pdf.len(),
            || {
                let doc = black_box(Document::load_from_bytes(black_box(&disk_pdf)).unwrap());
                let tree = doc.pages().unwrap();
                black_box(tree.get(0).unwrap());
            },
        );
        r2.print();
    }

    // ── 2. Full-document parse throughput ────────────────────────────────────
    println!("\n── 2. Full-document parse throughput ───────────────────────────────");
    println!("   Measures: bytes/sec for full load_from_bytes on various sizes");
    {
        for &page_count in &[10usize, 50, 100, 200] {
            let pdf = fixture_bytes(&format!("large/{page_count}_pages.pdf"))
                .to_owned()
                .pipe_or(|| n_page_pdf(page_count));
            // Use in-memory generator for "fifty" naming mismatch
            let pdf = if pdf.is_empty() { n_page_pdf(page_count) } else { pdf };
            let iters = match page_count {
                200 => 500,
                100 => 1_000,
                _ => 2_000,
            };
            let r = bench_throughput(
                &format!("load_from_bytes ({page_count:>3}-page PDF, {} KB)", pdf.len() / 1024),
                iters,
                pdf.len(),
                || {
                    black_box(Document::load_from_bytes(black_box(&pdf)).unwrap());
                },
            );
            r.print();
        }
    }

    // ── 3. Text extraction throughput ────────────────────────────────────────
    println!("\n── 3. Text extraction throughput ───────────────────────────────────");
    println!("   Measures: content stream bytes/sec through extract_text pipeline");
    {
        for &lines in &[10usize, 100, 500] {
            let cs = heavy_content_stream(lines);
            let r = bench_throughput(
                &format!("extract_text ({lines:>3} lines, {} B stream)", cs.len()),
                10_000,
                cs.len(),
                || {
                    black_box(extract_text(black_box(&cs), None));
                },
            );
            r.print();
        }

        // Tokenization only (no Unicode decode)
        let cs100 = heavy_content_stream(100);
        let r = bench_throughput(
            "parse_content_stream only (100 lines)",
            10_000,
            cs100.len(),
            || {
                let _ = black_box(parse_content_stream(black_box(&cs100)));
            },
        );
        r.print();
    }

    // ── 4. Full save and incremental save speed ───────────────────────────────
    println!("\n── 4. Save speed ────────────────────────────────────────────────────");
    println!("   Measures: full-rewrite and incremental save wall time");
    {
        let small_pdf = two_page_pdf();
        let large_pdf = n_page_pdf(100);

        let small_doc = Document::load_from_bytes(&small_pdf).unwrap();
        let large_doc = Document::load_from_bytes(&large_pdf).unwrap();

        // Full rewrite — small
        let r = bench(
            "save_to full-rewrite (2-page PDF)",
            10_000,
            || {
                let mut buf = std::io::Cursor::new(Vec::with_capacity(2048));
                black_box(small_doc.save_to(black_box(&mut buf)).unwrap());
            },
        );
        r.print();

        // Full rewrite — large
        let r = bench(
            "save_to full-rewrite (100-page PDF)",
            1_000,
            || {
                let mut buf = std::io::Cursor::new(Vec::with_capacity(large_pdf.len() + 4096));
                black_box(large_doc.save_to(black_box(&mut buf)).unwrap());
            },
        );
        r.print();

        // Incremental save — small
        let mut changed_small = std::collections::BTreeMap::new();
        changed_small.insert(ObjectId::new(5, 0), CosObject::Integer(42));
        let r = bench(
            "save_incremental (2-page + 1 changed obj)",
            10_000,
            || {
                let mut out = Vec::with_capacity(small_pdf.len() + 512);
                black_box(
                    small_doc
                        .save_incremental(black_box(&small_pdf), black_box(&changed_small), &mut out)
                        .unwrap(),
                );
            },
        );
        r.print();

        // Incremental save — large
        let mut changed_large = std::collections::BTreeMap::new();
        for i in 0u32..10 {
            changed_large.insert(ObjectId::new(200 + i, 0), CosObject::Integer(i as i64));
        }
        let r = bench(
            "save_incremental (100-page + 10 changed objs)",
            1_000,
            || {
                let mut out = Vec::with_capacity(large_pdf.len() + 4096);
                black_box(
                    large_doc
                        .save_incremental(black_box(&large_pdf), black_box(&changed_large), &mut out)
                        .unwrap(),
                );
            },
        );
        r.print();
    }

    // ── 5. Stream cache sizing — peak memory ─────────────────────────────────
    println!("\n── 5. Decoded stream cache sizing ───────────────────────────────────");
    println!("   Measures: StreamCache decode latency, Arc clone cost, cache hits");
    {
        // Build a PDF with a plain (no filter) stream object for cache benchmarking
        let content = b"BT /F1 12 Tf 72 720 Td (Cache benchmark content) Tj ET";
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let stream_off = pdf.len() as u64;
        let stream_hdr =
            format!("5 0 obj\n<< /Length {} >>\nstream\n", content.len());
        pdf.extend_from_slice(stream_hdr.as_bytes());
        pdf.extend_from_slice(content);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        let page_off = pdf.len() as u64;
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Contents 5 0 R >>\nendobj\n",
        );
        let pages_off = pdf.len() as u64;
        pdf.extend_from_slice(
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
        );
        let cat_off = pdf.len() as u64;
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let xref_off = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \r\n");
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", page_off).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \r\n");
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", stream_off).as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\n");
        pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());

        let stream_id = ObjectId::new(5, 0);

        // Cache miss — first decode
        let r = bench(
            "StreamCache::get_decoded (cold miss, plain stream)",
            20_000,
            || {
                let doc = Document::load_from_bytes(&pdf).unwrap();
                black_box(doc.get_decoded_stream(black_box(&stream_id)));
            },
        );
        r.print();

        // Cache hit — Arc clone only
        let doc = Document::load_from_bytes(&pdf).unwrap();
        doc.get_decoded_stream(&stream_id); // prime cache
        let r = bench(
            "StreamCache::get_decoded (warm hit, Arc clone)",
            100_000,
            || {
                black_box(doc.get_decoded_stream(black_box(&stream_id)));
            },
        );
        r.print();

        // source_bytes Arc clone cost
        let r = bench(
            "Document::source_bytes() Arc clone",
            200_000,
            || {
                black_box(doc.source_bytes());
            },
        );
        r.print();

        // Report cache occupancy
        let count = doc.cached_stream_count();
        let arc_bytes = content.len();
        println!(
            "  Cache occupancy: {count} stream(s), ~{arc_bytes} bytes decoded content"
        );
    }

    // ── Page tree traversal ───────────────────────────────────────────────────
    println!("\n── 6. Page tree traversal ───────────────────────────────────────────");
    {
        for &n in &[2usize, 10, 100] {
            let pdf = n_page_pdf(n);
            let doc = Document::load_from_bytes(&pdf).unwrap();
            let r = bench(
                &format!("pages().iter().count() ({n:>3} pages)"),
                if n > 50 { 5_000 } else { 50_000 },
                || {
                    let tree = black_box(doc.pages().unwrap());
                    black_box(tree.iter().count());
                },
            );
            r.print();
        }
    }

    println!("\n✓ All benchmarks complete.\n");
}

// ---------------------------------------------------------------------------
// Tiny helper trait — avoids None from fixture_bytes for non-standard names
// ---------------------------------------------------------------------------
trait PipeOr {
    fn pipe_or<F: FnOnce() -> Vec<u8>>(self, f: F) -> Vec<u8>;
}
impl PipeOr for Vec<u8> {
    fn pipe_or<F: FnOnce() -> Vec<u8>>(self, f: F) -> Vec<u8> {
        if self.is_empty() { f() } else { self }
    }
}
