//! The GridCraft engine for Swift, through UniFFI.
//!
//! A coarse API over one [`Session`]: run a command (`id` + JSON params → JSON result), drain the
//! UI requests commands make, list the command catalog, and typed read models for what the Mac
//! frontend draws every frame: the viewport (cell appearance), the selection's format (toolbar
//! and inspector) and the grid geometry (positions and hit-testing).
//!
//! Every call catches panics, so none unwinds into Swift: a panic comes back as
//! [`FfiError::Internal`] (and a command's workbook is restored by `Session::execute`).
#![deny(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gridcraft_engine::Session;
use gridcraft_engine::cf::CfCache;
use gridcraft_engine::core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_engine::formula_locale::locale_param;
use gridcraft_engine::model::{self as model, Workbook};
use gridcraft_engine::viewport::{self as vp, Measure};
use serde_json::Value as Json;

mod entry;
mod forms;
mod objects;
mod records;

pub use entry::*;
pub use forms::*;
pub use objects::*;
pub use records::*;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    /// The engine refused (unknown command, bad parameters, no workbook…); `message` is for the user.
    #[error("{message}")]
    Engine { message: String },
    /// A bug in the engine; the workbook was kept as it was.
    #[error("internal error: {message}")]
    Internal { message: String },
}

fn engine_error(e: impl std::fmt::Display) -> FfiError {
    FfiError::Engine { message: e.to_string() }
}

/// Runs `f`, turning a panic into [`FfiError::Internal`].
fn guard<T>(f: impl FnOnce() -> Result<T, FfiError>) -> Result<T, FfiError> {
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let message =
                p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into());
            Err(FfiError::Internal { message })
        }
    }
}

/// Text measurement, implemented in Swift with Core Text.
#[uniffi::export(with_foreign)]
pub trait TextMeasurer: Send + Sync {
    /// Advance width of `text` on one line in `face` at `size` points (screen units).
    fn text_width(&self, text: String, face: FontFace, size: f32) -> f32;
}

/// The engine's [`Measure`] over a Swift [`TextMeasurer`], remembering widths within a request
/// (most cells ask for the width of "0" in the same font).
struct Measurer {
    swift: Arc<dyn TextMeasurer>,
    cache: Mutex<HashMap<(String, String, bool, bool, u32), f32>>,
}

impl Measure for Measurer {
    fn text_width(&self, text: &str, font: &model::Font, size: f32) -> f32 {
        let key = (text.to_string(), font.name.clone(), font.bold, font.italic, size.to_bits());
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(w) = cache.get(&key) {
            return *w;
        }
        let face = FontFace { name: font.name.clone(), bold: font.bold, italic: font.italic };
        let w = self.swift.text_width(text.to_string(), face, size);
        // Whatever Swift answers, the layout gets a usable width.
        let w = if w.is_finite() { w.clamp(0.0, 1.0e7) } else { 0.0 };
        if cache.len() < 100_000 {
            cache.insert(key, w);
        }
        w
    }
}

/// One GridCraft session (open workbooks, clipboard, history). Swift owns it from one actor.
#[derive(uniffi::Object)]
pub struct Engine {
    session: Mutex<Session>,
}

