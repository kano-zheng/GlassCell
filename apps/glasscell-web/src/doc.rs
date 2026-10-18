//! The document the page edits: one engine session, the scroll offset, and the layout rules the
//! Mac frontend uses (header sizes, scale), so both draw the same grid from the same read models.
//!
//! Everything here is plain Rust over the engine, so it runs (and is tested) natively; the
//! browser parts (canvas, DOM) live in the wasm-only modules.

use gridcraft_engine::cf::CfCache;
use gridcraft_engine::cmd::data::{self as data_cmd, FilterMenu};
use gridcraft_engine::core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_engine::forms::{self, Form};
use gridcraft_engine::formula::FormulaLocale;
use gridcraft_engine::geometry::{self, Geometry, Hit, Pane};
use gridcraft_engine::model::{Sheet, Workbook};
use gridcraft_engine::objects::{self, ObjectKind, ObjectView};
use gridcraft_engine::selection_format::{self, SelectionFormat};
use gridcraft_engine::viewport::{self, Measure, Viewport, ViewportRequest};
use gridcraft_engine::{Selection, Session, display, entry, selection};
use serde_json::{Value as Json, json};

/// CSS pixels per sheet unit at 100% before adapting to the user's environment: a default
/// 20-unit row is 20 px tall, as Excel draws it on a 96 dpi screen. (The Mac app's
/// `GridRenderer.displayScale` is 0.8, desktop point density; in a browser that was too small.)
pub const DISPLAY_SCALE: f32 = 1.0;

/// The display scale that suits the user's environment: their browser's default text size
/// (`root_font_px`, 16 unless they changed it), a touch screen (`coarse` pointer: bigger targets),
/// and the window's width (a phone gets a little less, a large display a little more).
pub fn adaptive_scale(root_font_px: f32, coarse: bool, width: f32) -> f32 {
    let text = if root_font_px.is_finite() && root_font_px > 0.0 { root_font_px / 16.0 } else { 1.0 };
    let touch = if coarse { 1.15 } else { 1.0 };
    let size = if width < 600.0 {
        0.9
    } else if width >= 2200.0 {
        1.15
    } else if width >= 1700.0 {
        1.08
    } else {
        1.0
    };
    ((DISPLAY_SCALE * text * touch * size).clamp(0.7, 2.0) * 20.0).round() / 20.0
}

/// Where the cells start and how big a sheet unit is, for a view of some size.
#[derive(Clone, Debug)]
pub struct Layout {
    pub header_w: f32,
    pub header_h: f32,
    pub scale: f32,
    pub headings: bool,
    pub geo: Geometry,
}

/// A column or row edge in a header, where a drag resizes and a double-click AutoFits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeEdge {
    pub columns: bool,
    /// The column or row whose far edge it is.
    pub index: u32,
    /// Its size now, in sheet units.
    pub size: f32,
}

/// The name `object.move` and `object.delete` give an object's kind.
pub fn kind_name(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Shape(_) => "shape",
        ObjectKind::Picture { .. } => "image",
        ObjectKind::Chart => "chart",
        ObjectKind::Slicer { .. } => "slicer",
    }
}

/// A floating object: its kind (`chart`, `image`, `shape`, `slicer`) and id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectRef {
    pub kind: &'static str,
    pub id: u32,
}

/// What Find and Replace looks for and how.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub what: String,
    pub with: String,
    pub match_case: bool,
    pub whole_cell: bool,
    /// Search the whole workbook, not just the sheet.
    pub workbook: bool,
}

impl FindOptions {
    fn params(&self, all: bool, replace: bool) -> Json {
        let mut p = json!({"what": self.what, "matchCase": self.match_case, "wholeCell": self.whole_cell, "within": if self.workbook { "workbook" } else { "sheet" }, "all": all});
        if replace {
            p["with"] = json!(self.with);
        }
        p
    }
}

/// A cell Find All found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub sheet: String,
    pub cell: String,
    pub value: String,
}

/// A defined name, as the Name Manager lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRow {
    pub name: String,
    pub refers_to: String,
    /// `Workbook`, or the sheet's name.
    pub scope: String,
    pub comment: String,
}

/// A worksheet function, as Insert Function lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionRow {
    pub name: String,
    pub signature: String,
    pub description: String,
}

/// What Goal Seek found.
#[derive(Clone, Debug, PartialEq)]
pub struct GoalOutcome {
    pub converged: bool,
    pub value: f64,
}

/// A conditional-format rule, as the rules manager lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRow {
    /// `Cell Is`, `Data Bar`…
    pub kind: String,
    pub ranges: String,
}

/// The dialogs that report a query's result: (dialog name, title, command).
pub const RESULT_DIALOGS: &[(&str, &str, &str)] = &[
    ("statistics", "Workbook Statistics", "review.workbookStatistics"),
    ("accessibility", "Accessibility", "review.checkAccessibility"),
    ("errorChecking", "Error Checking", "formulas.errorChecking"),
    ("evaluateFormula", "Evaluate Formula", "formulas.evaluateFormula"),
    ("spelling", "Spelling", "review.spelling"),
    ("macros", "Macros", "view.macros"),
];

/// A JSON value as indented `key: value` lines, keys in order, empty lists as `(none)`.
pub fn outline(value: &Json) -> Vec<String> {
    fn walk(v: &Json, indent: usize, out: &mut Vec<String>) {
        let pad = "  ".repeat(indent);
        match v {
            Json::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                for k in keys {
                    match &map[k] {
                        child @ (Json::Object(_) | Json::Array(_)) => {
                            out.push(format!("{pad}{k}:"));
                            walk(child, indent + 1, out);
                        }
                        Json::String(t) => out.push(format!("{pad}{k}: {t}")),
                        other => out.push(format!("{pad}{k}: {other}")),
                    }
                }
            }
            Json::Array(list) if list.is_empty() => out.push(format!("{pad}(none)")),
            Json::Array(list) => list.iter().for_each(|x| walk(x, indent, out)),
            Json::String(t) => out.push(format!("{pad}{t}")),
            other => out.push(format!("{pad}{other}")),
        }
    }
    let mut out = Vec::new();
    walk(value, 0, &mut out);
    out
}

/// `CellIs` → `Cell Is`.
fn spaced(name: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if c.is_uppercase() && previous_lower {
            out.push(' ');
        }
        previous_lower = c.is_lowercase();
        out.push(c);
    }
    out
}

/// The column letters of `range`, to sort by (at most 200 of them).
pub fn sort_columns(range: RangeRef) -> Vec<String> {
    (range.start.col..=range.end.col.min(range.start.col.saturating_add(200))).map(gridcraft_engine::core::col_to_letters).collect()
}

/// A workbook open in the page.
pub struct Doc {
    pub session: Session,
    /// Sheet units scrolled past the frozen panes.
    pub scroll: (f32, f32),
    /// Pixels per sheet unit at 100% zoom, adapted to the environment.
    pub display_scale: f32,
}

impl Default for Doc {
    fn default() -> Self {
        Doc::new()
    }
}

impl Doc {
    pub fn new() -> Doc {
        Doc { session: Session::new(), scroll: (0.0, 0.0), display_scale: DISPLAY_SCALE }
    }

    /// A document holding the named sample (`sales`, `budget`, `grades`).
    pub fn sample(name: &str) -> Doc {
        let mut doc = Doc::new();
        if doc.run("file.new", json!({"sample": name})).is_err() {
            let _ = doc.run("file.new", json!({}));
        }
        doc
    }

