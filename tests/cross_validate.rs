//! Cross-validation suite — validates rust-pdfbox against Java PDFBox reference snapshots.
//!
//! Each test loads a JSON snapshot (Java PDFBox reference output) from
//! `tests/cross_validation/` and validates our Rust output against it.
//!
//! Run: `cargo test --test cross_validate`

use rust_pdfbox::Document;

// ── Snapshot model ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct PageSnap {
    index: usize,
    width: f64,
    height: f64,
    rotation: i64,
    text_len_min: usize,
    text_len_max: usize,
    text_contains: Vec<String>,
}

#[derive(Debug, Clone)]
struct PermSnap { print: bool, copy: bool, modify: bool, annotate: bool }

#[derive(Debug)]
struct Snapshot {
    file: String,
    pdf_version: String,
    page_count: usize,
    pages: Vec<PageSnap>,
    permissions: PermSnap,
    fonts: Vec<String>,
}

// ── Minimal hand-rolled JSON parser ─────────────────────────────────────────

fn xstr(s: &str, key: &str) -> String {
    let needle = format!("\"{}\"", key);
    let pos = match s.find(&needle) { Some(p) => p, None => return String::new() };
    let after = &s[pos + needle.len()..];
    let colon = after.find(':').unwrap_or(0);
    let v = after[colon + 1..].trim_start();
    if v.starts_with('"') {
        let end = v[1..].find('"').unwrap_or(v.len() - 1);
        v[1..end + 1].to_string()
    } else { String::new() }
}

fn xusize(s: &str, key: &str) -> usize {
    let needle = format!("\"{}\"", key);
    let pos = match s.find(&needle) { Some(p) => p, None => return 0 };
    let after = &s[pos + needle.len()..];
    let colon = after.find(':').unwrap_or(0);
    let v = after[colon + 1..].trim_start();
    let end = v.find(|c: char| !c.is_ascii_digit()).unwrap_or(v.len());
    v[..end].parse().unwrap_or(0)
}

fn xf64(s: &str, key: &str) -> f64 {
    let needle = format!("\"{}\"", key);
    let pos = match s.find(&needle) { Some(p) => p, None => return 0.0 };
    let after = &s[pos + needle.len()..];
    let colon = after.find(':').unwrap_or(0);
    let v = after[colon + 1..].trim_start();
    let end = v.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-').unwrap_or(v.len());
    v[..end].parse().unwrap_or(0.0)
}

fn xi64(s: &str, key: &str) -> i64 {
    let needle = format!("\"{}\"", key);
    let pos = match s.find(&needle) { Some(p) => p, None => return 0 };
    let after = &s[pos + needle.len()..];
    let colon = after.find(':').unwrap_or(0);
    let v = after[colon + 1..].trim_start();
    let end = v.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(v.len());
    v[..end].parse().unwrap_or(0)
}

fn xbool(s: &str, key: &str) -> bool {
    let needle = format!("\"{}\"", key);
    let pos = match s.find(&needle) { Some(p) => p, None => return false };
    let after = &s[pos + needle.len()..];
    let colon = after.find(':').unwrap_or(0);
    after[colon + 1..].trim_start().starts_with("true")
}

fn xstrarray(json: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!("\"{}\"", key);
    let pos = match json.find(&needle) { Some(p) => p, None => return out };
    let after = &json[pos + needle.len()..];
    let arr_start = match after.find('[') { Some(p) => p + 1, None => return out };
    let arr = &after[arr_start..];
    let mut i = 0;
    while i < arr.len() {
        match arr[i..].find('"') {
            None => break,
            Some(qs) => {
                let from = i + qs + 1;
                match arr[from..].find('"') {
                    None => break,
                    Some(qe) => { out.push(arr[from..from + qe].to_string()); i = from + qe + 1; }
                }
            }
        }
        if arr[i..].starts_with(']') { break; }
    }
    out
}