impl Engine {
    fn session(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The active workbook and its active sheet's index.
    fn workbook(&self) -> Result<(Arc<Workbook>, usize), FfiError> {
        let s = self.session();
        let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
        Ok((d.wb.clone(), d.wb.active_sheet))
    }
}

#[uniffi::export]
impl Engine {
    /// A session with no workbook open (`file.new` or `file.open` opens one).
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Engine { session: Mutex::new(Session::new()) })
    }

    /// Runs command `id` with JSON `params` (`""` for none) and returns its JSON result.
    /// Programmatic calls never open dialogs: with no params a command runs with its defaults,
    /// or asks for a dialog through [`Engine::take_ui_requests`].
    pub fn execute(&self, id: String, params: String) -> Result<String, FfiError> {
        guard(|| {
            let params: Json = if params.trim().is_empty() { Json::Null } else { serde_json::from_str(&params).map_err(engine_error)? };
            let r = self.session().execute(&id, params).map_err(engine_error)?;
            Ok(r.to_string())
        })
    }

    /// The commands run so far, replayable (passwords left out), as a JSON array of
    /// `{command, params}`.
    pub fn journal(&self) -> Result<String, FfiError> {
        guard(|| {
            let s = self.session();
            let list: Vec<Json> = s.journal.iter().map(|(id, p)| serde_json::json!({"command": id, "params": p})).collect();
            Ok(Json::Array(list).to_string())
        })
    }

    /// The UI requests commands made since the last call (dialogs, messages, edit mode, URLs).
    pub fn take_ui_requests(&self) -> Result<Vec<UiRequest>, FfiError> {
        guard(|| Ok(self.session().take_ui_requests().into_iter().map(UiRequest::from).collect()))
    }

    /// The command catalog, with each command's enabled state now.
    pub fn commands(&self) -> Result<Vec<CommandInfo>, FfiError> {
        guard(|| {
            let s = self.session();
            Ok(gridcraft_engine::command_specs().iter().map(|c| CommandInfo::from(c.info(&s))).collect())
        })
    }

    /// Changes whenever the active workbook does (edits, undo, opening another one), so cached
    /// drawing can be kept until it moves.
    pub fn revision(&self) -> Result<u64, FfiError> {
        guard(|| {
            let s = self.session();
            Ok(s.active().map_or(0, |d| d.uid.wrapping_mul(1 << 32) ^ d.revision ^ (Arc::as_ptr(&d.wb) as u64)))
        })
    }

    /// The active workbook's undo history: labels (most recent first), and a revision that moves
    /// whenever an undoable command changed the workbook.
    pub fn history(&self) -> Result<History, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            Ok(History {
                undo: d.undo.iter().rev().map(|e| e.label.clone()).collect(),
                redo: d.redo.iter().rev().map(|e| e.label.clone()).collect(),
                revision: d.revision,
                dirty: d.is_dirty(),
            })
        })
    }

    /// Where editing the active cell starts: the cell (a merged area's first) and its text in
    /// `locale`'s formula syntax; `None` for a picture cell (typing still replaces it).
    pub fn edit_start(&self, locale: String) -> Result<Option<EntryStart>, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let start = gridcraft_engine::entry::start(&d.wb, d.wb.active_sheet, d.selection.active, locale_param(Some(&locale)));
            Ok(start.map(|(cell, text)| EntryStart { cell: cell.into(), text }))
        })
    }

    /// The worksheet functions' names in `locale`, for autocomplete.
    pub fn function_names(&self, locale: String) -> Result<Vec<String>, FfiError> {
        guard(|| {
            let (wb, si) = self.workbook()?;
            Ok(gridcraft_engine::entry::function_names(&wb, si, locale_param(Some(&locale))))
        })
    }

    /// Column AutoComplete for typing `typed` into `cell`: the one entry of its column it completes to.
    pub fn column_completion(&self, cell: CellAddress, typed: String) -> Result<Option<String>, FfiError> {
        guard(|| {
            let (wb, si) = self.workbook()?;
            let sh = wb.sheet(si).ok_or_else(|| engine_error("no sheet"))?;
            Ok(gridcraft_engine::entry::column_completion(sh, CellRef::new(row_of(cell.row), col_of(cell.col)), &typed))
        })
    }

    /// The reference point mode inserts for `range` on the active sheet, editing a cell on sheet
    /// `editing_sheet` (sheet-qualified when they differ).
    pub fn point_reference(&self, range: CellRange, editing_sheet: u32) -> Result<String, FfiError> {
        guard(|| {
            let (wb, si) = self.workbook()?;
            Ok(gridcraft_engine::entry::point_reference(&wb, editing_sheet as usize, si, range.to_engine()))
        })
    }

    /// Double-clicking the fill handle: where the current area fills down to (as far as the
    /// column beside it has data), or `None`.
    pub fn fill_down_target(&self) -> Result<Option<CellRange>, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let sh = d.wb.active().ok_or_else(|| engine_error("no sheet"))?;
            Ok(gridcraft_engine::geometry::fill_down_target(sh, d.selection.current()).map(CellRange::from))
        })
    }

    /// The columns resizing (or AutoFitting) column `col` changes, as `home.columnWidth`'s `cols`.
    pub fn resize_columns(&self, col: u32) -> Result<String, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            Ok(gridcraft_engine::geometry::resize_columns(&d.selection, col_of(col)))
        })
    }

    /// The rows resizing (or AutoFitting) row `row` changes, as `home.rowHeight`'s `rows`.
    pub fn resize_rows(&self, row: u32) -> Result<String, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            Ok(gridcraft_engine::geometry::resize_rows(&d.selection, row_of(row)))
        })
    }

    /// The active sheet's selection.
    pub fn selection(&self) -> Result<SelectionState, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let sel = &d.selection;
            Ok(SelectionState { active: sel.active.into(), ranges: sel.ranges.iter().map(|r| CellRange::from(*r)).collect() })
        })
    }

    /// The active sheet's view settings.
    pub fn sheet(&self) -> Result<SheetInfo, FfiError> {
        guard(|| {
            let (wb, si) = self.workbook()?;
            let sh = wb.sheet(si).ok_or_else(|| engine_error("no sheet"))?;
            let (fr, fc) = sh.freeze.unwrap_or((0, 0));
            Ok(SheetInfo {
                index: si as u32,
                name: sh.name.clone(),
                sheet_names: wb.sheets.iter().map(|s| s.name.clone()).collect(),
                zoom: sh.zoom,
                show_gridlines: sh.show_gridlines,
                show_headings: sh.show_headings,
                frozen: CellAddress { row: fr, col: fc },
                right_to_left: sh.right_to_left,
                normal_view: sh.view == model::SheetViewMode::Normal,
                default_row_height: sh.default_row_height,
                default_col_width: sh.default_col_width,
                landscape: sh.print.orientation == model::Orientation::Landscape,
                print_gridlines: sh.print.gridlines,
                print_headings: sh.print.headings,
            })
        })
    }

    /// The status bar's Sum / Count / Average for the selection (none for a single cell).
    pub fn status_items(&self) -> Result<Vec<StatusItem>, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let Some(sh) = d.wb.active() else { return Ok(Vec::new()) };
            let items = gridcraft_engine::display::status_items(&d.wb, sh, &d.selection);
            Ok(items.into_iter().map(|(label, value)| StatusItem { label: label.to_string(), value }).collect())
        })
    }

    /// The workbook for Spotlight: labels, and up to `max_chars` characters of cell text.
    pub fn search_summary(&self, max_chars: u32) -> Result<SearchSummary, FfiError> {
        guard(|| {
            let (wb, _) = self.workbook()?;
            let cap = max_chars as usize;
            let mut text = String::new();
            let mut chars = 0usize;
            'sheets: for sh in &wb.sheets {
                let Some(used) = sh.used_range() else { continue };
                let mut row = None;
                for (c, _) in sh.cells.iter_range(used) {
                    let shown = gridcraft_engine::display::cell_text(&wb, sh, c);
                    let shown = shown.trim();
                    if shown.is_empty() {
                        continue;
                    }
                    let sep = match row {
                        None => "",
                        Some(r) if r == c.row => "\t",
                        Some(_) => "\n",
                    };
                    row = Some(c.row);
                    let n = sep.len() + shown.chars().count();
                    if chars.saturating_add(n) > cap {
                        break 'sheets;
                    }
                    text.push_str(sep);
                    text.push_str(shown);
                    chars += n;
                }
                if chars < cap && !text.is_empty() {
                    text.push('\n');
                    chars += 1;
                }
            }
            Ok(SearchSummary {
                sheets: wb.sheets.iter().map(|s| s.name.clone()).collect(),
                tables: wb.sheets.iter().flat_map(|s| s.tables.iter().map(|t| t.name.clone())).collect(),
                names: wb.names.iter().map(|n| n.name.clone()).collect(),
                text,
            })
        })
    }

    /// Lays out a block of cells of the active workbook for drawing.
    pub fn viewport(&self, request: ViewportQuery, measurer: Arc<dyn TextMeasurer>) -> Result<Viewport, FfiError> {
        guard(|| {
            let (wb, active) = self.workbook()?;
            let req = vp::ViewportRequest {
                sheet: request.sheet.map_or(active, |s| s as usize),
                rows: (request.rows.first, request.rows.last),
                cols: (request.cols.first, request.cols.last),
                scale: request.scale,
                formula_locale: gridcraft_engine::formula_locale::locale_param(Some(&request.formula_locale)),
            };
            let m = Measurer { swift: measurer, cache: Mutex::new(HashMap::new()) };
            Ok(Viewport::from(vp::viewport(&wb, &req, &mut CfCache::new(), &m)))
        })
    }

    /// The selection's formatting for the toolbar and the inspector (`None` = mixed).
    pub fn selection_format(&self) -> Result<SelectionFormat, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let f = gridcraft_engine::selection_format::selection_format(&d.wb, d.wb.active_sheet, &d.selection);
            Ok(SelectionFormat::new(&f, &d.wb))
        })
    }

    /// Geometry of the active sheet for a scroll offset (sheet units past the frozen panes), with
    /// the cells area at `origin` and `size` big, at `scale` screen units per sheet unit.
    pub fn geometry(&self, origin: GridPoint, size: GridSize, scroll: GridPoint, scale: f32) -> Result<Arc<GridGeometry>, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let si = d.wb.active_sheet;
            let sheet = d.wb.sheets.get(si).cloned().ok_or_else(|| engine_error("no sheet"))?;
            let (origin, size, scroll) = ((origin.x, origin.y), (size.width, size.height), (scroll.x, scroll.y));
            // Page Layout view lays the cells out on printed pages.
            let paged = (sheet.view == model::SheetViewMode::PageLayout)
                .then(|| gridcraft_engine::geometry::Geometry::paged(&d.wb, si, origin, size, scroll, scale))
                .flatten();
            let geo = paged.unwrap_or_else(|| gridcraft_engine::geometry::Geometry::new(&sheet, origin, size, scroll, scale));
            Ok(Arc::new(GridGeometry { sheet, selection: d.selection.clone(), geo }))
        })
    }

    /// The scroll offset the active sheet opens with (its saved first visible cell).
    pub fn saved_scroll(&self) -> Result<GridPoint, FfiError> {
        guard(|| {
            let (wb, si) = self.workbook()?;
            let (x, y) = wb.sheet(si).map(gridcraft_engine::view_state::saved_scroll).unwrap_or_default();
            Ok(GridPoint { x: x as f32, y: y as f32 })
        })
    }

    /// Records the active sheet's scroll offset, so a save writes its first visible cell.
    pub fn store_scroll(&self, scroll: GridPoint) -> Result<(), FfiError> {
        guard(|| {
            let mut s = self.session();
            let d = s.active_mut().ok_or_else(|| engine_error("no workbook open"))?;
            let si = d.wb.active_sheet;
            if let Some(top) = d.wb.sheet(si).and_then(|sh| gridcraft_engine::view_state::top_left_at(sh, scroll.x as f64, scroll.y as f64)) {
                d.store_view(&[(si, top)]);
            }
            Ok(())
        })
    }

    /// The Name Box text; `dragging` while a selection is being dragged out.
    pub fn name_box_text(&self, dragging: bool) -> Result<String, FfiError> {
        guard(|| {
            let s = self.session();
            Ok(s.active().map(|d| gridcraft_engine::selection::name_box_text(&d.wb, &d.selection, dragging)).unwrap_or_default())
        })
    }

    /// AutoSave may save the active workbook now (changed, with an XLSX file).
    pub fn autosave_due(&self) -> Result<bool, FfiError> {
        guard(|| Ok(self.session().active().is_some_and(gridcraft_engine::DocState::autosave_due)))
    }

    /// The info-bar notice for a `file.open` result (JSON), or none for a clean open.
    pub fn open_notice(&self, open_result: String) -> Result<Option<Notice>, FfiError> {
        guard(|| {
            let r: Json = serde_json::from_str(&open_result).map_err(engine_error)?;
            Ok(gridcraft_engine::notice::open_notice(&r).map(Notice::from))
        })
    }

    /// GridCraft put `text` on the system clipboard for a copy or a cut.
    pub fn clipboard_published(&self, cut: bool, text: String) -> Result<(), FfiError> {
        guard(|| {
            self.session().host_clipboard.published(cut, &text);
            Ok(())
        })
    }

    /// GridCraft copied or cut without touching the system clipboard, which holds `system`.
    pub fn clipboard_unpublished(&self, cut: bool, system: Option<String>) -> Result<(), FfiError> {
        guard(|| {
            self.session().host_clipboard.unpublished(cut, system.as_deref());
            Ok(())
        })
    }

    /// What Cmd+V with `text` from the system clipboard pastes (a cut pastes once).
    pub fn paste_for_text(&self, text: String) -> Result<PasteSource, FfiError> {
        guard(|| {
            let mut s = self.session();
            let internal = s.clipboard.is_some();
            Ok(s.host_clipboard.paste_text(text, internal).into())
        })
    }

    /// What the PasteSource command pastes, with `system` the system clipboard's text (if any).
    pub fn paste_for_command(&self, system: Option<String>) -> Result<PasteSource, FfiError> {
        guard(|| {
            let mut s = self.session();
            let internal = s.clipboard.is_some();
            Ok(s.host_clipboard.paste_command(system.as_deref(), internal).into())
        })
    }
}