    /// Runs a command; the UI acts only through this.
    pub fn run(&mut self, id: &str, params: Json) -> Result<Json, String> {
        self.session.run(id, params)
    }

    /// Opens workbook bytes (xlsx, csv, …) through `file.open`.
    pub fn open_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<Json, String> {
        let r = self.run("file.open", json!({"name": name, "base64": base64(bytes)}))?;
        self.scroll = (0.0, 0.0);
        Ok(r)
    }

    pub fn workbook(&self) -> Option<&Workbook> {
        self.session.active().map(|d| &*d.wb)
    }

    pub fn sheet(&self) -> Option<&Sheet> {
        self.workbook().and_then(Workbook::active)
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.session.active().map(|d| &d.selection)
    }

    pub fn title(&self) -> String {
        self.session.active().map(|d| d.display_title()).unwrap_or_default()
    }

    /// What Undo and Redo would undo and redo: the labels of the last steps (`None`: nothing).
    pub fn history(&self) -> (Option<String>, Option<String>) {
        match self.session.active() {
            Some(d) => (d.undo.last().map(|h| h.label.clone()), d.redo.last().map(|h| h.label.clone())),
            None => (None, None),
        }
    }

    /// Whether the active cell holds a checkbox that Space toggles.
    pub fn space_toggles(&self) -> bool {
        self.session.commands().iter().any(|c| c.id == "cell.toggleCheckbox" && c.enabled)
    }

    /// Home (the first column of the row) or, with `ctrl`, the top-left cell.
    pub fn home(&mut self, ctrl: bool, extend: bool) -> Result<(), String> {
        let Some(active) = self.selection().map(|s| s.active) else { return Ok(()) };
        self.select(if ctrl { CellRef::new(0, 0) } else { CellRef::new(active.row, 0) }, extend)
    }

    /// Ctrl+End: the last cell with content.
    pub fn end(&mut self) -> Result<(), String> {
        self.run("edit.goToSpecial", json!({"kind": "lastCell"})).map(|_| ())
    }

    /// How many rows a Page Up or Page Down moves: the rows in view, less one.
    pub fn page_rows(&self, size: (f32, f32)) -> i64 {
        self.layout(size).map_or(20, |l| {
            let (first, last) = l.geo.scroll_rows();
            (i64::from(last) - i64::from(first)).max(1)
        })
    }

    /// `(name, is_active)` of every sheet.
    pub fn sheet_tabs(&self) -> Vec<(String, bool)> {
        let Some(wb) = self.workbook() else { return Vec::new() };
        wb.sheets.iter().enumerate().map(|(i, s)| (s.name.clone(), i == wb.active_sheet)).collect()
    }

    /// The formatting of the selection (the toolbar's and the inspector's state).
    pub fn selection_format(&self) -> Option<SelectionFormat> {
        let d = self.session.active()?;
        Some(selection_format::selection_format(&d.wb, d.wb.active_sheet, &d.selection))
    }

    pub fn name_box_text(&self) -> String {
        match (self.workbook(), self.selection()) {
            (Some(wb), Some(sel)) => selection::name_box_text(wb, sel, false),
            _ => String::new(),
        }
    }

    /// The active cell's text as the formula bar and the editor start with it.
    pub fn entry_text(&self) -> String {
        let (Some(wb), Some(sel)) = (self.workbook(), self.selection()) else { return String::new() };
        entry::start(wb, wb.active_sheet, sel.active, FormulaLocale::En).map(|(_, t)| t).unwrap_or_default()
    }

    /// Sum / Count / Average of a multi-cell selection.
    pub fn status_items(&self) -> Vec<(&'static str, String)> {
        match (self.workbook(), self.sheet(), self.selection()) {
            (Some(wb), Some(sh), Some(sel)) => display::status_items(wb, sh, sel),
            _ => Vec::new(),
        }
    }

    /// The layout for a view of `size` points.
    pub fn layout(&self, size: (f32, f32)) -> Option<Layout> {
        let sh = self.sheet()?;
        let zoom = (f32::from(sh.zoom) / 100.0).clamp(0.1, 4.0);
        let scale = zoom * self.display_scale;
        let chrome = zoom.max(0.75);
        let headings = sh.show_headings;
        let header_h = if headings { (21.0 * chrome).round() } else { 0.0 };
        let build =
            |header_w: f32| Geometry::new(sh, (header_w, header_h), ((size.0 - header_w).max(0.0), (size.1 - header_h).max(0.0)), self.scroll, scale);
        let mut header_w = if headings { ((2.0 * 7.0 + 14.0) * chrome).round() } else { 0.0 };
        let mut geo = build(header_w);
        if headings {
            // As wide as the bottom row's number needs, and every row the scroll bar reaches:
            // the width holds while scrolling (the engine's rule).
            let digits = geometry::row_number_digits(sh, geo.row_at(size.1)) as f32;
            let width = ((digits * 7.0 + 14.0) * chrome).round();
            if width != header_w {
                header_w = width;
                geo = build(header_w);
            }
        }
        Some(Layout { header_w, header_h, scale, headings, geo })
    }

    /// Each pane's cells laid out for drawing.
    pub fn viewports(&self, layout: &Layout, m: &dyn Measure) -> Vec<(Pane, Viewport)> {
        let Some(wb) = self.workbook() else { return Vec::new() };
        let mut cf = CfCache::new();
        layout
            .geo
            .panes()
            .into_iter()
            .map(|pane| {
                let req = ViewportRequest {
                    sheet: wb.active_sheet,
                    rows: pane.rows,
                    cols: pane.cols,
                    scale: layout.scale,
                    formula_locale: FormulaLocale::En,
                };
                (pane, viewport::viewport(wb, &req, &mut cf, m))
            })
            .collect()
    }

    /// Scrolls by a wheel delta in points, within what the sheet reaches.
    pub fn scroll_by(&mut self, size: (f32, f32), dx: f32, dy: f32) {
        let Some(layout) = self.layout(size) else { return };
        let Some(sh) = self.sheet() else { return };
        let max = layout.geo.scroll_extent(sh);
        let next = ((self.scroll.0 + dx / layout.scale).clamp(0.0, max.0), (self.scroll.1 + dy / layout.scale).clamp(0.0, max.1));
        self.scroll = next;
    }

    /// Scrolls the active cell into view.
    pub fn reveal_active(&mut self, size: (f32, f32)) {
        let (Some(layout), Some(sh), Some(sel)) = (self.layout(size), self.sheet(), self.selection()) else { return };
        self.scroll = layout.geo.reveal(sh, sel.active);
    }

    /// What a click at `(x, y)` (view points) lands on: a cell, or `None` over the headers.
    pub fn cell_at(&self, layout: &Layout, x: f32, y: f32) -> Option<CellRef> {
        (x >= layout.header_w && y >= layout.header_h).then(|| layout.geo.cell_at(x, y))
    }

    /// Selects `cell`, or with `extend` the area from the anchor to it.
    pub fn select(&mut self, cell: CellRef, extend: bool) -> Result<(), String> {
        let params = match (extend, self.selection()) {
            // The active cell is the anchor: a drag to the left or up makes the range start at the
            // far end, which is not where it began.
            (true, Some(sel)) => json!({"range": RangeRef::new(sel.active, cell).a1(), "active": sel.active.a1()}),
            _ => json!({"cell": cell.a1()}),
        };
        self.run("selection.set", params).map(|_| ())
    }