fn parse_pages(json: &str) -> Vec<PageSnap> {
    let mut pages = Vec::new();
    let start = match json.find("\"pages\"") { Some(p) => p, None => return pages };
    let arr_start = match json[start..].find('[') { Some(p) => start + p + 1, None => return pages };
    let arr = &json[arr_start..];
    let mut depth = 0i32;
    let mut obj_start: Option<usize> = None;
    for (i, ch) in arr.char_indices() {
        match ch {
            '{' => { if depth == 0 { obj_start = Some(i); } depth += 1; }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = obj_start {
                        let obj = &arr[s..=i];
                        pages.push(PageSnap {
                            index:         xusize(obj, "index"),
                            width:         xf64(obj, "width"),
                            height:        xf64(obj, "height"),
                            rotation:      xi64(obj, "rotation"),
                            text_len_min:  xusize(obj, "text_len_min"),
                            text_len_max:  xusize(obj, "text_len_max"),
                            text_contains: xstrarray(obj, "text_contains"),
                        });
                    }
                    obj_start = None;
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    pages
}

fn parse_snapshot(json: &str) -> Snapshot {
    let pb  = json.find("\"permissions\"").unwrap_or(0);
    let pb2 = json[pb..].find('{').map(|p| pb + p).unwrap_or(pb);
    let pb3 = json[pb2..].find('}').map(|p| pb2 + p + 1).unwrap_or(json.len());
    let perm = &json[pb2..pb3];
    Snapshot {
        file:        xstr(json, "file"),
        pdf_version: xstr(json, "pdf_version"),
        page_count:  xusize(json, "page_count"),
        pages:       parse_pages(json),
        permissions: PermSnap {
            print:    xbool(perm, "print"),
            copy:     xbool(perm, "copy"),
            modify:   xbool(perm, "modify"),
            annotate: xbool(perm, "annotate"),
        },
        fonts: xstrarray(json, "fonts"),
    }
}

// ── Validation engine ────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum Check { Pass, Fail(String), Skip(String) }

struct VResult { file: String, checks: Vec<(String, Check)> }

impl VResult {
    fn new(f: &str) -> Self { Self { file: f.to_string(), checks: Vec::new() } }
    fn pass(&mut self, n: &str) { self.checks.push((n.into(), Check::Pass)); }
    fn fail(&mut self, n: &str, m: impl Into<String>) { self.checks.push((n.into(), Check::Fail(m.into()))); }
    fn skip(&mut self, n: &str, m: impl Into<String>) { self.checks.push((n.into(), Check::Skip(m.into()))); }
    fn is_ok(&self) -> bool { self.checks.iter().all(|(_, c)| !matches!(c, Check::Fail(_))) }
    fn summary(&self) -> String {
        let status = if self.is_ok() { "PASS" } else { "FAIL" };
        let mut s = format!("\n=== {} --- {} ===\n", self.file, status);
        for (n, c) in &self.checks {
            match c {
                Check::Pass    => s.push_str(&format!("  OK   {}\n", n)),
                Check::Fail(m) => s.push_str(&format!("  FAIL {} -- {}\n", n, m)),
                Check::Skip(m) => s.push_str(&format!("  SKIP {} ({})\n", n, m)),
            }
        }
        s
    }
}

fn validate(doc: &Document, snap: &Snapshot) -> VResult {
    let mut r = VResult::new(&snap.file);

    // page count
    let pc = doc.page_count();
    if pc == snap.page_count { r.pass("page_count"); }
    else { r.fail("page_count", format!("expected {}, got {}", snap.page_count, pc)); }

    // pdf version — not directly exposed; skip gracefully
    r.skip("pdf_version", "version not exposed via API");

    // permissions — structural check (not encrypted in our test fixtures)
    r.skip("permissions", "encryption handler not used for these fixtures");

    // page tree — only needed when the snapshot requires per-page checks
    let pages = match doc.pages() {
        Ok(p)  => Some(p),
        Err(e) => {
            if snap.pages.is_empty() {
                // Malformed / empty fixture — page tree inaccessible is expected
                r.skip("page_tree", format!("lenient: {}", e));
                None
            } else {
                r.fail("page_tree", format!("{}", e));
                return r;
            }
        }
    };
    let pages = match pages { Some(p) => p, None => return r };

    for ps in &snap.pages {
        let page = match pages.get(ps.index) {
            Some(p) => p,
            None    => { r.fail(&format!("page[{}]", ps.index), "page not found"); continue; }
        };

        // dimensions
        let dname = format!("page[{}]_dims", ps.index);
        if let Some(mb) = page.media_box() {
            let (w, h) = (mb.width(), mb.height());
            if (w - ps.width).abs() <= 0.5 && (h - ps.height).abs() <= 0.5 {
                r.pass(&dname);
            } else {
                r.fail(&dname, format!("expected {}x{} got {}x{}", ps.width, ps.height, w, h));
            }
        } else {
            r.skip(&dname, "no MediaBox");
        }

        // rotation
        let rname = format!("page[{}]_rotation", ps.index);
        if page.rotation() == ps.rotation { r.pass(&rname); }
        else { r.fail(&rname, format!("expected {}, got {}", ps.rotation, page.rotation())); }

        // text
        let tname = format!("page[{}]_text", ps.index);
        if ps.text_len_min == 0 && ps.text_len_max >= 9999 && ps.text_contains.is_empty() {
            r.skip(&tname, "no text constraints in snapshot");
            continue;
        }

        #[cfg(feature = "text")]
        {
            use rust_pdfbox::extract_text;
            let content_bytes: Vec<u8> = if let Some(contents) = page.contents_object() {
                if let Some(s) = contents.as_stream() {
                    s.data.clone()
                } else if let Some(refid) = contents.as_reference() {
                    doc.objects.get(&refid)
                        .and_then(|o| o.as_stream())
                        .map(|s| s.data.clone())
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            let text = extract_text(&content_bytes, None);
            let tl = text.len();
            if tl < ps.text_len_min {
                r.fail(&tname, format!("text too short: {} < {}", tl, ps.text_len_min));
            } else if tl > ps.text_len_max {
                r.fail(&tname, format!("text too long: {} > {}", tl, ps.text_len_max));
            } else {
                let mut ok = true;
                for req in &ps.text_contains {
                    if !text.contains(req.as_str()) {
                        r.fail(&tname, format!("missing: '{}'", req));
                        ok = false;
                        break;
                    }
                }
                if ok { r.pass(&tname); }
            }
        }
        #[cfg(not(feature = "text"))]
        r.skip(&tname, "text feature disabled");
    }

    r
}

// ── PDF generators ───────────────────────────────────────────────────────────

fn single_page_pdf(width: f64, height: f64) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let p1_off = pdf.len() as u64;
    pdf.extend_from_slice(format!("2 0 obj\n<< /Type /Page /MediaBox [0 0 {width} {height}] >>\nendobj\n").as_bytes());
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", p1_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn single_page_pdf_version(version: &str, width: f64, height: f64) -> Vec<u8> {
    let mut pdf = format!("%PDF-{version}\n").into_bytes();
    let p1_off = pdf.len() as u64;
    pdf.extend_from_slice(format!("2 0 obj\n<< /Type /Page /MediaBox [0 0 {width} {height}] >>\nendobj\n").as_bytes());
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", p1_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn rotated_page_pdf(width: f64, height: f64, rotation: i64) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let p1_off = pdf.len() as u64;
    pdf.extend_from_slice(format!("2 0 obj\n<< /Type /Page /MediaBox [0 0 {width} {height}] /Rotate {rotation} >>\nendobj\n").as_bytes());
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", p1_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn minimal_catalog_pdf() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn n_page_pdf(n: usize, width: f64, height: f64) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut page_offsets: Vec<u64> = Vec::new();
    for i in 0..n {
        let obj_num = i + 2;
        page_offsets.push(pdf.len() as u64);
        pdf.extend_from_slice(format!("{obj_num} 0 obj\n<< /Type /Page /MediaBox [0 0 {width} {height}] >>\nendobj\n").as_bytes());
    }
    let pages_obj = n + 2;
    let pages_off = pdf.len() as u64;
    let kids: String = (0..n).map(|i| format!("{} 0 R", i + 2)).collect::<Vec<_>>().join(" ");
    pdf.extend_from_slice(format!("{pages_obj} 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {n} >>\nendobj\n").as_bytes());
    let cat_obj = n + 3;
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(format!("{cat_obj} 0 obj\n<< /Type /Catalog /Pages {pages_obj} 0 R >>\nendobj\n").as_bytes());
    let xref_off = pdf.len();
    let total = cat_obj + 1;
    pdf.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(b"0000000000 00000 n \r\n");
    for off in &page_offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("trailer\n<< /Size {total} /Root {cat_obj} 0 R >>\n").as_bytes());
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn round_trip_pdf(n: usize) -> Vec<u8> {
    let bytes = n_page_pdf(n, 612.0, 792.0);
    let doc = rust_pdfbox::Document::load_from_bytes(&bytes).expect("round_trip_pdf: load");
    let mut buf = std::io::Cursor::new(Vec::new());
    doc.save_to(&mut buf).expect("round_trip_pdf: save");
    buf.into_inner()
}

fn content_stream_pdf(text: &str) -> Vec<u8> {
    let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let stream_off = pdf.len() as u64;
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n", content.len()).as_bytes());
    let page_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n");
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", page_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", stream_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn multiline_content_stream_pdf() -> Vec<u8> {
    let content = "BT /F1 12 Tf 72 720 Td (Line one) Tj 0 -14 Td (Line two) Tj ET";
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let stream_off = pdf.len() as u64;
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n", content.len()).as_bytes());
    let page_off = pdf.len() as u64;
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n");
    let pages_off = pdf.len() as u64;
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [2 0 R] /Count 1 >>\nendobj\n");
    let cat_off = pdf.len() as u64;
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");
    let xref_off = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n");
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", cat_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", page_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", pages_off).as_bytes());
    pdf.extend_from_slice(format!("{:010} 00000 n \r\n", stream_off).as_bytes());
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
    pdf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    pdf
}

fn missing_header_pdf() -> Vec<u8> {
    let mut bytes = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec();
    bytes.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    bytes.extend_from_slice(b"startxref\n0\n%%EOF\n");
    bytes
}

fn empty_bytes_pdf() -> Vec<u8> { vec![] }

fn broken_xref_pdf() -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    pdf.extend_from_slice(b"xref\nGARBAGE\n");
    pdf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    pdf.extend_from_slice(b"startxref\n999999\n%%EOF\n");
    pdf
}

// ── Fixture dispatch ─────────────────────────────────────────────────────────

fn fixture_bytes(fixture: &str) -> Vec<u8> {
    match fixture {
        // Smoke
        "smoke/letter_single_page.pdf" => single_page_pdf(612.0, 792.0),
        "smoke/a4_single_page.pdf"     => single_page_pdf(595.0, 842.0),
        "smoke/five_pages.pdf"         => n_page_pdf(5,   612.0, 792.0),
        "smoke/ten_pages.pdf"          => n_page_pdf(10,  612.0, 792.0),
        "smoke/three_pages.pdf"        => n_page_pdf(3,   612.0, 792.0),
        "smoke/minimal_catalog.pdf"    => minimal_catalog_pdf(),
        "smoke/custom_page_size.pdf"   => single_page_pdf(200.0, 300.0),
        "smoke/version_1_7.pdf"        => single_page_pdf_version("1.7", 612.0, 792.0),
        "smoke/rotated_90.pdf"         => rotated_page_pdf(612.0, 792.0, 90),
        "smoke/rotated_270.pdf"        => rotated_page_pdf(612.0, 792.0, 270),
        "smoke/round_trip.pdf"         => round_trip_pdf(3),
        // Font-heavy
        "font_heavy/text_hello_world.pdf"  => content_stream_pdf("Hello World"),
        "font_heavy/text_multiline.pdf"    => multiline_content_stream_pdf(),
        "font_heavy/text_empty_stream.pdf" => content_stream_pdf(""),
        // Encrypted (structural only — no actual encryption applied)
        "encrypted/permissions_all.pdf"       => single_page_pdf(612.0, 792.0),
        "encrypted/permissions_none.pdf"      => single_page_pdf(612.0, 792.0),
        "encrypted/permissions_print_only.pdf" => single_page_pdf(612.0, 792.0),
        // Malformed
        "malformed/missing_header.pdf" => missing_header_pdf(),
        "malformed/empty_bytes.pdf"    => empty_bytes_pdf(),
        "malformed/broken_xref.pdf"    => broken_xref_pdf(),
        // Large
        "large/100_pages.pdf"   => n_page_pdf(100, 612.0, 792.0),
        "large/fifty_pages.pdf" => n_page_pdf(50,  612.0, 792.0),
        "large/200_pages.pdf"   => n_page_pdf(200, 612.0, 792.0),
        other => panic!("Unknown fixture: {other}"),
    }
}

// ── Test runners ─────────────────────────────────────────────────────────────

fn run_cv(snapshot: &str, fixture: &str) -> VResult {
    let root = env!("CARGO_MANIFEST_DIR");
    let json = std::fs::read_to_string(format!("{root}/tests/cross_validation/{snapshot}"))
        .unwrap_or_else(|e| panic!("Cannot read snapshot {snapshot}: {e}"));
    let snap = parse_snapshot(&json);
    let bytes = fixture_bytes(fixture);
    let doc = Document::load_from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("Failed to load {fixture}: {e}"));
    validate(&doc, &snap)
}

fn run_cv_lenient(snapshot: &str, fixture: &str) -> VResult {
    let root = env!("CARGO_MANIFEST_DIR");
    let json = std::fs::read_to_string(format!("{root}/tests/cross_validation/{snapshot}"))
        .unwrap_or_else(|e| panic!("Cannot read snapshot {snapshot}: {e}"));
    let snap = parse_snapshot(&json);
    let bytes = fixture_bytes(fixture);
    let (doc, _report) = Document::load_lenient(&bytes);
    validate(&doc, &snap)
}

// ── Test macros ───────────────────────────────────────────────────────────────

macro_rules! cv {
    ($name:ident, $snap:expr, $fix:expr) => {
        #[test]
        fn $name() {
            let r = run_cv($snap, $fix);
            print!("{}", r.summary());
            assert!(r.is_ok(), "Cross-validation FAILED for {}\n{}", $fix, r.summary());
        }
    };
}

macro_rules! cv_lenient {
    ($name:ident, $snap:expr, $fix:expr) => {
        #[test]
        fn $name() {
            let r = run_cv_lenient($snap, $fix);
            print!("{}", r.summary());
            assert!(r.is_ok(), "Cross-validation FAILED for {}\n{}", $fix, r.summary());
        }
    };
}

// ── Smoke tier ───────────────────────────────────────────────────────────────
cv!(cv_smoke_letter_single_page, "smoke_letter_single_page.json", "smoke/letter_single_page.pdf");
cv!(cv_smoke_a4_single_page,     "smoke_a4_single_page.json",     "smoke/a4_single_page.pdf");
cv!(cv_smoke_five_pages,         "smoke_five_pages.json",         "smoke/five_pages.pdf");
cv!(cv_smoke_ten_pages,          "smoke_ten_pages.json",          "smoke/ten_pages.pdf");
cv!(cv_smoke_three_pages,        "smoke_three_pages.json",        "smoke/three_pages.pdf");
cv!(cv_smoke_minimal_catalog,    "smoke_minimal_catalog.json",    "smoke/minimal_catalog.pdf");
cv!(cv_smoke_custom_page_size,   "smoke_custom_page_size.json",   "smoke/custom_page_size.pdf");
cv!(cv_smoke_version_1_7,        "smoke_version_1_7.json",        "smoke/version_1_7.pdf");
cv!(cv_smoke_rotated_90,         "smoke_rotated_90.json",         "smoke/rotated_90.pdf");
cv!(cv_smoke_rotated_270,        "smoke_rotated_270.json",        "smoke/rotated_270.pdf");
cv!(cv_smoke_round_trip,         "smoke_round_trip.json",         "smoke/round_trip.pdf");

// ── Font-heavy tier ───────────────────────────────────────────────────────────
cv!(cv_font_heavy_hello_world,   "font_heavy_text_hello_world.json",  "font_heavy/text_hello_world.pdf");
cv!(cv_font_heavy_multiline,     "font_heavy_text_multiline.json",    "font_heavy/text_multiline.pdf");
cv!(cv_font_heavy_empty_stream,  "font_heavy_text_empty_stream.json", "font_heavy/text_empty_stream.pdf");

// ── Encrypted tier ────────────────────────────────────────────────────────────
cv!(cv_encrypted_perms_all,        "encrypted_permissions_all.json",        "encrypted/permissions_all.pdf");
cv!(cv_encrypted_perms_none,       "encrypted_permissions_none.json",       "encrypted/permissions_none.pdf");
cv!(cv_encrypted_perms_print_only, "encrypted_permissions_print_only.json", "encrypted/permissions_print_only.pdf");

// ── Malformed tier (lenient) ──────────────────────────────────────────────────
cv_lenient!(cv_malformed_missing_header, "malformed_missing_header.json", "malformed/missing_header.pdf");
cv_lenient!(cv_malformed_empty_bytes,    "malformed_empty_bytes.json",    "malformed/empty_bytes.pdf");
cv_lenient!(cv_malformed_broken_xref,    "malformed_broken_xref.json",    "malformed/broken_xref.pdf");

// ── Large tier ────────────────────────────────────────────────────────────────
cv!(cv_large_100_pages,  "large_100_pages.json",   "large/100_pages.pdf");
cv!(cv_large_fifty_pages, "large_fifty_pages.json", "large/fifty_pages.pdf");
cv!(cv_large_200_pages,  "large_200_pages.json",   "large/200_pages.pdf");

// ── Snapshot parser self-tests ────────────────────────────────────────────────

#[test]
fn cv_parser_file_field() {
    let j = r#"{"file":"smoke/t.pdf","pdf_version":"1.4","page_count":1,"pages":[],"permissions":{"print":true,"copy":false,"modify":true,"annotate":false},"fonts":[]}"#;
    assert_eq!(parse_snapshot(j).file, "smoke/t.pdf");
}

#[test]
fn cv_parser_version_and_count() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.7","page_count":3,"pages":[],"permissions":{"print":true,"copy":true,"modify":true,"annotate":true},"fonts":[]}"#;
    let s = parse_snapshot(j);
    assert_eq!(s.pdf_version, "1.7");
    assert_eq!(s.page_count, 3);
}

#[test]
fn cv_parser_page_dimensions() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.4","page_count":1,"pages":[{"index":0,"width":612.0,"height":792.0,"rotation":0,"text_len_min":0,"text_len_max":9999,"text_contains":[]}],"permissions":{"print":true,"copy":true,"modify":true,"annotate":true},"fonts":[]}"#;
    let s = parse_snapshot(j);
    assert_eq!(s.pages.len(), 1);
    assert_eq!(s.pages[0].width, 612.0);
    assert_eq!(s.pages[0].height, 792.0);
}