/// Where a fill-handle drag of `source` to the cell under the pointer fills.
#[uniffi::export]
pub fn fill_target(source: CellRange, pointer: CellAddress) -> CellRange {
    gridcraft_engine::geometry::fill_target(source.to_engine(), CellRef::new(row_of(pointer.row), col_of(pointer.col))).into()
}

/// The label a row or column resize shows: `Width: 8.43 (64 pixels)`, `Height: 15.00 (20 pixels)`.
#[uniffi::export]
pub fn resize_label(columns: bool, size: f32) -> String {
    gridcraft_engine::geometry::resize_label(columns, size)
}

/// A column's header label (`A`, `Z`, `AA`…).
#[uniffi::export]
pub fn column_label(col: u32) -> String {
    gridcraft_engine::core::col_to_letters(col_of(col))
}

/// A cell's A1 reference (`B3`), for command parameters.
#[uniffi::export]
pub fn cell_name(cell: CellAddress) -> String {
    CellRef::new(row_of(cell.row), col_of(cell.col)).a1()
}

/// The cell an A1 reference names (`B3`, `$B$3`); `None` for anything else.
#[uniffi::export]
pub fn parse_cell(text: String) -> Option<CellAddress> {
    CellRef::parse(text.trim()).map(CellAddress::from)
}

