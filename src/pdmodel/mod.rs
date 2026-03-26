//! High-level document model — pages, resources, and page-level metadata.
//!
//! Maps to the `org.apache.pdfbox.pdmodel` Java package.
//!
//! # Module layout
//!
//! | Sub-module | Contents | Java PDFBox equivalent |
//! |---|---|---|
//! | [`page`] | [`Page`], [`Rectangle`], [`Resources`] | `PDPage`, `PDRectangle`, `PDResources` |
//! | [`page_tree`] | [`PageTree`] | `PDPageTree`, `PDDocumentCatalog.getPages()` |

pub mod page;
pub mod page_tree;

pub use page::{Page, Rectangle, Resources, rectangle_from_cos};
pub use page_tree::PageTree;