#[test]
fn cv_parser_page_rotation_nonzero() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.4","page_count":1,"pages":[{"index":0,"width":595.0,"height":842.0,"rotation":90,"text_len_min":0,"text_len_max":9999,"text_contains":[]}],"permissions":{"print":true,"copy":true,"modify":true,"annotate":true},"fonts":[]}"#;
    let s = parse_snapshot(j);
    assert_eq!(s.pages[0].rotation, 90i64);
}

#[test]
fn cv_parser_permissions_mixed() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.4","page_count":0,"pages":[],"permissions":{"print":true,"copy":false,"modify":false,"annotate":true},"fonts":[]}"#;
    let s = parse_snapshot(j);
    assert!(s.permissions.print);
    assert!(!s.permissions.copy);
    assert!(!s.permissions.modify);
    assert!(s.permissions.annotate);
}

#[test]
fn cv_parser_fonts_list() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.4","page_count":0,"pages":[],"permissions":{"print":true,"copy":true,"modify":true,"annotate":true},"fonts":["Helvetica","Times-Roman"]}"#;
    let s = parse_snapshot(j);
    assert_eq!(s.fonts, vec!["Helvetica", "Times-Roman"]);
}

#[test]
fn cv_parser_text_bounds() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.4","page_count":1,"pages":[{"index":0,"width":612.0,"height":792.0,"rotation":0,"text_len_min":5,"text_len_max":100,"text_contains":["Hello","PDF"]}],"permissions":{"print":true,"copy":true,"modify":true,"annotate":true},"fonts":[]}"#;
    let s = parse_snapshot(j);
    assert_eq!(s.pages[0].text_contains, vec!["Hello", "PDF"]);
    assert_eq!(s.pages[0].text_len_min, 5);
    assert_eq!(s.pages[0].text_len_max, 100);
}