    /// The header edge under `(x, y)`, if any.
    pub fn resize_edge(&self, layout: &Layout, x: f32, y: f32) -> Option<ResizeEdge> {
        let sh = self.sheet()?;
        if y < layout.header_h && x >= layout.header_w {
            let (index, _) = layout.geo.col_edge(sh, x)?;
            Some(ResizeEdge { columns: true, index, size: sh.col_width(index) })
        } else if x < layout.header_w && y >= layout.header_h {
            let (index, _) = layout.geo.row_edge(sh, y)?;
            Some(ResizeEdge { columns: false, index, size: sh.row_height(index) })
        } else {
            None
        }
    }

    /// Sets the edge's column or row to `size` sheet units: inside a selection of whole columns
    /// (or rows) every selected one changes.
    pub fn resize(&mut self, edge: ResizeEdge, size: f32) -> Result<(), String> {
        let sel = self.selection().cloned().unwrap_or_default();
        let size = if size.is_finite() { size.max(0.0).round() } else { 0.0 };
        if edge.columns {
            self.run("home.columnWidth", json!({"cols": geometry::resize_columns(&sel, edge.index), "width": size}))
        } else {
            self.run("home.rowHeight", json!({"rows": geometry::resize_rows(&sel, edge.index), "height": size}))
        }
        .map(|_| ())
    }

    /// AutoFits the edge's column or row (and the selected whole ones with it).
    pub fn autofit(&mut self, edge: ResizeEdge) -> Result<(), String> {
        let sel = self.selection().cloned().unwrap_or_default();
        if edge.columns {
            self.run("home.autofitColumnWidth", json!({"cols": geometry::resize_columns(&sel, edge.index)}))
        } else {
            self.run("home.autofitRowHeight", json!({"rows": geometry::resize_rows(&sel, edge.index)}))
        }
        .map(|_| ())
    }

    /// What a press at `(x, y)` lands on over the cells: an object, the fill handle, a cell…
    pub fn hit(&self, layout: &Layout, x: f32, y: f32) -> Option<Hit> {
        if x < layout.header_w || y < layout.header_h {
            return None;
        }
        Some(layout.geo.hit(self.sheet()?, self.selection()?, x, y))
    }

    /// The dropdown of the filter button on header cell `cell`.
    pub fn filter_menu(&self, cell: CellRef) -> Option<FilterMenu> {
        let wb = self.workbook()?;
        data_cmd::filter_menu(wb, wb.active_sheet, cell)
    }

    /// Where the filter button of header cell `cell` is.
    pub fn filter_button_rect(&self, layout: &Layout, cell: CellRef) -> Option<geometry::Rect> {
        Some(layout.geo.filter_button(self.sheet()?, cell))
    }

    /// Shows only `showing` of the values the menu lists, in column `column`: every value clears
    /// the column's filter (none would hide every row, so it does nothing).
    pub fn apply_filter(&mut self, menu: &FilterMenu, column: u32, showing: &[String]) -> Result<(), String> {
        if showing.is_empty() {
            return Ok(());
        }
        let letters = gridcraft_engine::core::col_to_letters(column);
        let params = if menu.values.iter().all(|(v, _)| showing.contains(v)) {
            json!({"column": letters, "clear": true, "range": menu.range.a1()})
        } else {
            let values: Vec<&String> = showing.iter().filter(|v| v.as_str() != "(Blanks)").collect();
            json!({"column": letters, "values": values, "blanks": showing.iter().any(|v| v == "(Blanks)"), "range": menu.range.a1()})
        };
        self.run("data.filterBy", params).map(|_| ())
    }

    /// Sorts the column of a filter dropdown.
    pub fn sort_column(&mut self, column: u32, ascending: bool) -> Result<(), String> {
        let letters = gridcraft_engine::core::col_to_letters(column);
        self.run(if ascending { "data.sortAscending" } else { "data.sortDescending" }, json!({"column": letters})).map(|_| ())
    }

    /// Moves the cells of `from` to `to` (dragging the selection's border): one undo step.
    pub fn move_cells(&mut self, from: RangeRef, to: CellRef) -> Result<(), String> {
        if from.start == to {
            return Ok(());
        }
        self.run("edit.moveCells", json!({"from": from.a1(), "to": to.a1()})).map(|_| ())
    }

    /// The engine's form for dialog `name` (Row Height, Rename Sheet…), starting from the
    /// workbook's state; `None` when `name` isn't a plain form.
    pub fn open_form(&mut self, name: &str, params: &Json) -> Option<Form> {
        forms::open(&mut self.session, name, params)
    }

    /// OK on a form: runs its command with the values. The engine's refusal comes back as the
    /// error, for the dialog to show.
    pub fn submit_form(&mut self, form: &Form) -> Result<(), String> {
        self.run(form.command, forms::build_params(&form.name, &form.values)).map(|_| ())
    }

    /// Selects the next cell that matches, after the active one.
    pub fn find_next(&mut self, o: &FindOptions) -> Result<String, String> {
        let r = self.run("edit.find", o.params(false, false))?;
        Ok(r.get("cell").and_then(Json::as_str).unwrap_or_default().to_string())
    }

