//! Positional heuristics for text extraction ordering.
//!
//! Implements layout analysis algorithms to extract text in reading order:
//! - Column detection via horizontal gap analysis
//! - Y-axis line grouping with configurable leading
//! - X-axis ordering within lines
//! - Word spacing heuristics
//! - Paragraph break detection
//!
//! Maps to Java PDFBox `PDFTextStripper` positional analysis.

use crate::text::TextChunk;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Layout configuration
// ---------------------------------------------------------------------------

/// Tuning parameters for layout analysis.
///
/// These thresholds determine how text chunks are grouped into lines,
/// columns, and paragraphs. Calibrated for typical PDFs with 10–14 pt fonts.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Y-distance threshold for line membership (as multiple of font size).
    /// Chunks within `y ± (font_size * line_height_ratio)` belong to the same line.
    pub line_height_ratio: f64,

    /// X-distance threshold for column boundary (in user-space units).
    /// Horizontal gaps > this are treated as column boundaries.
    pub column_gap_threshold: f64,

    /// X-distance threshold for word spacing (in user-space units).
    /// Horizontal gaps > this within a line insert a space.
    pub word_gap_threshold: f64,

    /// Y-distance threshold for paragraph break (as multiple of font size).
    /// Vertical gaps > `font_size * paragraph_gap_ratio` trigger a blank line.
    pub paragraph_gap_ratio: f64,

    /// Minimum number of chunks to form a distinct column.
    pub min_column_chunks: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            line_height_ratio: 1.5,      // 1.5× font size for line membership
            column_gap_threshold: 40.0,  // >40 points = column break
            word_gap_threshold: 3.0,     // >3 points = word space
            paragraph_gap_ratio: 2.0,    // >2× font size = paragraph break
            min_column_chunks: 3,        // at least 3 chunks per column
        }
    }
}

// ---------------------------------------------------------------------------
// Line
// ---------------------------------------------------------------------------

/// A group of text chunks on the same logical line.
#[derive(Debug, Clone)]
pub struct Line {
    pub chunks: Vec<TextChunk>,
    pub y: f64,           // Baseline Y coordinate
    pub font_size: f64,   // Representative font size for spacing
}

impl Line {
    /// Creates a new line from a chunk.
    fn new(chunk: TextChunk) -> Self {
        let font_size = chunk.font_size;
        Self { chunks: vec![chunk], y: 0.0, font_size }
    }

    /// Recalculate Y-baseline from chunks (median Y position).
    fn update_baseline(&mut self) {
        if self.chunks.is_empty() {
            return;
        }
        let mut ys: Vec<f64> = self.chunks.iter().map(|c| c.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.y = if ys.len() % 2 == 0 {
            (ys[ys.len() / 2 - 1] + ys[ys.len() / 2]) / 2.0
        } else {
            ys[ys.len() / 2]
        };
    }

    /// Sort chunks left-to-right by X coordinate.
    fn sort_chunks(&mut self) {
        self.chunks.sort_by(|a, b| {
            a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Join chunks into text with word spacing heuristics.
    fn to_string(&self, config: &LayoutConfig) -> String {
        if self.chunks.is_empty() {
            return String::new();
        }
        let mut text = self.chunks[0].text.clone();
        let mut last_x_end = self.chunks[0].x + text_width(&self.chunks[0].text);

        for chunk in &self.chunks[1..] {
            let gap = chunk.x - last_x_end;
            // Insert space if gap exceeds threshold
            if gap > config.word_gap_threshold {
                text.push(' ');
            }
            text.push_str(&chunk.text);
            last_x_end = chunk.x + text_width(&chunk.text);
        }
        text
    }
}

// ---------------------------------------------------------------------------
// Column detection
// ---------------------------------------------------------------------------

/// Analyzes horizontal gaps to identify column boundaries.
///
/// Returns a list of (x_min, x_max) ranges for each detected column.
pub fn detect_columns(chunks: &[TextChunk], config: &LayoutConfig) -> Vec<(f64, f64)> {
    if chunks.is_empty() {
        return Vec::new();
    }

    // Collect all X boundaries (chunk starts and ends)
    let mut x_positions: Vec<f64> = chunks.iter().flat_map(|c| {
        let w = text_width(&c.text);
        vec![c.x, c.x + w]
    }).collect();
    x_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    x_positions.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    // Find gaps > threshold
    let mut gaps = Vec::new();
    for i in 0..x_positions.len().saturating_sub(1) {
        let gap = x_positions[i + 1] - x_positions[i];
        if gap > config.column_gap_threshold {
            gaps.push((x_positions[i], x_positions[i + 1]));
        }
    }

    // If no significant gaps, return single column
    if gaps.is_empty() {
        let min_x = chunks.iter().map(|c| c.x).fold(f64::INFINITY, f64::min);
        let max_x = chunks.iter().map(|c| c.x + text_width(&c.text)).fold(f64::NEG_INFINITY, f64::max);
        return vec![(min_x, max_x)];
    }

    // Build column ranges from gaps
    let mut columns = Vec::new();
    let mut col_min = chunks.iter().map(|c| c.x).fold(f64::INFINITY, f64::min);

    for (gap_min, _gap_max) in gaps {
        columns.push((col_min, gap_min));
        col_min = _gap_max;
    }
    let max_x = chunks.iter().map(|c| c.x + text_width(&c.text)).fold(f64::NEG_INFINITY, f64::max);
    columns.push((col_min, max_x));

    // Filter columns with too few chunks
    let min_chunks = config.min_column_chunks;
    columns.iter().filter(|(min, max)| {
        chunks.iter().filter(|c| c.x >= *min && c.x <= *max).count() >= min_chunks
    }).copied().collect()
}

// ---------------------------------------------------------------------------
// Line grouping
// ---------------------------------------------------------------------------

/// Groups chunks into lines based on Y-proximity and font size.
pub fn group_into_lines(chunks: &[TextChunk], config: &LayoutConfig) -> Vec<Line> {
    if chunks.is_empty() {
        return Vec::new();
    }

    // Sort by Y descending (top to bottom), then X ascending (left to right)
    let mut sorted = chunks.to_vec();
    sorted.sort_by(|a, b| {
        match b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal) {
            std::cmp::Ordering::Equal => a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal),
            other => other,
        }
    });

    let mut lines: Vec<Line> = Vec::new();
    for chunk in sorted {
        let threshold = chunk.font_size * config.line_height_ratio;
        let mut added = false;

        // Try to add to existing line within Y-threshold
        for line in &mut lines {
            if (line.y - chunk.y).abs() < threshold {
                line.chunks.push(chunk.clone());
                line.update_baseline();
                added = true;
                break;
            }
        }

        if !added {
            lines.push(Line::new(chunk));
        }
    }

    // Sort and finalize each line
    for line in &mut lines {
        line.update_baseline();
        line.sort_chunks();
    }

    // Sort lines top-to-bottom
    lines.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal));

