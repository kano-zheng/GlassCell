//! Floating objects for Swift: what is on the sheet, chart and sparkline primitives, picture
//! bytes, and where a dragged object lands.

use std::sync::Arc;

use gridcraft_chart::Prim;
use gridcraft_engine::model::{self as model, ShapeKind, ShapeTextAlign, ShapeTextAnchor};
use gridcraft_engine::objects as obj;

use crate::{CellAddress, Engine, FfiError, FontFace, GridGeometry, GridRect, Rgb, SparklineKind, TextMeasurer, engine_error, guard};

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ObjectAnchor {
    pub cell: CellAddress,
    /// Offset into the cell and size, in sheet units.
    pub dx: f32,
    pub dy: f32,
    pub width: f32,
    pub height: f32,
}

impl From<model::Anchor> for ObjectAnchor {
    fn from(a: model::Anchor) -> Self {
        ObjectAnchor { cell: a.cell.into(), dx: a.dx, dy: a.dy, width: a.width, height: a.height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ShapeType {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Triangle,
    Line,
    Arrow,
    TextBox,
    Icon,
    Ink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TextAnchor {
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct InkStroke {
    /// Points relative to the anchor, sheet units, as x, y pairs.
    pub points: Vec<f32>,
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ShapeStyle {
    pub shape: ShapeType,
    pub fill: Option<Rgb>,
    pub line: Option<Rgb>,
    pub text: String,
    pub text_color: Rgb,
    pub text_size: f32,
    pub font: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
    pub anchor: TextAnchor,
    pub ink: Option<InkStroke>,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ObjectContent {
    Shape { style: ShapeStyle },
    Picture { mime: String, alt: String },
    Chart,
    Slicer { caption: String },
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SheetObject {
    pub id: u32,
    /// For `object.move` / `object.delete`: `shape`, `image`, `chart` or `slicer`.
    pub kind: String,
    pub anchor: ObjectAnchor,
    pub content: ObjectContent,
}

fn shape_type(k: ShapeKind) -> ShapeType {
    match k {
        ShapeKind::Rectangle => ShapeType::Rectangle,
        ShapeKind::RoundedRectangle => ShapeType::RoundedRectangle,
        ShapeKind::Ellipse => ShapeType::Ellipse,
        ShapeKind::Triangle => ShapeType::Triangle,
        ShapeKind::Line => ShapeType::Line,
        ShapeKind::Arrow => ShapeType::Arrow,
        ShapeKind::TextBox => ShapeType::TextBox,
        ShapeKind::Icon => ShapeType::Icon,
        ShapeKind::Ink => ShapeType::Ink,
    }
}

impl From<obj::ObjectView> for SheetObject {
    fn from(o: obj::ObjectView) -> Self {
        let (kind, content) = match o.kind {
            obj::ObjectKind::Shape(s) => (
                "shape",
                ObjectContent::Shape {
                    style: ShapeStyle {
                        shape: shape_type(s.shape),
                        fill: s.fill.map(Rgb::from),
                        line: s.line.map(Rgb::from),
                        text: s.text,
                        text_color: s.text_color.into(),
                        text_size: s.text_size,
                        font: s.font,
                        bold: s.bold,
                        italic: s.italic,
                        align: match s.align {
                            ShapeTextAlign::Left => TextAlign::Left,
                            ShapeTextAlign::Center => TextAlign::Center,
                            ShapeTextAlign::Right => TextAlign::Right,
                        },
                        anchor: match s.anchor {
                            ShapeTextAnchor::Top => TextAnchor::Top,
                            ShapeTextAnchor::Middle => TextAnchor::Middle,
                            ShapeTextAnchor::Bottom => TextAnchor::Bottom,
                        },
                        ink: s.ink.map(|(pts, width)| InkStroke { points: pts.into_iter().flatten().collect(), width }),
                        icon: s.icon,
                    },
                },
            ),
            obj::ObjectKind::Picture { mime, alt } => ("image", ObjectContent::Picture { mime, alt }),
            obj::ObjectKind::Chart => ("chart", ObjectContent::Chart),
            obj::ObjectKind::Slicer { caption } => ("slicer", ObjectContent::Slicer { caption }),
        };
        SheetObject { id: o.id, kind: kind.into(), anchor: o.anchor.into(), content }
    }
}

/// An RGBA colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<[u8; 4]> for Rgba {
    fn from([r, g, b, a]: [u8; 4]) -> Self {
        Rgba { r, g, b, a }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Stroke {
    pub color: Rgba,
    pub width: f32,
}

/// A drawing primitive (chart units from the chart's top-left; y down). Points are x, y pairs.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ChartPrim {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Option<Rgba>,
        stroke: Option<Stroke>,
        radius: f32,
    },
    Line {
        points: Vec<f32>,
        color: Rgba,
        width: f32,
        dash: bool,
    },
    Polygon {
        points: Vec<f32>,
        fill: Rgba,
        stroke: Option<Stroke>,
    },
    /// Angles in radians, 0 at 12 o'clock, clockwise.
    Wedge {
        cx: f32,
        cy: f32,
        r_outer: f32,
        r_inner: f32,
        a0: f32,
        a1: f32,
        fill: Rgba,
        stroke: Option<Stroke>,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: Option<Rgba>,
        stroke: Option<Stroke>,
    },
    /// Anchored at (x, y): `align` 0 left, 1 centre, 2 right; `valign` 0 top, 1 middle, 2 bottom;
    /// rotated by `rotation` radians about the anchor.
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: Rgba,
        bold: bool,
        align: u8,
        valign: u8,
        rotation: f32,
    },
}

fn stroke(s: Option<([u8; 4], f32)>) -> Option<Stroke> {
    s.map(|(c, w)| Stroke { color: c.into(), width: w })
}

fn flat(pts: Vec<[f32; 2]>) -> Vec<f32> {
    pts.into_iter().flatten().collect()
}

impl From<Prim> for ChartPrim {
    fn from(p: Prim) -> Self {
        use gridcraft_chart::{HAlign as H, VAlign as V};
        match p {
            Prim::Rect { x, y, w, h, fill, stroke: s, radius } => {
                ChartPrim::Rect { x, y, w, h, fill: fill.map(Rgba::from), stroke: stroke(s), radius }
            }
            Prim::Line { pts, color, width, dash } => ChartPrim::Line { points: flat(pts), color: color.into(), width, dash },
            Prim::Polygon { pts, fill, stroke: s } => ChartPrim::Polygon { points: flat(pts), fill: fill.into(), stroke: stroke(s) },
            Prim::Wedge { cx, cy, r_outer, r_inner, a0, a1, fill, stroke: s } => {
                ChartPrim::Wedge { cx, cy, r_outer, r_inner, a0, a1, fill: fill.into(), stroke: stroke(s) }
            }
            Prim::Circle { cx, cy, r, fill, stroke: s } => ChartPrim::Circle { cx, cy, r, fill: fill.map(Rgba::from), stroke: stroke(s) },
            Prim::Text { x, y, text, size, color, bold, align, valign, rotation } => ChartPrim::Text {
                x,
                y,
                text,
                size,
                color: color.into(),
                bold,
                align: match align {
                    H::Left => 0,
                    H::Center => 1,
                    H::Right => 2,
                },
                valign: match valign {
                    V::Top => 0,
                    V::Middle => 1,
                    V::Bottom => 2,
                },
                rotation,
            },
        }
    }
}

/// Chart text measured by Swift (the system font).
struct ChartMeasure(Arc<dyn TextMeasurer>);

impl gridcraft_chart::Measure for ChartMeasure {
    fn text_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let w = self.0.text_width(text.to_string(), FontFace { name: String::new(), bold, italic: false }, size);
        if w.is_finite() { w.clamp(0.0, 1.0e6) } else { 0.0 }
    }
}

/// A sparkline drawn into `width` × `height`.
#[uniffi::export]
pub fn sparkline_prims(kind: SparklineKind, values: Vec<Option<f64>>, color: Rgb, markers: bool, width: f32, height: f32) -> Vec<ChartPrim> {
    let kind = match kind {
        SparklineKind::Line => model::SparklineKind::Line,
        SparklineKind::Column => model::SparklineKind::Column,
        SparklineKind::WinLoss => model::SparklineKind::WinLoss,
    };
    let (w, h) = (if width.is_finite() { width.clamp(0.0, 1.0e5) } else { 0.0 }, if height.is_finite() { height.clamp(0.0, 1.0e5) } else { 0.0 });
    let values: Vec<Option<f64>> = values.into_iter().take(100_000).collect();
    gridcraft_chart::render_sparkline(kind, &values, [color.r, color.g, color.b, 255], markers, w, h).into_iter().map(ChartPrim::from).collect()
}

#[uniffi::export]
impl Engine {
    /// The active sheet's floating objects, bottom first.
    pub fn objects(&self) -> Result<Vec<SheetObject>, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            Ok(obj::objects(&d.wb, d.wb.active_sheet).into_iter().map(SheetObject::from).collect())
        })
    }

    /// A picture's PNG or JPEG bytes.
    pub fn picture(&self, id: u32) -> Result<Option<Vec<u8>>, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            Ok(obj::picture(&d.wb, d.wb.active_sheet, id).map(<[u8]>::to_vec))
        })
    }

    /// Chart `id` as primitives at its anchor size (sheet units).
    pub fn chart_prims(&self, id: u32, measurer: Arc<dyn TextMeasurer>) -> Result<Vec<ChartPrim>, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let prims = obj::chart_prims(&d.wb, d.wb.active_sheet, id, &ChartMeasure(measurer)).ok_or_else(|| engine_error("no such chart"))?;
            Ok(prims.into_iter().map(ChartPrim::from).collect())
        })
    }

    /// Where object `anchor` lands dragged by `(dx, dy)` sheet units, as `object.move`'s
    /// `at`, `dx` and `dy`.
    pub fn moved_anchor(&self, anchor: ObjectAnchor, dx: f32, dy: f32) -> Result<ObjectAnchor, FfiError> {
        guard(|| {
            let s = self.session();
            let d = s.active().ok_or_else(|| engine_error("no workbook open"))?;
            let sh = d.wb.active().ok_or_else(|| engine_error("no sheet"))?;
            let a = model::Anchor {
                cell: gridcraft_engine::core::CellRef::new(anchor.cell.row, anchor.cell.col),
                dx: anchor.dx,
                dy: anchor.dy,
                width: anchor.width,
                height: anchor.height,
                ..model::Anchor::default()
            };
            let (cell, ndx, ndy) = obj::moved_anchor(sh, &a, dx, dy);
            Ok(ObjectAnchor { cell: cell.into(), dx: ndx, dy: ndy, width: anchor.width, height: anchor.height })
        })
    }
}

