//!
//! Page Manipulation module.
//!
//! Maps to `org.apache.pdfbox.multipdf.*` in Java PDFBox.

pub mod extract;
pub mod merge;
pub mod overlay;
pub mod rotate;
pub mod split;
pub mod watermark;

pub use extract::extract_pages;
pub use merge::PdfMerger;
pub use overlay::{OverlayPosition, OverlayType, PdfOverlay};
pub use rotate::rotate_page;
pub use split::PdfSplitter;
pub use watermark::{WatermarkConfig, add_watermark};