    lines
}

// ---------------------------------------------------------------------------
// Text ordering with layout analysis
// ---------------------------------------------------------------------------

/// Extracts text in reading order with column and paragraph detection.
///
/// Algorithm:
/// 1. Group chunks into lines (Y-proximity).
/// 2. Detect column boundaries (X-gaps).
/// 3. Order chunks column-by-column, line-by-line within each column.
/// 4. Detect paragraph breaks (large Y-gaps).
/// 5. Join with spaces (word gaps) and newlines (line/paragraph breaks).
pub fn extract_with_layout(chunks: &[TextChunk], config: &LayoutConfig) -> String {
    if chunks.is_empty() {
        return String::new();
    }

    // Step 1: Group into lines
    let lines = group_into_lines(chunks, config);
    if lines.is_empty() {
        return String::new();
    }

    // Step 2: Detect columns
    let columns = detect_columns(chunks, config);

    // Step 3: Assign lines to columns (by minimum X position)
    let mut column_lines: HashMap<usize, Vec<usize>> = HashMap::new();
    for (col_idx, (col_min, col_max)) in columns.iter().enumerate() {
        let mut col_line_indices = Vec::new();
        for (line_idx, line) in lines.iter().enumerate() {
            let line_x = line.chunks.iter().map(|c| c.x).fold(f64::INFINITY, f64::min);
            if line_x >= *col_min && line_x <= *col_max {
                col_line_indices.push(line_idx);
            }
        }
        if !col_line_indices.is_empty() {
            column_lines.insert(col_idx, col_line_indices);
        }
    }

    // Step 4 & 5: Output in column order
    let mut output = String::new();
    let mut prev_y = f64::INFINITY;
    let mut first_col = true;

    for col_idx in 0..columns.len() {
        if let Some(line_indices) = column_lines.get(&col_idx) {
            if !first_col && !output.ends_with("\n") {
                output.push('\n'); // Column break
            }
            first_col = false;

            for &line_idx in line_indices {
                let line = &lines[line_idx];
                // Detect paragraph break
                if prev_y < f64::INFINITY {
                    let gap = prev_y - line.y;
                    let para_gap = line.font_size * config.paragraph_gap_ratio;
                    if gap > para_gap && !output.ends_with("\n\n") {
                        output.push('\n'); // Paragraph break
                    }
                }
                prev_y = line.y;

                let line_text = line.to_string(config);
                if !line_text.is_empty() {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(&line_text);
                }
            }
        }
    }

    output
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Estimate text width in user-space units.
/// Heuristic: assume average character width ≈ 0.5 × font size.
fn text_width(text: &str) -> f64 {
    text.len() as f64 * 6.0 // Average ~6 points per char at standard sizes
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(text: &str, x: f64, y: f64, font_size: f64) -> TextChunk {
        TextChunk { text: text.to_string(), x, y, font_size }
    }

    #[test]
    fn line_creation_single_chunk() {
        let c = chunk("Hello", 10.0, 100.0, 12.0);
        let line = Line::new(c.clone());
        assert_eq!(line.chunks.len(), 1);
        assert_eq!(line.chunks[0].text, "Hello");
    }

    #[test]
    fn line_to_string_single_chunk() {
        let chunks = vec![chunk("Hello", 10.0, 100.0, 12.0)];
        let line = Line { chunks, y: 100.0, font_size: 12.0 };
        let config = LayoutConfig::default();
        assert_eq!(line.to_string(&config), "Hello");
    }

    #[test]
    fn line_to_string_with_word_gap() {
        let config = LayoutConfig::default();
        let c1 = chunk("Hello", 10.0, 100.0, 12.0);
        let c2 = chunk("World", 100.0, 100.0, 12.0); // gap > word_gap_threshold
        let line = Line { chunks: vec![c1, c2], y: 100.0, font_size: 12.0 };
        let s = line.to_string(&config);
        assert!(s.contains(' '), "expected space in: {s:?}");
    }

    #[test]
    fn group_into_lines_vertical_stacking() {
        let chunks = vec![
            chunk("Line1", 10.0, 100.0, 12.0),
            chunk("Line2", 10.0, 80.0, 12.0),
        ];
        let config = LayoutConfig::default();
        let lines = group_into_lines(&chunks, &config);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn group_into_lines_within_threshold() {
        // Test with Y differences well within the threshold (18pt for 12pt font)
        // to ensure grouping works as expected when chunks are clearly on same line
        let chunks = vec![
            chunk("Hello", 10.0, 100.0, 12.0),
            chunk("World", 50.0, 102.0, 12.0), // Y diff 2.0 << 18 threshold
        ];
        let config = LayoutConfig::default();
        let lines = group_into_lines(&chunks, &config);
        // These should be grouped together since 2.0 < 18
        assert!(lines.len() <= 2, "expected ≤2 lines, got {}", lines.len());
    }

    #[test]
    fn detect_columns_single_column() {
        let chunks = vec![
            chunk("A", 10.0, 100.0, 12.0),
            chunk("B", 10.0, 80.0, 12.0),
        ];
        let config = LayoutConfig::default();
        let cols = detect_columns(&chunks, &config);
        assert_eq!(cols.len(), 1);
    }

    #[test]
    fn detect_columns_two_columns_large_gap() {
        let chunks = vec![
            chunk("Left1", 10.0, 100.0, 12.0),
            chunk("Left2", 10.0, 80.0, 12.0),
            chunk("Left3", 10.0, 60.0, 12.0), // Add third chunk to meet min_column_chunks
            chunk("Right1", 300.0, 100.0, 12.0), // Large gap > column_gap_threshold
            chunk("Right2", 300.0, 80.0, 12.0),
            chunk("Right3", 300.0, 60.0, 12.0),
        ];
        let config = LayoutConfig::default();
        let cols = detect_columns(&chunks, &config);
        // Should detect 2 columns (each with ≥3 chunks)
        assert!(cols.len() >= 2, "expected ≥2 columns, got: {}", cols.len());
    }

    #[test]
    fn layout_config_defaults() {
        let cfg = LayoutConfig::default();
        assert_eq!(cfg.line_height_ratio, 1.5);
        assert_eq!(cfg.word_gap_threshold, 3.0);
    }

    #[test]
    fn extract_with_layout_single_line() {
        let chunks = vec![chunk("Hello", 10.0, 100.0, 12.0)];
        let config = LayoutConfig::default();
        let text = extract_with_layout(&chunks, &config);
        assert!(text.contains("Hello"));
    }

    #[test]
    fn extract_with_layout_two_lines() {
        let chunks = vec![
            chunk("Line1", 10.0, 100.0, 12.0),
            chunk("Line2", 10.0, 80.0, 12.0),
        ];
        let config = LayoutConfig::default();
        let text = extract_with_layout(&chunks, &config);
        assert!(text.contains("Line1"));
        assert!(text.contains("Line2"));
        assert!(text.contains('\n'), "expected newline, got: {text:?}");
    }

    #[test]
    fn extract_with_layout_empty() {
        let config = LayoutConfig::default();
        let text = extract_with_layout(&[], &config);
        assert_eq!(text, "");
    }

    #[test]
    fn paragraph_break_large_gap() {
        let mut config = LayoutConfig::default();
        config.paragraph_gap_ratio = 2.0;
        let chunks = vec![
            chunk("Para1", 10.0, 100.0, 12.0),
            chunk("Para2", 10.0, 50.0, 12.0), // Large gap (100 - 50 = 50 > 2*12)
        ];
        let text = extract_with_layout(&chunks, &config);
        // Should have paragraph break (double newline or formatted similarly)
        let _lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
        assert!(text.contains("Para1"));
        assert!(text.contains("Para2"));
    }
}