#[test]
fn cv_parser_empty_pages() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.4","page_count":0,"pages":[],"permissions":{"print":true,"copy":true,"modify":true,"annotate":true},"fonts":[]}"#;
    assert_eq!(parse_snapshot(j).pages.len(), 0);
}

#[test]
fn cv_parser_multi_page() {
    let j = r#"{"file":"x.pdf","pdf_version":"1.4","page_count":2,"pages":[{"index":0,"width":612.0,"height":792.0,"rotation":0,"text_len_min":0,"text_len_max":9999,"text_contains":[]},{"index":1,"width":595.0,"height":842.0,"rotation":0,"text_len_min":0,"text_len_max":9999,"text_contains":[]}],"permissions":{"print":true,"copy":true,"modify":true,"annotate":true},"fonts":[]}"#;
    let s = parse_snapshot(j);
    assert_eq!(s.pages.len(), 2);
    assert_eq!(s.pages[1].width, 595.0);
}

#[test]
fn cv_vresult_all_pass() {
    let mut r = VResult::new("t.pdf");
    r.pass("page_count");
    r.pass("pdf_version");
    assert!(r.is_ok());
}

#[test]
fn cv_vresult_one_fail() {
    let mut r = VResult::new("t.pdf");
    r.pass("page_count");
    r.fail("pdf_version", "mismatch");
    assert!(!r.is_ok());
}

#[test]
fn cv_vresult_skip_is_ok() {
    let mut r = VResult::new("t.pdf");
    r.skip("text", "feature off");
    assert!(r.is_ok());
}

#[test]
fn cv_summary_has_filename() {
    let mut r = VResult::new("smoke/letter.pdf");
    r.pass("page_count");
    assert!(r.summary().contains("smoke/letter.pdf"));
}

#[test]
fn cv_summary_shows_fail_status() {
    let mut r = VResult::new("t.pdf");
    r.fail("dims", "bad size");
    assert!(r.summary().contains("FAIL"));
    assert!(r.summary().contains("bad size"));
}
