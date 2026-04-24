use crate::{Document, PdfResult};
use crate::cos::{CosName, CosObject};

/// Rotates a page by the given number of degrees.
/// The `degrees` should be a multiple of 90.
/// 
/// Modifies the document directly.
pub fn rotate_page(doc: &mut Document, page_index: usize, degrees: i64) -> PdfResult<()> {
    let tree = doc.pages()?;
    let page = tree.get(page_index).ok_or_else(|| crate::PdfError::Parse {
        offset: None,
        context: format!("page index out of bounds: {}", page_index),
    })?;
    
    let current_rotation = page.rotation();
    let new_rotation = (current_rotation + degrees) % 360;
    let new_rotation = if new_rotation < 0 { new_rotation + 360 } else { new_rotation };
    
    let page_id = page.id;
    doc.mutate_object(page_id, |obj| {
        if let CosObject::Dictionary(dict) = obj {
            dict.insert(CosName::new(b"Rotate".to_vec()), CosObject::Integer(new_rotation));
        }
    });
    
    Ok(())
}

