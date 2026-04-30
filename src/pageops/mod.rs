//!
//! Page Manipulation module.
//!
//! Maps to `org.apache.pdfbox.multipdf.*` in Java PDFBox.

pub mod rotate;
pub mod split;
pub mod extract;
pub mod merge;
pub mod overlay;
pub mod watermark;

pub use rotate::rotate_page;
pub use extract::extract_pages;
pub use split::PdfSplitter;
pub use merge::PdfMerger;
pub use overlay::{PdfOverlay, OverlayType, OverlayPosition};
pub use watermark::{add_watermark, WatermarkConfig};
