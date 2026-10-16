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
