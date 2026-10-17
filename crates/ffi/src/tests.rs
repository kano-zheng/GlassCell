use std::sync::Arc;

use super::*;

/// Every character is half the font size wide.
struct Mono;
impl TextMeasurer for Mono {
    fn text_width(&self, text: String, _face: FontFace, size: f32) -> f32 {
        text.chars().count() as f32 * size / 2.0
    }
}

/// A measurer that answers nonsense.
struct Broken;
impl TextMeasurer for Broken {
    fn text_width(&self, _text: String, _face: FontFace, _size: f32) -> f32 {
        f32::NAN
    }
}

fn engine() -> Arc<Engine> {
    let e = Engine::new();
    e.execute("file.new".into(), r#"{"sample": "sales"}"#.into()).unwrap();
    e
}

fn query(rows: (u32, u32), cols: (u32, u32)) -> ViewportQuery {
    ViewportQuery { sheet: None, rows: rows.into(), cols: cols.into(), scale: 0.8, formula_locale: "en".into() }
}

#[test]
fn commands_run_and_report_errors() {
    let e = engine();
    let r = e.execute("cell.set".into(), r#"{"cell": "A1", "input": "=1+2"}"#.into()).unwrap();
    assert!(r.starts_with('{'), "{r}");
    let got = e.execute("cell.get".into(), r#"{"cell": "A1"}"#.into()).unwrap();
    assert!(got.contains("\"text\":\"3\""), "{got}");
    assert!(matches!(e.execute("no.such".into(), String::new()), Err(FfiError::Engine { .. })));
    assert!(matches!(e.execute("cell.set".into(), "{not json".into()), Err(FfiError::Engine { .. })));
    assert!(e.commands().unwrap().iter().any(|c| c.id == "home.bold" && c.enabled));
}

#[test]
fn without_a_workbook_read_models_are_errors_not_crashes() {
    let e = Engine::new();
    assert!(e.viewport(query((0, 10), (0, 10)), Arc::new(Mono)).is_err());
    assert!(e.selection_format().is_err());
    assert!(e.geometry(GridPoint { x: 0.0, y: 0.0 }, GridSize { width: 100.0, height: 100.0 }, GridPoint { x: 0.0, y: 0.0 }, 1.0).is_err());
    assert_eq!(e.name_box_text(false).unwrap(), "");
    assert!(!e.autosave_due().unwrap());
}

#[test]
fn the_viewport_lays_out_the_sample() {
    let e = engine();
    let v = e.viewport(query((0, 30), (0, 12)), Arc::new(Mono)).unwrap();
    let title = v.cells.iter().find(|c| c.text.as_ref().is_some_and(|t| t.text == "Quarterly Sales Dashboard")).unwrap();
    assert_eq!(title.cell, CellAddress { row: 1, col: 1 });
    assert!(!v.filter_buttons.is_empty());
    assert!(!v.sparklines.is_empty());
    // A broken measurer still gives a layout.
    assert!(!e.viewport(query((0, 30), (0, 12)), Arc::new(Broken)).unwrap().cells.is_empty());
}

#[test]
fn selection_format_and_geometry_follow_the_selection() {
    let e = engine();
    e.execute("home.italic".into(), r#"{"range": "B4", "on": true}"#.into()).unwrap();
    e.execute("selection.set".into(), r#"{"range": "B4:C4", "active": "B4"}"#.into()).unwrap();
    let f = e.selection_format().unwrap();
    assert_eq!(f.cells, 2);
    assert_eq!(f.active.italic, Some(true));
    assert_eq!(f.selection.italic, None, "mixed");
    let g = e.geometry(GridPoint { x: 0.0, y: 0.0 }, GridSize { width: 900.0, height: 600.0 }, GridPoint { x: 0.0, y: 0.0 }, 0.8).unwrap();
    let r = g.range_rect(CellRange { first: CellAddress { row: 3, col: 1 }, last: CellAddress { row: 3, col: 2 } });
    assert_eq!(g.hit(r.x + r.width, r.y + r.height), GridHit::FillHandle);
    assert_eq!(g.hit(r.x + 20.0, r.y + r.height / 2.0), GridHit::Cell { cell: CellAddress { row: 3, col: 1 } });
    assert!(!g.panes().is_empty());
    assert_eq!(g.x(u32::MAX), g.x(gridcraft_engine::core::MAX_COLS - 1));
    assert_eq!(e.selection().unwrap().ranges, vec![CellRange { first: CellAddress { row: 3, col: 1 }, last: CellAddress { row: 3, col: 2 } }]);
    let sheet = e.sheet().unwrap();
    assert_eq!((sheet.name.as_str(), sheet.zoom), ("Sales", 100));
}

#[test]
fn ui_requests_and_the_clipboard_rules_come_through() {
    let e = engine();
    e.execute("home.formatCells".into(), String::new()).unwrap();
    assert!(e.take_ui_requests().unwrap().iter().any(|r| matches!(r, UiRequest::Dialog { name, .. } if name == "formatCells")));
    e.execute("edit.cut".into(), r#"{"range": "B5"}"#.into()).unwrap();
    e.clipboard_published(true, "North".into()).unwrap();
    assert_eq!(e.paste_for_text("North".into()).unwrap(), PasteSource::Text { text: "North".into() });
    assert!(e.open_notice(r#"{"warnings": ["VBA project kept"]}"#.into()).unwrap().is_some_and(|n| n.kind == NoticeKind::Warning));
    assert!(e.open_notice("not json".into()).is_err());
}

#[test]
fn references_are_named_like_the_engine_names_them() {
    assert_eq!(column_label(0), "A");
    assert_eq!(column_label(27), "AB");
    assert_eq!(column_label(u32::MAX), "XFD");
    assert_eq!(cell_name(CellAddress { row: 2, col: 1 }), "B3");
    let r = CellRange { first: CellAddress { row: 0, col: 0 }, last: CellAddress { row: 4, col: 2 } };
    assert_eq!(range_name(r), "A1:C5");
}

#[test]
fn history_follows_undoable_commands() {
    let e = engine();
    let h0 = e.history().unwrap();
    assert!(!h0.dirty);
    e.execute("cell.set".into(), r#"{"cell": "A1", "input": "x"}"#.into()).unwrap();
    let h1 = e.history().unwrap();
    assert_ne!(h1.revision, h0.revision);
    assert_eq!(h1.undo.first().map(String::as_str), Some("Enter Cell"));
    assert!(h1.dirty);
    e.execute("selection.set".into(), r#"{"range": "B2"}"#.into()).unwrap();
    assert_eq!(e.history().unwrap().revision, h1.revision, "selecting isn't undoable");
    e.execute("edit.undo".into(), String::new()).unwrap();
    assert_eq!(e.history().unwrap().redo.first().map(String::as_str), Some("Enter Cell"));
}

#[test]
fn editing_starts_from_the_engine_and_commits_through_it() {
    let e = engine();
    e.execute("selection.set".into(), r#"{"range": "B2"}"#.into()).unwrap();
    let start = e.edit_start("en".into()).unwrap().unwrap();
    assert_eq!((start.cell, start.text.as_str()), (CellAddress { row: 1, col: 1 }, "Quarterly Sales Dashboard"));
    assert!(e.function_names("es".into()).unwrap().iter().any(|n| n == "SUMA"));
    let r = CellRange { first: CellAddress { row: 0, col: 0 }, last: CellAddress { row: 1, col: 1 } };
    assert_eq!(e.point_reference(r, 0).unwrap(), "A1:B2");
    assert_eq!(e.column_completion(CellAddress { row: 10, col: 1 }, "Nor".into()).unwrap().as_deref(), Some("North"));
}

#[test]
fn fill_and_resize_rules_come_through() {
    let e = engine();
    let a = |row, col| CellAddress { row, col };
    assert_eq!(fill_target(CellRange { first: a(4, 2), last: a(4, 2) }, a(9, 2)), CellRange { first: a(4, 2), last: a(9, 2) });
    // H5: the column beside it (G, the totals) has data down to G11.
    e.execute("selection.set".into(), r#"{"range": "H5"}"#.into()).unwrap();
    assert_eq!(e.fill_down_target().unwrap(), Some(CellRange { first: a(4, 7), last: a(10, 7) }));
    assert_eq!(e.resize_columns(3).unwrap(), "D:D");
    assert!(resize_label(true, 64.0).starts_with("Width: 8.43"));
}

#[test]
fn page_layout_geometry_has_pages() {
    let e = engine();
    e.execute("view.pageLayout".into(), String::new()).unwrap();
    let g = e.geometry(GridPoint { x: 0.0, y: 0.0 }, GridSize { width: 1200.0, height: 900.0 }, GridPoint { x: 0.0, y: 0.0 }, 0.8).unwrap();
    assert!(!g.pages().is_empty());
    assert!(g.scale() > 0.0);
    e.execute("view.normal".into(), String::new()).unwrap();
    let g = e.geometry(GridPoint { x: 0.0, y: 0.0 }, GridSize { width: 1200.0, height: 900.0 }, GridPoint { x: 0.0, y: 0.0 }, 0.8).unwrap();
    assert!(g.pages().is_empty());
}

#[test]
fn the_journal_lists_commands_without_passwords() {
    let e = engine();
    e.execute("cell.set".into(), r#"{"cell": "A1", "input": "1"}"#.into()).unwrap();
    e.execute("review.protectSheet".into(), r#"{"password": "secret"}"#.into()).unwrap();
    let journal = e.journal().unwrap();
    assert!(journal.contains("cell.set"));
    assert!(!journal.contains("secret"));
}

#[test]
fn cell_names_parse_back() {
    let cell = parse_cell(" $C$7 ".into()).unwrap();
    assert_eq!((cell.row, cell.col), (6, 2));
    assert_eq!(cell_name(cell), "C7");
    assert!(parse_cell("not a cell".into()).is_none());
    assert!(parse_cell("".into()).is_none());
}

#[test]
fn the_search_summary_lists_labels_and_capped_text() {
    let e = engine();
    e.execute("file.new".into(), r#"{"sample": "sales"}"#.into()).unwrap();
    let full = e.search_summary(100_000).unwrap();
    assert_eq!(full.sheets, vec!["Sales".to_string()]);
    assert!(full.tables.contains(&"Sales".to_string()));
    assert!(full.text.contains("Quarterly Sales Dashboard"));
    assert!(full.text.contains("North"));
    let short = e.search_summary(40).unwrap();
    assert!(short.text.chars().count() <= 40);
    assert!(e.search_summary(0).unwrap().text.is_empty());
}

#[test]
fn the_status_bar_sums_a_selection_in_its_format() {
    let e = engine();
    e.execute("file.new".into(), r#"{"sample": "sales"}"#.into()).unwrap();
    e.execute("selection.set".into(), r#"{"range": "C5"}"#.into()).unwrap();
    assert!(e.status_items().unwrap().is_empty());
    e.execute("selection.set".into(), r#"{"range": "B5:D6"}"#.into()).unwrap();
    let items = e.status_items().unwrap();
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["Sum", "Count", "Average"]);
    // The active cell B5 is a label: General shows grouped digits.
    assert_eq!(items[0].value, "176,670");
    assert_eq!(items[1].value, "6");
    e.execute("selection.set".into(), r#"{"range": "C5:D6"}"#.into()).unwrap();
    let items = e.status_items().unwrap();
    assert_eq!(items[0].value, "$176,670");
    assert_eq!(items[1].value, "4");
}

#[test]
fn a_tables_header_has_a_filter_menu() {
    let e = engine();
    e.execute("file.new".into(), r#"{"sample": "sales"}"#.into()).unwrap();
    let menu = e.filter_menu(CellAddress { row: 3, col: 1 }).unwrap().unwrap();
    assert_eq!(menu.range, "B4:I11");
    assert_eq!(menu.column, "B");
    assert!(menu.values.iter().any(|v| v.text == "North" && v.shown));
    assert!(e.filter_menu(CellAddress { row: 4, col: 1 }).unwrap().is_none());
    let params = format!(r#"{{"column": "B", "values": ["North", "South"], "range": "{}"}}"#, menu.range);
    e.execute("data.filterBy".into(), params).unwrap();
    let menu = e.filter_menu(CellAddress { row: 3, col: 1 }).unwrap().unwrap();
    assert!(menu.values.iter().any(|v| v.text == "East" && !v.shown));
}
