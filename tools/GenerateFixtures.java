/**
 * GenerateFixtures.java — Generates all PDF test fixtures using Apache PDFBox 3.0.7.
 *
 * Usage:
 *   cd rust-pdfbox
 *   javac -cp pdfbox-app-3.0.7.jar tools/GenerateFixtures.java
 *   java  -cp pdfbox-app-3.0.7.jar:tools GenerateFixtures
 *
 * Output goes to tests/fixtures/{smoke,font_heavy,encrypted,large,malformed}/*.pdf
 */

import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.PDPageContentStream;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.encryption.AccessPermission;
import org.apache.pdfbox.pdmodel.encryption.StandardProtectionPolicy;
import org.apache.pdfbox.pdmodel.font.PDType1Font;
import org.apache.pdfbox.pdmodel.font.Standard14Fonts;

import org.apache.pdfbox.Loader;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

public class GenerateFixtures {

    static final String BASE = "tests/fixtures";

    public static void main(String[] args) throws Exception {
        // Ensure output directories exist
        for (String tier : new String[]{"smoke", "font_heavy", "encrypted", "large", "malformed"}) {
            Files.createDirectories(Paths.get(BASE, tier));
        }

        System.out.println("=== Generating PDF fixtures with PDFBox 3.0.7 ===\n");

        // ── Smoke tier ─────────────────────────────────────────────────────
        genSmoke_A4SinglePage();
        genSmoke_LetterSinglePage();
        genSmoke_CustomPageSize();
        genSmoke_ThreePages();
        genSmoke_FivePages();
        genSmoke_TenPages();
        genSmoke_MinimalCatalog();
        genSmoke_Version17();
        genSmoke_Rotated90();
        genSmoke_Rotated270();
        genSmoke_RoundTrip();

        // ── Font-heavy tier ────────────────────────────────────────────────
        genFontHeavy_HelloWorld();
        genFontHeavy_Multiline();
        genFontHeavy_EmptyStream();

        // ── Encrypted tier ─────────────────────────────────────────────────
        genEncrypted_PermissionsAll();
        genEncrypted_PermissionsNone();
        genEncrypted_PermissionsPrintOnly();

        // ── Large tier ─────────────────────────────────────────────────────
        genLarge_FiftyPages();
        genLarge_100Pages();
        genLarge_200Pages();

        // ── Malformed tier ─────────────────────────────────────────────────
        genMalformed_MissingHeader();
        genMalformed_EmptyBytes();
        genMalformed_BrokenXref();

        System.out.println("\n=== All fixtures generated successfully ===");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // SMOKE
    // ═══════════════════════════════════════════════════════════════════════

    static void genSmoke_A4SinglePage() throws IOException {
        String path = path("smoke", "a4_single_page.pdf");
        try (PDDocument doc = new PDDocument()) {
            doc.addPage(new PDPage(PDRectangle.A4));
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_LetterSinglePage() throws IOException {
        String path = path("smoke", "letter_single_page.pdf");
        try (PDDocument doc = new PDDocument()) {
            doc.addPage(new PDPage(PDRectangle.LETTER));
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_CustomPageSize() throws IOException {
        String path = path("smoke", "custom_page_size.pdf");
        try (PDDocument doc = new PDDocument()) {
            doc.addPage(new PDPage(new PDRectangle(200f, 300f)));
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_ThreePages() throws IOException {
        String path = path("smoke", "three_pages.pdf");
        try (PDDocument doc = new PDDocument()) {
            for (int i = 0; i < 3; i++) {
                doc.addPage(new PDPage(PDRectangle.LETTER));
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_FivePages() throws IOException {
        String path = path("smoke", "five_pages.pdf");
        try (PDDocument doc = new PDDocument()) {
            for (int i = 0; i < 5; i++) {
                doc.addPage(new PDPage(PDRectangle.LETTER));
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_TenPages() throws IOException {
        String path = path("smoke", "ten_pages.pdf");
        try (PDDocument doc = new PDDocument()) {
            for (int i = 0; i < 10; i++) {
                doc.addPage(new PDPage(PDRectangle.LETTER));
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_MinimalCatalog() throws IOException {
        // A valid PDF with zero pages — just catalog + empty pages dict
        String path = path("smoke", "minimal_catalog.pdf");
        try (PDDocument doc = new PDDocument()) {
            // PDFBox requires at least one page to save, so we add and remove
            // Instead: save an empty doc (PDFBox 3.x allows 0-page docs)
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_Version17() throws IOException {
        String path = path("smoke", "version_1_7.pdf");
        try (PDDocument doc = new PDDocument()) {
            doc.addPage(new PDPage(PDRectangle.LETTER));
            doc.setVersion(1.7f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_Rotated90() throws IOException {
        String path = path("smoke", "rotated_90.pdf");
        try (PDDocument doc = new PDDocument()) {
            PDPage page = new PDPage(PDRectangle.LETTER);
            page.setRotation(90);
            doc.addPage(page);
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_Rotated270() throws IOException {
        String path = path("smoke", "rotated_270.pdf");
        try (PDDocument doc = new PDDocument()) {
            PDPage page = new PDPage(PDRectangle.LETTER);
            page.setRotation(270);
            doc.addPage(page);
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genSmoke_RoundTrip() throws IOException {
        // Create 3-page doc, save, reload, save again — the "round trip" fixture
        String path = path("smoke", "round_trip.pdf");
        byte[] firstPass;
        try (PDDocument doc = new PDDocument()) {
            for (int i = 0; i < 3; i++) {
                doc.addPage(new PDPage(PDRectangle.LETTER));
            }
            doc.setVersion(1.4f);
            java.io.ByteArrayOutputStream baos = new java.io.ByteArrayOutputStream();
            doc.save(baos);
            firstPass = baos.toByteArray();
        }
        // Reload and save again
        try (PDDocument doc = Loader.loadPDF(firstPass)) {
            doc.save(path);
        }
        ok(path);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // FONT-HEAVY
    // ═══════════════════════════════════════════════════════════════════════

    static void genFontHeavy_HelloWorld() throws IOException {
        String path = path("font_heavy", "text_hello_world.pdf");
        try (PDDocument doc = new PDDocument()) {
            PDPage page = new PDPage(PDRectangle.LETTER);
            doc.addPage(page);
            PDType1Font font = new PDType1Font(Standard14Fonts.FontName.HELVETICA);
            try (PDPageContentStream cs = new PDPageContentStream(doc, page)) {
                cs.beginText();
                cs.setFont(font, 12);
                cs.newLineAtOffset(72, 720);
                cs.showText("Hello World");
                cs.endText();
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genFontHeavy_Multiline() throws IOException {
        String path = path("font_heavy", "text_multiline.pdf");
        try (PDDocument doc = new PDDocument()) {
            PDPage page = new PDPage(PDRectangle.LETTER);
            doc.addPage(page);
            PDType1Font font = new PDType1Font(Standard14Fonts.FontName.HELVETICA);
            try (PDPageContentStream cs = new PDPageContentStream(doc, page)) {
                cs.beginText();
                cs.setFont(font, 12);
                cs.newLineAtOffset(72, 720);
                cs.showText("Line one");
                cs.newLineAtOffset(0, -14);
                cs.showText("Line two");
                cs.endText();
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genFontHeavy_EmptyStream() throws IOException {
        String path = path("font_heavy", "text_empty_stream.pdf");
        try (PDDocument doc = new PDDocument()) {
            PDPage page = new PDPage(PDRectangle.LETTER);
            doc.addPage(page);
            // Create an empty content stream (BT ET with no text operators)
            try (PDPageContentStream cs = new PDPageContentStream(doc, page)) {
                // intentionally empty — produces a content stream with no visible text
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ENCRYPTED
    // ═══════════════════════════════════════════════════════════════════════

    static void genEncrypted_PermissionsAll() throws IOException {
        String path = path("encrypted", "permissions_all.pdf");
        try (PDDocument doc = new PDDocument()) {
            doc.addPage(new PDPage(PDRectangle.LETTER));
            doc.setVersion(1.4f);

            AccessPermission ap = new AccessPermission();
            ap.setCanPrint(true);
            ap.setCanExtractContent(true);
            ap.setCanModify(true);
            ap.setCanModifyAnnotations(true);
            ap.setCanFillInForm(true);
            ap.setCanAssembleDocument(true);
            ap.setCanPrintFaithful(true);
            ap.setCanExtractForAccessibility(true);

            StandardProtectionPolicy policy = new StandardProtectionPolicy("owner", "", ap);
            policy.setEncryptionKeyLength(128);
            doc.protect(policy);
            doc.save(path);
        }
        ok(path);
    }

    static void genEncrypted_PermissionsNone() throws IOException {
        String path = path("encrypted", "permissions_none.pdf");
        try (PDDocument doc = new PDDocument()) {
            doc.addPage(new PDPage(PDRectangle.LETTER));
            doc.setVersion(1.4f);

            AccessPermission ap = new AccessPermission();
            ap.setCanPrint(false);
            ap.setCanExtractContent(false);
            ap.setCanModify(false);
            ap.setCanModifyAnnotations(false);
            ap.setCanFillInForm(false);
            ap.setCanAssembleDocument(false);
            ap.setCanPrintFaithful(false);
            ap.setCanExtractForAccessibility(false);

            StandardProtectionPolicy policy = new StandardProtectionPolicy("owner", "", ap);
            policy.setEncryptionKeyLength(128);
            doc.protect(policy);
            doc.save(path);
        }
        ok(path);
    }

    static void genEncrypted_PermissionsPrintOnly() throws IOException {
        String path = path("encrypted", "permissions_print_only.pdf");
        try (PDDocument doc = new PDDocument()) {
            doc.addPage(new PDPage(PDRectangle.LETTER));
            doc.setVersion(1.4f);

            AccessPermission ap = new AccessPermission();
            ap.setCanPrint(true);
            ap.setCanExtractContent(false);
            ap.setCanModify(false);
            ap.setCanModifyAnnotations(false);
            ap.setCanFillInForm(false);
            ap.setCanAssembleDocument(false);
            ap.setCanPrintFaithful(true);
            ap.setCanExtractForAccessibility(false);

            StandardProtectionPolicy policy = new StandardProtectionPolicy("owner", "", ap);
            policy.setEncryptionKeyLength(128);
            doc.protect(policy);
            doc.save(path);
        }
        ok(path);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // LARGE
    // ═══════════════════════════════════════════════════════════════════════

    static void genLarge_FiftyPages() throws IOException {
        String path = path("large", "fifty_pages.pdf");
        try (PDDocument doc = new PDDocument()) {
            for (int i = 0; i < 50; i++) {
                doc.addPage(new PDPage(PDRectangle.LETTER));
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genLarge_100Pages() throws IOException {
        String path = path("large", "100_pages.pdf");
        try (PDDocument doc = new PDDocument()) {
            for (int i = 0; i < 100; i++) {
                doc.addPage(new PDPage(PDRectangle.LETTER));
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    static void genLarge_200Pages() throws IOException {
        String path = path("large", "200_pages.pdf");
        try (PDDocument doc = new PDDocument()) {
            for (int i = 0; i < 200; i++) {
                doc.addPage(new PDPage(PDRectangle.LETTER));
            }
            doc.setVersion(1.4f);
            doc.save(path);
        }
        ok(path);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // MALFORMED — These are crafted raw bytes, NOT from the PDFBox API.
    // ═══════════════════════════════════════════════════════════════════════

    static void genMalformed_MissingHeader() throws IOException {
        // A PDF-like file with no %PDF- header
        String path = path("malformed", "missing_header.pdf");
        StringBuilder sb = new StringBuilder();
        sb.append("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        sb.append("2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        sb.append("xref\n0 3\n");
        sb.append("0000000000 65535 f \r\n");
        sb.append("0000000000 00000 n \r\n");
        sb.append("0000000049 00000 n \r\n");
        sb.append("trailer\n<< /Size 3 /Root 1 0 R >>\n");
        sb.append("startxref\n99\n%%EOF\n");
        Files.write(Paths.get(path), sb.toString().getBytes());
        ok(path);
    }

    static void genMalformed_EmptyBytes() throws IOException {
        // Completely empty file — 0 bytes
        String path = path("malformed", "empty_bytes.pdf");
        Files.write(Paths.get(path), new byte[0]);
        ok(path);
    }

    static void genMalformed_BrokenXref() throws IOException {
        // Valid header + body, but xref section is garbage
        String path = path("malformed", "broken_xref.pdf");
        StringBuilder sb = new StringBuilder();
        sb.append("%PDF-1.4\n");
        sb.append("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        sb.append("2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        sb.append("xref\nGARBAGE NOT A VALID XREF\n");
        sb.append("trailer\n<< /Size 3 /Root 1 0 R >>\n");
        sb.append("startxref\n999999\n%%EOF\n");
        Files.write(Paths.get(path), sb.toString().getBytes());
        ok(path);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Helpers
    // ═══════════════════════════════════════════════════════════════════════

    static String path(String tier, String name) {
        return BASE + "/" + tier + "/" + name;
    }

    static void ok(String path) {
        File f = new File(path);
        System.out.printf("  ✓ %-50s %,8d bytes%n", path, f.length());
    }
}

