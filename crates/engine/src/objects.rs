//! The floating objects of a sheet as a frontend draws them: shapes (with their colours and
//! text style resolved, defaults included), pictures, charts (as `gridcraft_chart` primitives)
//! and slicers, in paint order; and where a moved object lands.

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS};
use gridcraft_model::{Anchor, Color, ShapeKind, ShapeTextAlign, ShapeTextAnchor, Sheet, Workbook};
use serde::Serialize;

use crate::viewport::Rgb;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObjectView {
    pub id: u32,
    pub anchor: Anchor,
    pub kind: ObjectKind,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ObjectKind {
    Shape(ShapeView),
    /// A PNG or JPEG ([`picture`] has the bytes).
    Picture {
        mime: String,
        alt: String,
    },
    /// [`chart_prims`] draws it.
    Chart,
    Slicer {
        caption: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShapeView {
    pub shape: ShapeKind,
    /// `None`: no fill / no outline.
    pub fill: Option<Rgb>,
    pub line: Option<Rgb>,
    pub text: String,
    pub text_color: Rgb,
    /// Text size in sheet units (96 per inch).
    pub text_size: f32,
    pub font: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub align: ShapeTextAlign,
    pub anchor: ShapeTextAnchor,
    /// A freehand stroke: points relative to the anchor (sheet units), and its width.
    pub ink: Option<(Vec<[f32; 2]>, f32)>,
    /// GridCraft's own icon set: the icon's name.
    pub icon: Option<String>,
}

fn rgb(c: Color, wb: &Workbook) -> Option<Rgb> {
    c.resolve(&wb.theme)
}

fn shape_view(wb: &Workbook, sp: &gridcraft_model::Shape) -> ShapeView {
    let f = &sp.format;
    let text_box = sp.kind == ShapeKind::TextBox;
    let ink = (sp.kind == ShapeKind::Ink).then(|| (crate::cmd::draw::ink_points(sp), crate::cmd::draw::ink_width(sp)));
    let icon = (sp.kind == ShapeKind::Icon).then(|| sp.text.clone());
    let plain = ink.is_none() && icon.is_none();
    ShapeView {
        shape: sp.kind,
        fill: if plain { rgb(sp.fill, wb) } else { None },
        line: if f.no_line { None } else { rgb(sp.line, wb).or(Some([0x40, 0x40, 0x40])) },
        text: if plain { sp.text.clone() } else { String::new() },
        // Text boxes write black by default, other shapes white (on their accent fill).
        text_color: f.text_color.and_then(|c| rgb(c, wb)).unwrap_or(if text_box { [0, 0, 0] } else { [255, 255, 255] }),
        // A set size is in points; the default is 14 sheet units.
        text_size: f.font_size.map_or(14.0, |pt| pt * 96.0 / 72.0).clamp(1.0, 2000.0),
        font: f.font.clone(),
        bold: f.bold,
        italic: f.italic,
        align: f.align.unwrap_or(if text_box { ShapeTextAlign::Left } else { ShapeTextAlign::Center }),
        anchor: f.anchor.unwrap_or(if text_box { ShapeTextAnchor::Top } else { ShapeTextAnchor::Middle }),
        ink,
        icon,
    }
}

/// The objects on sheet `sheet`, bottom first: shapes, pictures, charts, slicers.
pub fn objects(wb: &Workbook, sheet: usize) -> Vec<ObjectView> {
    let Some(sh) = wb.sheet(sheet) else { return vec![] };
    let mut out: Vec<ObjectView> =
        sh.shapes.iter().map(|sp| ObjectView { id: sp.id, anchor: sp.anchor, kind: ObjectKind::Shape(shape_view(wb, sp)) }).collect();
    out.extend(sh.images.iter().map(|im| ObjectView {
        id: im.id,
        anchor: im.anchor,
        kind: ObjectKind::Picture { mime: im.mime.clone(), alt: im.alt.clone() },
    }));
    out.extend(sh.charts.iter().map(|ch| ObjectView { id: ch.id, anchor: ch.anchor, kind: ObjectKind::Chart }));
    out.extend(sh.slicers.iter().map(|sl| ObjectView { id: sl.id, anchor: sl.anchor, kind: ObjectKind::Slicer { caption: sl.caption.clone() } }));
    out
}

/// A picture's bytes.
pub fn picture(wb: &Workbook, sheet: usize, id: u32) -> Option<&[u8]> {
    wb.sheet(sheet)?.images.iter().find(|im| im.id == id).map(|im| im.data.as_slice())
}

/// Chart `id` drawn at its anchor's size (sheet units), as primitives from its top-left corner.
pub fn chart_prims(wb: &Workbook, sheet: usize, id: u32, m: &dyn gridcraft_chart::Measure) -> Option<Vec<gridcraft_chart::Prim>> {
    let ch = wb.sheet(sheet)?.charts.iter().find(|c| c.id == id)?;
    let data = gridcraft_chart::resolve(wb, sheet, ch);
    Some(gridcraft_chart::render(ch, &data, ch.anchor.width, ch.anchor.height, m))
}

/// Where an object anchored at `a` lands when dragged by `(dx, dy)` sheet units: the cell under
/// its new top-left corner, and the offset into it.
pub fn moved_anchor(sh: &Sheet, a: &Anchor, dx: f32, dy: f32) -> (CellRef, f32, f32) {
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let x0 = (sh.col_left(a.cell.col.min(MAX_COLS - 1)) as f32 + a.dx + finite(dx)).max(0.0);
    let y0 = (sh.row_top(a.cell.row.min(MAX_ROWS - 1)) as f32 + a.dy + finite(dy)).max(0.0);
    let cell = CellRef::new(sh.row_at(f64::from(y0)), sh.col_at(f64::from(x0)));
    (cell, x0 - sh.col_left(cell.col) as f32, y0 - sh.row_top(cell.row) as f32)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Session;

    #[test]
    fn the_sample_charts_and_a_shape_are_listed_and_drawn() {
        let mut s = Session::new();
        s.execute("file.new", json!({"sample": "sales"})).unwrap();
        s.execute("insert.shape", json!({"kind": "textBox", "at": "B20", "text": "Note"})).unwrap();
        let d = s.active().unwrap();
        let list = objects(&d.wb, 0);
        let charts: Vec<_> = list.iter().filter(|o| o.kind == ObjectKind::Chart).collect();
        assert_eq!(charts.len(), 2);
        let prims = chart_prims(&d.wb, 0, charts[0].id, &gridcraft_chart::ApproxMeasure).unwrap();
        assert!(prims.len() > 10);
        let text_box = list.iter().find_map(|o| if let ObjectKind::Shape(sv) = &o.kind { Some(sv) } else { None }).unwrap();
        assert_eq!((text_box.text.as_str(), text_box.text_color, text_box.align), ("Note", [0, 0, 0], ShapeTextAlign::Left));
        assert!(chart_prims(&d.wb, 0, 9999, &gridcraft_chart::ApproxMeasure).is_none());
        assert!(objects(&d.wb, 42).is_empty());
    }

    #[test]
    fn a_dragged_object_lands_in_the_cell_under_its_corner() {
        let sh = Sheet::new("S");
        let a = Anchor { cell: CellRef::new(1, 1), dx: 5.0, dy: 2.0, width: 100.0, height: 50.0, ..Anchor::default() };
        let w = sh.default_col_width;
        let (cell, dx, _) = moved_anchor(&sh, &a, w, 0.0);
        assert_eq!((cell, dx), (CellRef::new(1, 2), 5.0));
        let (cell, dx, dy) = moved_anchor(&sh, &a, -1.0e9, f32::NAN);
        assert_eq!((cell.col, dx), (0, 0.0));
        assert_eq!(dy, 2.0);
    }
}