#[uniffi::export]
impl GridGeometry {
    /// Where an object anchored at `anchor` is on screen.
    pub fn object_rect(&self, anchor: ObjectAnchor) -> GridRect {
        let a = model::Anchor {
            cell: gridcraft_engine::core::CellRef::new(
                anchor.cell.row.min(gridcraft_engine::core::MAX_ROWS - 1),
                anchor.cell.col.min(gridcraft_engine::core::MAX_COLS - 1),
            ),
            dx: anchor.dx,
            dy: anchor.dy,
            width: anchor.width,
            height: anchor.height,
            ..model::Anchor::default()
        };
        self.geometry().anchor_rect(&a).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mono;
    impl TextMeasurer for Mono {
        fn text_width(&self, text: String, _face: FontFace, size: f32) -> f32 {
            text.chars().count() as f32 * size / 2.0
        }
    }

    #[test]
    fn objects_and_charts_come_through() {
        let e = Engine::new();
        e.execute("file.new".into(), r#"{"sample": "sales"}"#.into()).unwrap();
        let list = e.objects().unwrap();
        let chart = list.iter().find(|o| o.kind == "chart").unwrap();
        assert!(e.chart_prims(chart.id, Arc::new(Mono)).unwrap().len() > 10);
        assert!(e.chart_prims(99_999, Arc::new(Mono)).is_err());
        let moved = e.moved_anchor(chart.anchor, 1.0e9, -1.0e9).unwrap();
        assert!(moved.cell.row == 0);
        // Hostile sizes don't crash; a real size draws.
        let _ = sparkline_prims(SparklineKind::Line, vec![Some(1.0), None, Some(3.0)], Rgb { r: 0, g: 0, b: 0 }, true, f32::NAN, f32::INFINITY);
        assert!(!sparkline_prims(SparklineKind::Column, vec![Some(1.0), Some(3.0)], Rgb { r: 0, g: 0, b: 0 }, false, 60.0, 20.0).is_empty());
    }
}