    /// Every cell that matches.
    pub fn find_all(&mut self, o: &FindOptions) -> Result<Vec<Found>, String> {
        let r = self.run("edit.find", o.params(true, false))?;
        let text = |v: &Json, k: &str| v.get(k).map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string)).unwrap_or_default();
        Ok(r.get("results")
            .and_then(Json::as_array)
            .map(|a| a.iter().map(|v| Found { sheet: text(v, "sheet"), cell: text(v, "cell"), value: text(v, "value") }).collect())
            .unwrap_or_default())
    }

    /// Replaces in the active cell if it matches, then finds the next one.
    pub fn replace_one(&mut self, o: &FindOptions) -> Result<(), String> {
        self.run("edit.replace", o.params(false, true))?;
        // After the last one there is nothing to find: that is not a failure of the replace.
        let _ = self.find_next(o);
        Ok(())
    }

    /// Replaces every match; returns how many.
    pub fn replace_all(&mut self, o: &FindOptions) -> Result<usize, String> {
        let r = self.run("edit.replace", o.params(true, true))?;
        Ok(r.get("replaced").and_then(Json::as_u64).unwrap_or(0) as usize)
    }

    /// Goes to a cell Find All listed.
    pub fn show_found(&mut self, f: &Found) -> Result<(), String> {
        if !f.sheet.is_empty() {
            self.run("sheet.activate", json!({"sheet": f.sheet}))?;
        }
        self.run("selection.set", json!({"cell": f.cell})).map(|_| ())
    }

    /// The defined names, for Go To's list.
    pub fn names(&mut self) -> Vec<String> {
        match self.run("formulas.nameManager", json!({})) {
            Ok(Json::Array(list)) => list.iter().filter_map(|n| n.get("name").and_then(Json::as_str).map(str::to_string)).collect(),
            _ => Vec::new(),
        }
    }

    /// Go To: a reference (`B5`, `Sheet2!A1:C3`) or a defined name.
    pub fn go_to(&mut self, reference: &str) -> Result<(), String> {
        self.run("edit.goTo", json!({"reference": reference})).map(|_| ())
    }

    /// Sorts `range` by `keys` (column letter, descending), the first the main key.
    pub fn sort(&mut self, range: RangeRef, header: bool, keys: &[(String, bool)]) -> Result<(), String> {
        let keys: Vec<Json> = keys.iter().map(|(column, desc)| json!({"column": column, "order": if *desc { "desc" } else { "asc" }})).collect();
        self.run("data.sort", json!({"range": range.a1(), "header": header, "keys": keys})).map(|_| ())
    }

    /// The defined names the Name Manager lists.
    pub fn name_rows(&mut self) -> Vec<NameRow> {
        let text = |v: &Json, k: &str| v.get(k).and_then(Json::as_str).unwrap_or_default().to_string();
        match self.run("formulas.nameManager", json!({})) {
            Ok(Json::Array(list)) => list
                .iter()
                .map(|n| NameRow { name: text(n, "name"), refers_to: text(n, "refersTo"), scope: text(n, "scope"), comment: text(n, "comment") })
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn delete_name(&mut self, n: &NameRow) -> Result<(), String> {
        self.run("formulas.deleteName", json!({"name": n.name, "scope": n.scope})).map(|_| ())
    }

    /// The parameters the Define Name form starts from to edit `n`.
    pub fn edit_name_params(n: &NameRow) -> Json {
        json!({"oldName": n.name, "oldScope": n.scope, "name": n.name, "scope": n.scope, "refersTo": n.refers_to, "comment": n.comment})
    }

    /// Every worksheet function, with its signature and what it does.
    pub fn functions(&mut self) -> Vec<FunctionRow> {
        let text = |v: &Json, k: &str| v.get(k).and_then(Json::as_str).unwrap_or_default().to_string();
        let mut rows: Vec<FunctionRow> = match self.run("formulas.functions", json!({})) {
            Ok(Json::Array(list)) => list
                .iter()
                .map(|f| FunctionRow { name: text(f, "name"), signature: text(f, "signature"), description: text(f, "description") })
                .collect(),
            _ => Vec::new(),
        };
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        rows.dedup_by(|a, b| a.name == b.name);
        rows
    }

    /// Goal Seek: changes `changing` until the formula in `set` gives `to`.
    pub fn goal_seek(&mut self, set: &str, to: f64, changing: &str) -> Result<GoalOutcome, String> {
        let r = self.run("data.goalSeek", json!({"set": set, "to": to, "changing": changing}))?;
        Ok(GoalOutcome {
            converged: r.get("converged").and_then(Json::as_bool).unwrap_or(false),
            value: r.get("value").and_then(Json::as_f64).unwrap_or(f64::NAN),
        })
    }

    /// The conditional-format rules of the active sheet, in priority order.
    pub fn rules(&mut self) -> Vec<RuleRow> {
        let Ok(r) = self.run("home.manageRules", json!({})) else { return Vec::new() };
        r.get("rules")
            .and_then(Json::as_array)
            .map(|list| {
                list.iter()
                    .map(|rule| {
                        let kind = match rule.get("rule") {
                            Some(Json::Object(m)) => m.keys().next().cloned().unwrap_or_default(),
                            Some(Json::String(s)) => s.clone(),
                            _ => String::new(),
                        };
                        let ranges = rule
                            .get("ranges")
                            .and_then(Json::as_array)
                            .map(|a| a.iter().filter_map(Json::as_str).collect::<Vec<_>>().join(", "))
                            .unwrap_or_default();
                        RuleRow { kind: spaced(&kind), ranges }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Deletes rule `i`, or moves it up or down (`action`: `delete`, `moveUp`, `moveDown`).
    pub fn rule_action(&mut self, action: &str, i: usize) -> Result<(), String> {
        self.run("home.manageRules", json!({ action: i })).map(|_| ())
    }

    /// What a reporting dialog shows: the query's result as outline lines.
    pub fn report(&mut self, command: &str) -> Result<Vec<String>, String> {
        self.run(command, json!({})).map(|v| outline(&v))
    }

    /// The object `r`, as the engine lists it now.
    pub fn object(&self, r: ObjectRef) -> Option<ObjectView> {
        self.objects().into_iter().find(|o| o.id == r.id && kind_name(&o.kind) == r.kind)
    }

    /// Moves an object by a drag of `(dx, dy)` view points: to the cell under its new corner.
    pub fn move_object(&mut self, r: ObjectRef, dx: f32, dy: f32, scale: f32) -> Result<(), String> {
        let (Some(o), Some(sh)) = (self.object(r), self.sheet()) else { return Err("That object is gone.".into()) };
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let (cell, ox, oy) = objects::moved_anchor(sh, &o.anchor, dx / scale, dy / scale);
        self.run("object.move", json!({"kind": r.kind, "id": r.id, "at": cell.a1(), "dx": ox, "dy": oy})).map(|_| ())
    }

    /// Gives an object a new size, in sheet units.
    pub fn resize_object(&mut self, r: ObjectRef, width: f32, height: f32) -> Result<(), String> {
        self.run("object.move", json!({"kind": r.kind, "id": r.id, "width": width.max(20.0), "height": height.max(20.0)})).map(|_| ())
    }

    pub fn delete_object(&mut self, r: ObjectRef) -> Result<(), String> {
        self.run("object.delete", json!({"kind": r.kind, "id": r.id})).map(|_| ())
    }

    /// Fills `source` out to `target` (a fill-handle drag), then selects the result. `alternate`
    /// swaps copying and extending a series (⌘ in Excel).
    pub fn fill(&mut self, source: RangeRef, target: RangeRef, alternate: bool) -> Result<(), String> {
        if source == target {
            return Ok(());
        }
        let mode = if alternate { "alternate" } else { "series" };
        self.run("edit.autoFill", json!({"source": source.a1(), "target": target.a1(), "mode": mode}))?;
        self.run("selection.set", json!({"range": target.a1()})).map(|_| ())
    }

    /// A double-click on the fill handle: fills down as far as the column beside has data.
    pub fn fill_down(&mut self) -> Result<(), String> {
        let (Some(sh), Some(sel)) = (self.sheet(), self.selection()) else { return Ok(()) };
        let source = sel.current();
        match geometry::fill_down_target(sh, source) {
            Some(target) => self.run("edit.autoFill", json!({"source": source.a1(), "target": target.a1()})).map(|_| ()),
            None => Ok(()),
        }
    }

    /// The active sheet's floating objects, bottom first.
    pub fn objects(&self) -> Vec<ObjectView> {
        self.workbook().map(|wb| objects::objects(wb, wb.active_sheet)).unwrap_or_default()
    }

    /// A chart drawn at its anchor's size, as primitives from its top-left corner.
    pub fn chart_prims(&self, id: u32, m: &dyn gridcraft_chart::Measure) -> Option<Vec<gridcraft_chart::Prim>> {
        let wb = self.workbook()?;
        objects::chart_prims(wb, wb.active_sheet, id, m)
    }

    /// A picture's bytes.
    pub fn picture(&self, id: u32) -> Option<Vec<u8>> {
        let wb = self.workbook()?;
        objects::picture(wb, wb.active_sheet, id).map(<[u8]>::to_vec)
    }

    /// Enters `input` in the active cell as the interface's syntax (`edit.commit`: validation,
    /// canonical formulas, undo). Returns the engine's message when it refused.
    pub fn commit(&mut self, input: &str) -> Result<(), String> {
        let r = self.run("edit.commit", json!({"input": input, "locale": "en"}))?;
        if r.get("committed").and_then(Json::as_bool) == Some(false) {
            let message = r.pointer("/validation/message").and_then(Json::as_str).unwrap_or("That entry isn't allowed here.");
            return Err(message.to_string());
        }
        Ok(())
    }

    /// Copies (or cuts) the selection: GridCraft's own clipboard keeps the formatting, and the
    /// returned text is what goes to the system clipboard.
    pub fn copy(&mut self, cut: bool) -> Result<String, String> {
        let r = self.run(if cut { "edit.cut" } else { "edit.copy" }, json!({}))?;
        let text = r.get("text").and_then(Json::as_str).unwrap_or_default().to_string();
        self.session.host_clipboard.published(cut, &text);
        Ok(text)
    }

    /// Pastes what the system clipboard held (`text`), or GridCraft's own copy when it is newer.
    pub fn paste(&mut self, text: Option<&str>) -> Result<(), String> {
        let internal = self.session.clipboard.is_some();
        let what = match text {
            Some(t) => self.session.host_clipboard.paste_command(Some(t), internal),
            None => gridcraft_engine::host_clipboard::Paste::Internal,
        };
        let params = match what {
            gridcraft_engine::host_clipboard::Paste::Internal => json!({}),
            gridcraft_engine::host_clipboard::Paste::Text(t) => json!({"text": t}),
        };
        self.run("edit.paste", params).map(|_| ())
    }

    /// Moves the selection after an Enter or Tab: down/right of a lone cell, or to the next cell
    /// inside a selection.
    pub fn advance(&mut self, by_row: bool, backwards: bool) -> Result<(), String> {
        let single = self.selection().is_none_or(Selection::is_single_cell);
        let params = if single {
            let step = if backwards { -1 } else { 1 };
            let (dr, dc) = if by_row { (0, step) } else { (step, 0) };
            json!({"dr": dr, "dc": dc})
        } else {
            json!({"forward": !backwards, "byRow": by_row})
        };
        self.run(if single { "selection.move" } else { "selection.next" }, params).map(|_| ())
    }
}

/// `range` moved by `dr` rows and `dc` columns, kept on the sheet.
pub fn moved_range(range: RangeRef, dr: i64, dc: i64) -> RangeRef {
    let (h, w) = (i64::from(range.height()) - 1, i64::from(range.width()) - 1);
    let row = (i64::from(range.start.row) + dr).clamp(0, i64::from(MAX_ROWS) - 1 - h);
    let col = (i64::from(range.start.col) + dc).clamp(0, i64::from(MAX_COLS) - 1 - w);
    RangeRef::new(CellRef::new(row as u32, col as u32), CellRef::new((row + h) as u32, (col + w) as u32))
}

/// Standard base64 (what `file.open` takes), without a dependency.
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The bytes of standard base64 text (what `file.saveBytes` returns); `None` when it isn't.
pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Column letters for a header.
pub fn column_label(col: u32) -> String {
    gridcraft_engine::core::col_to_letters(col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::model::Font;

    /// Half an em per character: enough for layout decisions in tests.
    struct Fixed;
    impl Measure for Fixed {
        fn text_width(&self, text: &str, _: &Font, size: f32) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    const VIEW: (f32, f32) = (900.0, 600.0);

    #[test]
    fn the_scale_adapts_to_the_environment() {
        assert_eq!(adaptive_scale(16.0, false, 1280.0), 1.0);
        assert_eq!(adaptive_scale(20.0, false, 1280.0), 1.25, "a larger default text size");
        assert_eq!(adaptive_scale(16.0, true, 1280.0), 1.15, "touch");
        assert!(adaptive_scale(16.0, false, 390.0) < 1.0, "a phone");
        assert!(adaptive_scale(16.0, false, 2560.0) > 1.0, "a large display");
        assert_eq!(adaptive_scale(f32::NAN, false, 1280.0), 1.0);
        assert!(adaptive_scale(1.0e6, true, 3000.0) <= 2.0);
    }

    #[test]
    fn a_larger_scale_makes_larger_rows() {
        let mut doc = Doc::sample("sales");
        let small = doc.layout(VIEW).expect("layout").geo.y(10);
        doc.display_scale = 1.5;
        assert!(doc.layout(VIEW).expect("layout").geo.y(10) > small * 1.4);
    }

    #[test]
    fn dragging_a_header_edge_resizes_and_a_double_click_fits() {
        let mut doc = Doc::sample("sales");
        let layout = doc.layout(VIEW).expect("layout");
        let sh = doc.sheet().expect("sheet");
        // The right edge of column B, in the column header.
        let edge_x = layout.geo.x(1) + sh.col_width(1) * layout.scale;
        let edge = doc.resize_edge(&layout, edge_x, layout.header_h / 2.0).expect("a column edge");
        assert!(edge.columns && edge.index == 1);
        doc.resize(edge, 150.0).expect("resize");
        assert_eq!(doc.sheet().expect("sheet").col_width(1), 150.0);
        doc.autofit(edge).expect("autofit");
        assert!(doc.sheet().expect("sheet").col_width(1) != 150.0, "AutoFit chose its own width");
        // A row edge, in the row header.
        let layout = doc.layout(VIEW).expect("layout");
        let sh = doc.sheet().expect("sheet");
        let edge_y = layout.geo.y(2) + sh.row_height(2) * layout.scale;
        let edge = doc.resize_edge(&layout, layout.header_w / 2.0, edge_y).expect("a row edge");
        assert!(!edge.columns && edge.index == 2);
        doc.resize(edge, 40.0).expect("resize");
        assert_eq!(doc.sheet().expect("sheet").row_height(2), 40.0);
        doc.run("edit.undo", json!({})).expect("undo");
        assert!(doc.sheet().expect("sheet").row_height(2) != 40.0);
        // Away from the edges there is nothing to grab.
        assert_eq!(doc.resize_edge(&layout, 300.0, 300.0), None);
    }

    #[test]
    fn resizing_inside_selected_columns_changes_them_all() {
        let mut doc = Doc::sample("sales");
        doc.run("selection.set", json!({"range": "B:D"})).expect("select");
        let layout = doc.layout(VIEW).expect("layout");
        let sh = doc.sheet().expect("sheet");
        let x = layout.geo.x(2) + sh.col_width(2) * layout.scale;
        let edge = doc.resize_edge(&layout, x, layout.header_h / 2.0).expect("edge");
        doc.resize(edge, 90.0).expect("resize");
        let sh = doc.sheet().expect("sheet");
        assert_eq!([1, 2, 3].map(|c| sh.col_width(c)), [90.0; 3]);
    }

    #[test]
    fn the_sample_has_a_chart_that_draws() {
        let doc = Doc::sample("sales");
        let charts: Vec<_> = doc.objects().into_iter().filter(|o| o.kind == gridcraft_engine::objects::ObjectKind::Chart).collect();
        assert!(!charts.is_empty(), "the sales sample carries a chart");
        let prims = doc.chart_prims(charts[0].id, &gridcraft_chart::ApproxMeasure).expect("prims");
        assert!(!prims.is_empty());
    }

    #[test]
    fn a_chart_is_hit_moved_resized_and_deleted() {
        let mut doc = Doc::sample("sales");
        let chart = doc.objects().into_iter().find(|o| o.kind == ObjectKind::Chart).expect("a chart");
        let r = ObjectRef { kind: "chart", id: chart.id };
        let layout = doc.layout(VIEW).expect("layout");
        let rect = layout.geo.anchor_rect(&chart.anchor);
        // The middle of the chart (if on screen) hits it.
        let (cx, cy) = (rect.x + 20.0, rect.y + 20.0);
        if cx < VIEW.0 && cy < VIEW.1 {
            assert!(matches!(doc.hit(&layout, cx, cy), Some(Hit::Object { id, .. }) if id == chart.id));
        }
        doc.move_object(r, 60.0, 40.0, layout.scale).expect("move");
        let moved = doc.object(r).expect("still there").anchor;
        let after = doc.layout(VIEW).expect("layout").geo.anchor_rect(&moved);
        assert!((after.x - rect.x - 60.0).abs() < 1.5 && (after.y - rect.y - 40.0).abs() < 1.5, "moved by the drag");
        doc.resize_object(r, 400.0, 250.0).expect("resize");
        let sized = doc.object(r).expect("still there").anchor;
        assert_eq!((sized.width, sized.height), (400.0, 250.0));
        doc.delete_object(r).expect("delete");
        assert!(doc.object(r).is_none());
        doc.run("edit.undo", json!({})).expect("undo");
        assert!(doc.object(r).is_some());
    }

    #[test]
    fn the_fill_handle_extends_a_series_and_double_click_fills_down() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"a,1\nb,2\nc,3\nd,4\n").expect("open");
        // Fill B1:B2 (1, 2) down to B4 as a series.
        let src = RangeRef::new(CellRef::new(0, 1), CellRef::new(1, 1));
        let target = geometry::fill_target(src, CellRef::new(3, 1));
        assert_eq!(target.a1(), "B1:B4");
        doc.run("selection.set", json!({"range": "B1:B2"})).expect("select");
        doc.run("edit.clearContents", json!({"range": "B3:B4"})).expect("clear");
        doc.fill(src, target, false).expect("fill");
        doc.select(CellRef::new(3, 1), false).expect("select");
        assert_eq!(doc.entry_text(), "4");
        assert_eq!(doc.selection().expect("selection").a1(), "B4");
        // Double-click: B1 alone fills down as far as column A has data.
        doc.run("edit.clearContents", json!({"range": "B2:B4"})).expect("clear");
        doc.run("selection.set", json!({"range": "B1"})).expect("select");
        doc.fill_down().expect("fill down");
        doc.select(CellRef::new(3, 1), false).expect("select");
        assert_eq!(doc.entry_text(), "1");
    }

    #[test]
    fn a_range_moves_but_stays_on_the_sheet() {
        let r = RangeRef::new(CellRef::new(1, 1), CellRef::new(2, 3));
        assert_eq!(moved_range(r, 3, 2).a1(), "D5:F6");
        assert_eq!(moved_range(r, -9, -9).a1(), "A1:C2", "kept at the top-left");
        let moved = moved_range(r, i64::MAX / 4, i64::MAX / 4);
        assert_eq!((moved.end.row, moved.end.col), (MAX_ROWS - 1, MAX_COLS - 1));
        assert_eq!((moved.height(), moved.width()), (r.height(), r.width()), "the size is kept");
    }

    #[test]
    fn dragging_a_selection_border_moves_the_cells_in_one_step() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"1,2\n3,4\n").expect("open");
        let from = RangeRef::new(CellRef::new(0, 0), CellRef::new(1, 1));
        doc.move_cells(from, CellRef::new(4, 3)).expect("move");
        doc.select(CellRef::new(5, 4), false).expect("select");
        assert_eq!(doc.entry_text(), "4");
        doc.select(CellRef::new(0, 0), false).expect("select");
        assert_eq!(doc.entry_text(), "");
        doc.run("edit.undo", json!({})).expect("undo");
        doc.select(CellRef::new(0, 0), false).expect("select");
        assert_eq!(doc.entry_text(), "1");
        doc.move_cells(from, from.start).expect("moving onto itself does nothing");
    }

    #[test]
    fn a_filter_dropdown_filters_and_clears() {
        let mut doc = Doc::sample("sales");
        let header = CellRef::new(3, 1);
        let menu = doc.filter_menu(header).expect("the Region header has a dropdown");
        assert!(menu.values.iter().any(|(v, on)| v == "North" && *on));
        // Show only North: the other regions' rows hide.
        doc.apply_filter(&menu, 1, &["North".to_string()]).expect("filter");
        let sh = doc.sheet().expect("sheet");
        assert!(!sh.is_row_hidden(4) && sh.is_row_hidden(5), "North stays, South hides");
        assert!(doc.filter_menu(header).expect("menu").values.iter().any(|(v, on)| v == "South" && !on));
        // Everything shown again clears it; nothing checked does nothing.
        doc.apply_filter(&menu, 1, &[]).expect("nothing");
        assert!(doc.sheet().expect("sheet").is_row_hidden(5));
        let all: Vec<String> = menu.values.iter().map(|(v, _)| v.clone()).collect();
        doc.apply_filter(&menu, 1, &all).expect("clear");
        assert!(!doc.sheet().expect("sheet").is_row_hidden(5));
        doc.sort_column(1, false).expect("sort");
        assert!(doc.filter_menu(CellRef::new(0, 0)).is_none(), "only header cells have a dropdown");
    }

    #[test]
    fn history_labels_follow_undo_and_redo() {
        let mut doc = Doc::sample("sales");
        assert_eq!(doc.history(), (None, None));
        doc.select(CellRef::new(0, 0), false).expect("select");
        doc.run("home.bold", json!({})).expect("bold");
        let (undo, redo) = doc.history();
        assert_eq!(undo.as_deref(), Some("Bold"));
        assert_eq!(redo, None);
        doc.run("edit.undo", json!({})).expect("undo");
        assert_eq!(doc.history(), (None, Some("Bold".to_string())));
        doc.run("edit.redo", json!({})).expect("redo");
        assert_eq!(doc.history().0.as_deref(), Some("Bold"));
    }

    #[test]
    fn home_end_and_page_keys_move_the_selection() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"a,b,c\n1,2,3\n4,5,6\n").expect("open");
        doc.select(CellRef::new(2, 2), false).expect("select");
        doc.home(false, false).expect("home");
        assert_eq!(doc.name_box_text(), "A3");
        doc.home(true, false).expect("ctrl+home");
        assert_eq!(doc.name_box_text(), "A1");
        doc.end().expect("ctrl+end");
        assert_eq!(doc.name_box_text(), "C3");
        doc.select(CellRef::new(2, 2), false).expect("select");
        doc.home(false, true).expect("shift+home");
        assert_eq!(doc.selection().expect("selection").a1(), "A3:C3");
        assert!(doc.page_rows(VIEW) >= 10, "a page is the rows in view");
    }

    #[test]
    fn an_engine_form_opens_edits_and_submits() {
        let mut doc = Doc::sample("sales");
        doc.select(CellRef::new(2, 0), false).expect("select");
        let mut form = doc.open_form("rowHeight", &json!({})).expect("Row Height is a form");
        assert_eq!(form.title, "Row Height");
        assert!(!form.fields.is_empty());
        forms::set_value(&mut form, "height", json!(44));
        doc.submit_form(&form).expect("submit");
        assert_eq!(doc.sheet().expect("sheet").row_height(2), 44.0);
        // A dialog that isn't a form is not one.
        assert!(doc.open_form("find", &json!({})).is_none());
        // The engine's refusal comes back for the dialog to show.
        let mut bad = doc.open_form("renameSheet", &json!({})).expect("form");
        forms::set_value(&mut bad, "name", json!(""));
        assert!(doc.submit_form(&bad).is_err());
    }

    #[test]
    fn find_next_find_all_and_replace() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"apple,pear\nApple pie,plum\ncherry,apple\n").expect("open");
        let mut o = FindOptions { what: "apple".into(), ..FindOptions::default() };
        // Not case sensitive: A1, A2 and B3.
        let all = doc.find_all(&o).expect("find all");
        assert_eq!(all.iter().map(|f| f.cell.as_str()).collect::<Vec<_>>(), ["A1", "A2", "B3"]);
        doc.select(CellRef::new(0, 0), false).expect("select");
        assert_eq!(doc.find_next(&o).expect("next"), "A2");
        assert_eq!(doc.find_next(&o).expect("next"), "B3");
        assert_eq!(doc.find_next(&o).expect("wraps"), "A1");
        o.match_case = true;
        assert_eq!(doc.find_all(&o).expect("find all").len(), 2);
        o.whole_cell = true;
        assert_eq!(doc.find_all(&o).expect("find all").len(), 2);
        o.what = "nothing like this".into();
        assert!(doc.find_next(&o).is_err());
        assert!(doc.find_all(&FindOptions::default()).is_err(), "nothing to look for");
        // Replace one: the active cell, then on to the next.
        let mut o = FindOptions { what: "apple".into(), with: "fig".into(), ..FindOptions::default() };
        doc.select(CellRef::new(0, 0), false).expect("select");
        doc.replace_one(&o).expect("replace");
        doc.select(CellRef::new(0, 0), false).expect("select");
        assert_eq!(doc.entry_text(), "fig");
        assert_eq!(doc.name_box_text(), "A1");
        // Replace all.
        o.with = "kiwi".into();
        assert_eq!(doc.replace_all(&o).expect("replace all"), 2);
        let after = doc.find_all(&FindOptions { what: "kiwi".into(), ..FindOptions::default() }).expect("find");
        assert_eq!(after.len(), 2);
        doc.show_found(&after[0]).expect("show");
        assert_eq!(doc.name_box_text(), after[0].cell);
        doc.run("edit.undo", json!({})).expect("undo");
        assert!(doc.find_all(&FindOptions { what: "kiwi".into(), ..FindOptions::default() }).is_err(), "one undo puts them back");
    }

    #[test]
    fn go_to_takes_references_and_names() {
        let mut doc = Doc::sample("sales");
        doc.go_to("C7").expect("go to");
        assert_eq!(doc.name_box_text(), "C7");
        doc.go_to("B2:D4").expect("go to a range");
        assert_eq!(doc.selection().expect("selection").a1(), "B2:D4");
        assert!(doc.go_to("not a place").is_err());
        doc.select(CellRef::new(4, 4), false).expect("select");
        doc.run("formulas.defineName", json!({"name": "Spot", "refersTo": "=Sales!$G$9"})).expect("define");
        let names = doc.names();
        assert!(names.iter().any(|n| n == "Spot"), "the name is listed");
        {
            doc.go_to("Spot").expect("go to a name");
            assert_eq!(doc.selection().expect("selection").a1(), "G9");
            assert_eq!(doc.name_box_text(), "Spot", "the Name Box shows the name");
        }
    }

    #[test]
    fn sorting_by_levels_and_headers() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"name,score\nb,2\na,2\nc,9\nd,1\n").expect("open");
        let range = RangeRef::new(CellRef::new(0, 0), CellRef::new(4, 1));
        assert_eq!(sort_columns(range), ["A", "B"]);
        // Highest score first, then name: the header stays.
        doc.sort(range, true, &[("B".into(), true), ("A".into(), false)]).expect("sort");
        let col = |doc: &mut Doc, c: u32| -> Vec<String> {
            (0..5)
                .map(|r| {
                    doc.select(CellRef::new(r, c), false).expect("select");
                    doc.entry_text()
                })
                .collect()
        };
        assert_eq!(col(&mut doc, 0), ["name", "c", "a", "b", "d"]);
        doc.run("edit.undo", json!({})).expect("undo");
        assert_eq!(col(&mut doc, 0), ["name", "b", "a", "c", "d"], "one undo");
        assert!(doc.sort(range, true, &[]).is_ok(), "no keys asks for the dialog and sorts nothing");
    }

    #[test]
    fn names_are_listed_edited_and_deleted() {
        let mut doc = Doc::sample("sales");
        doc.run("formulas.defineName", json!({"name": "Spot", "refersTo": "=Sales!$G$9", "comment": "a cell"})).expect("define");
        let rows = doc.name_rows();
        let spot = rows.iter().find(|n| n.name == "Spot").expect("listed");
        assert_eq!((spot.scope.as_str(), spot.comment.as_str()), ("Workbook", "a cell"));
        assert!(spot.refers_to.starts_with('='));
        let params = Doc::edit_name_params(spot);
        assert_eq!(params["oldName"], "Spot");
        let mut form = doc.open_form("defineName", &params).expect("the Define Name form");
        forms::set_value(&mut form, "name", json!("Place"));
        doc.submit_form(&form).expect("rename");
        let names: Vec<String> = doc.name_rows().into_iter().map(|n| n.name).collect();
        assert!(names.contains(&"Place".to_string()) && !names.contains(&"Spot".to_string()), "{names:?}");
        let place = doc.name_rows().into_iter().find(|n| n.name == "Place").expect("renamed");
        doc.delete_name(&place).expect("delete");
        assert!(doc.name_rows().iter().all(|n| n.name != "Place"));
    }

    #[test]
    fn functions_are_listed_and_insert_starts_a_formula() {
        let mut doc = Doc::sample("sales");
        let list = doc.functions();
        assert!(list.len() > 200, "{} functions", list.len());
        assert!(list.windows(2).all(|w| w[0].name < w[1].name), "sorted, once each");
        let sum = list.iter().find(|f| f.name == "SUM").expect("SUM");
        assert!(sum.signature.starts_with("SUM(") && !sum.description.is_empty());
        doc.run("formulas.insertFunction", json!({"name": "vlookup"})).expect("insert");
        let asked = doc.session.take_ui_requests();
        assert!(asked.iter().any(|r| matches!(r, gridcraft_engine::UiRequest::EditCell(Some(t)) if t == "=VLOOKUP(")), "{asked:?}");
    }

    #[test]
    fn goal_seek_finds_the_input() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"1,\n").expect("open");
        doc.run("cell.set", json!({"cell": "B1", "input": "=A1*3+4"})).expect("formula");
        let r = doc.goal_seek("B1", 25.0, "A1").expect("goal seek");
        assert!(r.converged && (r.value - 7.0).abs() < 0.01, "{r:?}");
        doc.select(CellRef::new(0, 1), false).expect("select");
        assert_eq!(doc.entry_text(), "=A1*3+4");
        assert!(doc.goal_seek("A1", 1.0, "A1").is_err(), "a constant can't be the goal");
        doc.run("edit.undo", json!({})).expect("undo");
        doc.select(CellRef::new(0, 0), false).expect("select");
        assert_eq!(doc.entry_text(), "1");
    }

    #[test]
    fn rules_are_listed_reordered_and_deleted() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"1\n2\n3\n").expect("open");
        for (range, v) in [("A1:A3", "1"), ("A2:A3", "2")] {
            let rule = json!({"type": "cellIs", "operator": "greaterThan", "value": v, "preset": "lightRedFill"});
            doc.run("home.conditionalFormat", json!({"range": range, "rule": rule})).expect("rule");
        }
        let rules = doc.rules();
        assert_eq!(rules.len(), 2);
        // The newest rule has the highest priority.
        assert_eq!(rules[0].ranges, "A2:A3");
        assert!(rules.iter().all(|r| !r.kind.is_empty()));
        doc.rule_action("moveDown", 0).expect("move down");
        assert_eq!(doc.rules()[0].ranges, "A1:A3");
        doc.rule_action("delete", 0).expect("delete");
        assert_eq!(doc.rules().len(), 1);
        assert_eq!(doc.rules()[0].ranges, "A2:A3");
        assert!(Doc::new().rules().is_empty(), "no workbook, no rules");
    }

    #[test]
    fn json_is_laid_out_as_an_outline() {
        let v = json!({"b": 2, "a": {"x": "text", "list": []}, "items": [{"k": 1}, "s"]});
        assert_eq!(outline(&v), ["a:", "  list:", "    (none)", "  x: text", "b: 2", "items:", "  k: 1", "  s"]);
        assert_eq!(spaced("CellIs"), "Cell Is");
        assert_eq!(spaced("DataBar"), "Data Bar");
        assert_eq!(spaced("Top10"), "Top10");
    }

    #[test]
    fn the_reporting_dialogs_report() {
        let mut doc = Doc::sample("sales");
        // Evaluate Formula needs a formula under the cursor: the Total of North.
        doc.select(CellRef::new(4, 6), false).expect("select");
        for (name, _, command) in RESULT_DIALOGS {
            let lines = doc.report(command);
            assert!(lines.is_ok(), "{name}: {lines:?}");
        }
        let stats = doc.report("review.workbookStatistics").expect("statistics");
        assert!(!stats.is_empty());
        assert!(Doc::new().report("review.workbookStatistics").is_err(), "nothing open");
        doc.select(CellRef::new(0, 0), false).expect("select");
        assert!(doc.report("formulas.evaluateFormula").is_err(), "a cell with no formula has nothing to evaluate");
    }

    #[test]
    fn base64_round_trips() {
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"foobar", &[0, 255, 128, 7, 9]] {
            assert_eq!(unbase64(&base64(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(unbase64("a$b"), None);
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_sample_lays_out_with_headers_and_cells() {
        let doc = Doc::sample("sales");
        let layout = doc.layout(VIEW).expect("layout");
        assert!(layout.header_w > 0.0 && layout.header_h > 0.0);
        let panes = doc.viewports(&layout, &Fixed);
        assert!(!panes.is_empty());
        assert!(panes.iter().any(|(_, v)| v.cells.iter().any(|c| c.text.is_some())), "the sample has text");
    }

    #[test]
    fn clicking_a_cell_selects_it_and_shift_extends() {
        let mut doc = Doc::sample("sales");
        let layout = doc.layout(VIEW).expect("layout");
        let r = layout.geo.cell_rect(doc.sheet().expect("sheet"), CellRef::new(2, 2));
        let cell = doc.cell_at(&layout, r.x + 3.0, r.y + 3.0).expect("a cell");
        assert_eq!(cell, CellRef::new(2, 2));
        doc.select(cell, false).expect("select");
        assert_eq!(doc.name_box_text(), "C3");
        doc.select(CellRef::new(4, 3), true).expect("extend");
        assert_eq!(doc.selection().expect("selection").a1(), "C3:D5");
        assert_eq!(doc.selection().expect("selection").active, CellRef::new(2, 2));
    }

    #[test]
    fn dragging_a_selection_in_any_direction_keeps_its_anchor() {
        let mut doc = Doc::sample("sales");
        let at = |r: u32, c: u32| CellRef::new(r, c);
        // Right to left, one cell at a time, as a drag reports it.
        doc.select(at(4, 4), false).expect("press");
        doc.select(at(4, 3), true).expect("drag");
        doc.select(at(4, 2), true).expect("drag");
        doc.select(at(4, 1), true).expect("drag");
        let sel = doc.selection().expect("selection");
        assert_eq!(sel.a1(), "B5:E5");
        assert_eq!(sel.active, at(4, 4), "the pressed cell stays active");
        // Then back toward the anchor and past it, and up-left diagonally.
        doc.select(at(4, 4), true).expect("drag");
        assert_eq!(doc.selection().expect("selection").a1(), "E5");
        doc.select(at(2, 2), true).expect("drag");
        assert_eq!(doc.selection().expect("selection").a1(), "C3:E5");
        // Keyboard extension after a leftward drag moves the dragged end, not the anchor.
        doc.select(at(4, 4), false).expect("press");
        doc.select(at(4, 2), true).expect("drag");
        doc.run("selection.move", json!({"dr": 0, "dc": -1, "extend": true})).expect("shift+left");
        assert_eq!(doc.selection().expect("selection").a1(), "B5:E5");
        doc.run("selection.move", json!({"dr": 0, "dc": 1, "extend": true})).expect("shift+right");
        assert_eq!(doc.selection().expect("selection").a1(), "C5:E5");
    }

    #[test]
    fn the_headers_are_not_cells() {
        let doc = Doc::sample("sales");
        let layout = doc.layout(VIEW).expect("layout");
        assert_eq!(doc.cell_at(&layout, 2.0, 100.0), None);
        assert_eq!(doc.cell_at(&layout, 100.0, 2.0), None);
    }

    #[test]
    fn committing_enters_the_value_and_enter_moves_down() {
        let mut doc = Doc::sample("sales");
        doc.select(CellRef::new(30, 0), false).expect("select");
        doc.commit("=1+2").expect("commit");
        doc.advance(false, false).expect("advance");
        assert_eq!(doc.name_box_text(), "A32");
        doc.select(CellRef::new(30, 0), false).expect("select");
        assert_eq!(doc.entry_text(), "=1+2");
        doc.run("edit.undo", json!({})).expect("undo");
        assert_eq!(doc.entry_text(), "");
    }

    #[test]
    fn scrolling_stays_inside_the_sheet() {
        let mut doc = Doc::sample("sales");
        doc.scroll_by(VIEW, -500.0, -500.0);
        assert_eq!(doc.scroll, (0.0, 0.0));
        doc.scroll_by(VIEW, 0.0, 800.0);
        assert!(doc.scroll.1 > 0.0);
        doc.scroll_by(VIEW, 1.0e9, 1.0e9);
        assert!(doc.scroll.0.is_finite() && doc.scroll.1.is_finite());
    }

    #[test]
    fn the_toolbar_state_follows_the_selection() {
        let mut doc = Doc::sample("sales");
        doc.select(CellRef::new(0, 0), false).expect("select");
        assert_eq!(doc.selection_format().expect("format").font.bold, Some(false));
        doc.run("home.bold", json!({})).expect("bold");
        assert_eq!(doc.selection_format().expect("format").font.bold, Some(true));
    }

    #[test]
    fn copy_then_paste_moves_the_text() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"7,8\n").expect("open");
        doc.select(CellRef::new(0, 0), false).expect("select");
        let text = doc.copy(false).expect("copy");
        assert_eq!(text.trim_end(), "7");
        doc.select(CellRef::new(3, 0), false).expect("select");
        doc.paste(Some(&text)).expect("paste");
        assert_eq!(doc.entry_text(), "7");
    }

    #[test]
    fn a_workbook_opens_from_bytes() {
        let mut doc = Doc::new();
        doc.open_bytes("t.csv", b"a,b\n1,2\n").expect("open");
        assert_eq!(doc.sheet_tabs().len(), 1);
        doc.select(CellRef::new(1, 1), false).expect("select");
        assert_eq!(doc.entry_text(), "2");
    }
}
