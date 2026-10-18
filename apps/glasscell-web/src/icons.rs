//! The toolbar's icons: original line drawings on a 16-unit grid, drawn in code (no files, no
//! vendor artwork). Each is the inside of an `<svg viewBox="0 0 16 16">` stroked with the text
//! colour.

pub const UNDO: &str = "M5 3.5 2.5 6 5 8.5M3 6h6.5a3.5 3.5 0 0 1 0 7H6.5";
pub const REDO: &str = "M11 3.5 13.5 6 11 8.5M13 6H6.5a3.5 3.5 0 0 0 0 7h3";
pub const ALIGN_LEFT: &str = "M2 3.5h12M2 6.5h8M2 9.5h12M2 12.5h8";
pub const ALIGN_CENTER: &str = "M2 3.5h12M4 6.5h8M2 9.5h12M4 12.5h8";
pub const ALIGN_RIGHT: &str = "M2 3.5h12M6 6.5h8M2 9.5h12M6 12.5h8";
pub const WRAP: &str = "M2 4h12M2 8h8.5a2 2 0 0 1 0 4H7.5M9 10.5 7.5 12 9 13.5";
pub const MERGE: &str = "M2 3.5v9M14 3.5v9M4.5 8h7M6.2 6.2 4.4 8l1.8 1.8M9.8 6.2 11.6 8 9.8 9.8";
pub const SUM: &str = "M12 3.5H4.5L8.5 8l-4 4.5H12";
pub const SORT_UP: &str = "M5 13V3M2.5 5.5 5 3l2.5 2.5M9.5 4h4M9.5 8h3M9.5 12h2";
pub const SORT_DOWN: &str = "M5 3v10M2.5 10.5 5 13l2.5-2.5M9.5 4h2M9.5 8h3M9.5 12h4";
pub const DEC_LESS: &str = "M2.5 11.5h1M6 4.5h2v5H6zM10.5 4.5h2v5h-2zM2 3.5l2 2-2 2";
pub const DEC_MORE: &str = "M2.5 11.5h1M6 4.5h2v5H6zM10.5 4.5h2v5h-2zM14 3.5l-2 2 2 2";
pub const INSPECTOR: &str = "M2.5 3.5h11v9h-11zM10 3.5v9";
pub const SEARCH: &str = "M7 11.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9ZM10.3 10.3 13.5 13.5";
pub const OPEN: &str = "M2.5 4.5h4l1.5 1.5h5.5v6.5h-11z";
pub const SAVE: &str = "M8 2.5v7M5 7l3 3 3-3M3 12.5h10";
