//! The view state saved with a file (`sheetView topLeftCell`) against a frontend's scroll
//! offset. Offsets are in sheet units, measured from the first unfrozen row and column. Normal
//! view only: Page Layout view scrolls over pages, not cells.

use gridcraft_core::CellRef;
use gridcraft_model::{Sheet, SheetViewMode};

/// The scroll offset `sh` opens with: its saved first visible cell (`(0, 0)` outside Normal view).
pub fn saved_scroll(sh: &Sheet) -> (f64, f64) {
    if sh.view != SheetViewMode::Normal {
        return (0.0, 0.0);
    }
    let (fr, fc) = sh.freeze.unwrap_or((0, 0));
    let top = sh.view_top_left;
    let y = sh.row_top(top.row.max(fr)) - sh.row_top(fr);
    let x = sh.col_left(top.col.max(fc)) - sh.col_left(fc);
    (x.max(0.0), y.max(0.0))
}

/// The first visible cell of the scrolling area at scroll offset `(x, y)`, to save with the file
/// ([`crate::DocState::store_view`]); `None` outside Normal view.
pub fn top_left_at(sh: &Sheet, x: f64, y: f64) -> Option<CellRef> {
    if sh.view != SheetViewMode::Normal {
        return None;
    }
    let (fr, fc) = sh.freeze.unwrap_or((0, 0));
    let finite = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
    // A hair past the edge so a row that starts exactly at the scroll offset is the one found.
    let row = sh.row_at(sh.row_top(fr) + finite(y) + 0.01);
    let col = sh.col_at(sh.col_left(fc) + finite(x) + 0.01);
    Some(CellRef::new(row, col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_and_top_left_cell_round_trip() {
        let mut sh = Sheet::new("S");
        sh.freeze = Some((2, 1));
        sh.view_top_left = CellRef::new(40, 7);
        let (x, y) = saved_scroll(&sh);
        assert_eq!(top_left_at(&sh, x, y), Some(CellRef::new(40, 7)));
        assert_eq!(top_left_at(&sh, f64::NAN, -5.0), Some(CellRef::new(2, 1)));
        sh.view = SheetViewMode::PageLayout;
        assert_eq!(saved_scroll(&sh), (0.0, 0.0));
        assert_eq!(top_left_at(&sh, x, y), None);
    }
}