/// A range's A1 reference (`B3:D7`, or `B3` for one cell), for command parameters.
#[uniffi::export]
pub fn range_name(range: CellRange) -> String {
    range.to_engine().a1()
}

/// The grid geometry for one scroll position (see [`Engine::geometry`]).
#[derive(uniffi::Object)]
pub struct GridGeometry {
    sheet: Arc<model::Sheet>,
    selection: gridcraft_engine::Selection,
    geo: gridcraft_engine::geometry::Geometry,
}

fn row_of(r: u32) -> u32 {
    r.min(MAX_ROWS - 1)
}

fn col_of(c: u32) -> u32 {
    c.min(MAX_COLS - 1)
}

impl GridGeometry {
    fn geometry(&self) -> &gridcraft_engine::geometry::Geometry {
        &self.geo
    }
}

#[uniffi::export]
impl GridGeometry {
    /// Screen units per sheet unit (in Page Layout view, times the print scale).
    pub fn scale(&self) -> f32 {
        self.geo.scale
    }
    /// Page Layout view: the visible pages (none in Normal view).
    pub fn pages(&self) -> Vec<PageRect> {
        self.geo.pages().into_iter().map(|p| PageRect { paper: p.paper.into(), content: p.content.into(), data: p.data }).collect()
    }
    /// Frozen rows and columns.
    pub fn frozen(&self) -> CellAddress {
        CellAddress { row: self.geo.frozen.0, col: self.geo.frozen.1 }
    }
    /// GridSize of the frozen columns (`width`) and rows (`height`).
    pub fn frozen_size(&self) -> GridSize {
        GridSize { width: self.geo.frozen_w, height: self.geo.frozen_h }
    }
    pub fn x(&self, col: u32) -> f32 {
        self.geo.x(col_of(col))
    }
    pub fn y(&self, row: u32) -> f32 {
        self.geo.y(row_of(row))
    }
    pub fn col_at(&self, x: f32) -> u32 {
        self.geo.col_at(x)
    }
    pub fn row_at(&self, y: f32) -> u32 {
        self.geo.row_at(y)
    }
    pub fn cell_rect(&self, cell: CellAddress) -> GridRect {
        self.geo.cell_rect(&self.sheet, CellRef::new(row_of(cell.row), col_of(cell.col))).into()
    }
    pub fn range_rect(&self, range: CellRange) -> GridRect {
        self.geo.range_rect(&self.sheet, range.to_engine()).into()
    }
    /// The panes to paint, the scrolling one first.
    pub fn panes(&self) -> Vec<GridPane> {
        self.geo.panes().into_iter().map(GridPane::from).collect()
    }
    /// The screen areas of the panes that hold part of `range` (clip a range's highlight to them).
    pub fn range_panes(&self, range: CellRange) -> Vec<GridRect> {
        self.geo.range_panes(range.to_engine()).into_iter().map(GridRect::from).collect()
    }
    /// The scroll offset that brings `cell` into view.
    pub fn reveal(&self, cell: CellAddress) -> GridPoint {
        let (x, y) = self.geo.reveal(&self.sheet, CellRef::new(row_of(cell.row), col_of(cell.col)));
        GridPoint { x, y }
    }
    /// The column whose right edge is at `x` (resizing from the column headers).
    pub fn col_edge(&self, x: f32) -> Option<LineEdge> {
        self.geo.col_edge(&self.sheet, x).map(|(index, at)| LineEdge { index, at })
    }
    /// The row whose bottom edge is at `y` (resizing from the row headers).
    pub fn row_edge(&self, y: f32) -> Option<LineEdge> {
        self.geo.row_edge(&self.sheet, y).map(|(index, at)| LineEdge { index, at })
    }
    pub fn fill_handle(&self) -> GridRect {
        self.geo.fill_handle(&self.sheet, &self.selection).into()
    }
    pub fn filter_button(&self, cell: CellAddress) -> GridRect {
        self.geo.filter_button(&self.sheet, CellRef::new(row_of(cell.row), col_of(cell.col))).into()
    }
    /// What a click at `(x, y)` over the cells is on.
    pub fn hit(&self, x: f32, y: f32) -> GridHit {
        self.geo.hit(&self.sheet, &self.selection, x, y).into()
    }
}

impl CellRange {
    fn to_engine(self) -> RangeRef {
        RangeRef::new(CellRef::new(row_of(self.first.row), col_of(self.first.col)), CellRef::new(row_of(self.last.row), col_of(self.last.col)))
    }
}

#[cfg(test)]
mod tests;
