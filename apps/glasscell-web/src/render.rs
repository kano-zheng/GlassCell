//! Paints the engine's layout onto a canvas: headers, gridlines, the cells of each pane (fills,
//! text, data bars, icons, checkboxes, borders, comment marks, sparklines, filter buttons), the
//! frozen-pane lines and the selection. All appearance decisions are the engine's; this only
//! places and paints, as the Mac app's `GridRenderer` does.
//!
//! Draws in points (the canvas is scaled by the device pixel ratio) with the origin at the top
//! left.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gridcraft_chart::{HAlign as PHAlign, Prim, VAlign as PVAlign};
use gridcraft_engine::core::RangeRef;
use gridcraft_engine::geometry::{Geometry, Pane, Rect};
use gridcraft_engine::model::{BorderStyle, HAlign, ShapeKind, ShapeTextAlign, ShapeTextAnchor, Sheet, Underline, VAlign};
use gridcraft_engine::objects::{ObjectKind, ObjectView, ShapeView};
use gridcraft_engine::viewport::{BorderSeg, CellText, CellView, CommentMark, DataBar, Edge, Rgb, TextLayout, Viewport};
use wasm_bindgen::JsCast as _;
use web_sys::CanvasRenderingContext2d as Ctx;

use crate::doc::{Doc, Layout, ObjectRef, column_label, kind_name};
use crate::measure::font_css;

const SHEET: &str = "#ffffff";
const GRIDLINE: &str = "#dbdbdb";
const FROZEN_LINE: &str = "#969696";
const SECONDARY: &str = "#6e6e6e";
const NOTE: &str = "#d12e2e";
const THREADED: &str = "#8754c7";
const CHECKBOX: &str = "#0f7d40";
const ACCENT: &str = "#007aff";
const SELECTION_FILL: &str = "rgba(0, 122, 255, 0.12)";
/// The chrome around the cells (the headers) follows the system's light or dark appearance; the
/// cells stay on white paper, as the file defines them.
struct Chrome {
    header: &'static str,
    line: &'static str,
    text: &'static str,
    selected: &'static str,
}

const LIGHT: Chrome = Chrome { header: "#f5f5f5", line: "#d1d1d1", text: "#545454", selected: "rgba(0, 122, 255, 0.18)" };
const DARK: Chrome = Chrome { header: "#2c2c2e", line: "#48484a", text: "#b0b0b5", selected: "rgba(10, 132, 255, 0.4)" };

const UI_FONT: &str = "system-ui, -apple-system, \"Helvetica Neue\", sans-serif";

fn rgb(c: Rgb) -> String {
    format!("rgb({},{},{})", c[0], c[1], c[2])
}

fn fill_rect(ctx: &Ctx, x: f64, y: f64, w: f64, h: f64) {
    ctx.fill_rect(x, y, w, h);
}

fn line(ctx: &Ctx, a: (f64, f64), b: (f64, f64)) {
    ctx.begin_path();
    ctx.move_to(a.0, a.1);
    ctx.line_to(b.0, b.1);
    ctx.stroke();
}

fn clip(ctx: &Ctx, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.rect(x, y, w.max(0.0), h.max(0.0));
    ctx.clip();
}

fn dash(ctx: &Ctx, pattern: &[f64]) {
    let a = js_sys::Array::new();
    for p in pattern {
        a.push(&(*p).into());
    }
    let _ = ctx.set_line_dash(&a);
}

/// Everything the painter needs about one frame.
pub struct Frame<'a> {
    pub size: (f64, f64),
    pub doc: &'a Doc,
    pub layout: &'a Layout,
    pub panes: &'a [(Pane, Viewport)],
    /// A cell is being edited: no fill handle.
    pub editing: bool,
    /// The system is in dark mode.
    pub dark: bool,
    /// The sheet's floating objects, and each chart's primitives.
    pub scene: &'a Scene,
    /// A header edge being dragged: (columns, position), drawn as a guide line.
    pub guide: Option<(bool, f32)>,
    /// Redraws the page (a picture finished loading).
    pub repaint: &'a Rc<dyn Fn()>,
    /// The selected floating object, which gets a frame with handles.
    pub selected: Option<ObjectRef>,
    /// What a drag in progress would do, outlined.
    pub preview: Option<Preview>,
}

/// A drag in progress, outlined with a dashed line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Preview {
    /// Where an object would be (view points).
    Rect(Rect),
    /// The cells a fill would cover.
    Range(RangeRef),
}

/// The floating objects of the active sheet as they are drawn.
#[derive(Default)]
pub struct Scene {
    pub objects: Vec<ObjectView>,
    pub charts: HashMap<u32, Vec<Prim>>,
}

impl Scene {
    pub fn of(doc: &Doc) -> Scene {
        let objects = doc.objects();
        let charts = objects
            .iter()
            .filter(|o| o.kind == ObjectKind::Chart)
            .filter_map(|o| doc.chart_prims(o.id, &crate::measure::ChartMeasure).map(|p| (o.id, p)))
            .collect();
        Scene { objects, charts }
    }
}

pub fn paint(ctx: &Ctx, f: &Frame) {
    let Some(sheet) = f.doc.sheet() else { return };
    let geo = &f.layout.geo;
    ctx.set_fill_style_str(SHEET);
    fill_rect(ctx, 0.0, 0.0, f.size.0, f.size.1);
    for (pane, viewport) in f.panes {
        ctx.save();
        clip(ctx, f64::from(pane.rect.x), f64::from(pane.rect.y), f64::from(pane.rect.w), f64::from(pane.rect.h));
        if sheet.show_gridlines {
            gridlines(ctx, pane, geo);
        }
        cells(ctx, viewport, sheet, geo, f.layout.scale);
        selection(ctx, f, sheet, pane);
        ctx.restore();
    }
    objects(ctx, f);
    frozen_lines(ctx, geo, f.size);
    if f.layout.headings {
        headers(ctx, f);
    }
    if let Some(preview) = f.preview {
        let r = match preview {
            Preview::Rect(r) => r,
            Preview::Range(range) => geo.range_rect(sheet, range),
        };
        ctx.save();
        let cells = geo.cells();
        clip(ctx, f64::from(cells.x), f64::from(cells.y), f64::from(cells.w), f64::from(cells.h));
        ctx.set_stroke_style_str(SECONDARY);
        ctx.set_line_width(1.5);
        dash(ctx, &[4.0, 3.0]);
        ctx.stroke_rect(f64::from(r.x) + 0.5, f64::from(r.y) + 0.5, f64::from(r.w) - 1.0, f64::from(r.h) - 1.0);
        ctx.restore();
    }
    if let Some((columns, at)) = f.guide {
        let at = f64::from(at);
        ctx.set_stroke_style_str(ACCENT);
        ctx.set_line_width(1.0);
        dash(ctx, &[4.0, 3.0]);
        if columns {
            line(ctx, (at, 0.0), (at, f.size.1));
        } else {
            line(ctx, (0.0, at), (f.size.0, at));
        }
        dash(ctx, &[]);
    }
}

fn gridlines(ctx: &Ctx, pane: &Pane, geo: &Geometry) {
    let top = f64::from(geo.y(pane.rows.0));
    let bottom = f64::from(geo.y(pane.rows.1.saturating_add(1)));
    let left = f64::from(geo.x(pane.cols.0));
    let right = f64::from(geo.x(pane.cols.1.saturating_add(1)));
    ctx.set_stroke_style_str(GRIDLINE);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    let mut previous = f64::NEG_INFINITY;
    for col in pane.cols.0..=pane.cols.1.saturating_add(1) {
        let x = f64::from(geo.x(col)).round() - 0.5;
        // Hidden columns take no room: one line, not several on top of each other.
        if x == previous {
            continue;
        }
        previous = x;
        ctx.move_to(x, top);
        ctx.line_to(x, bottom);
    }
    previous = f64::NEG_INFINITY;
    for row in pane.rows.0..=pane.rows.1.saturating_add(1) {
        let y = f64::from(geo.y(row)).round() - 0.5;
        if y == previous {
            continue;
        }
        previous = y;
        ctx.move_to(left, y);
        ctx.line_to(right, y);
    }
    ctx.stroke();
}

fn cells(ctx: &Ctx, vp: &Viewport, sheet: &Sheet, geo: &Geometry, scale: f32) {
    let z = f64::from(scale);
    let rects: Vec<Rect> = vp.cells.iter().map(|c| geo.range_rect(sheet, c.span)).collect();
    // Fills; merged areas are blanked (no inner gridlines) and painted as one cell.
    for (cell, r) in vp.cells.iter().zip(&rects) {
        if let Some(fill) = cell.fill {
            ctx.set_fill_style_str(&rgb(fill));
            fill_rect(ctx, f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
        } else if cell.merged {
            ctx.set_fill_style_str(SHEET);
            fill_rect(ctx, f64::from(r.x) + 0.5, f64::from(r.y) + 0.5, f64::from(r.w) - 1.0, f64::from(r.h) - 1.0);
        }
    }
    // Decorations and text.
    for (cell, r) in vp.cells.iter().zip(&rects) {
        if let Some(bar) = &cell.data_bar {
            data_bar(ctx, bar, r);
        }
        if let Some(icon) = &cell.icon {
            let s = 12.0 * z;
            cf_icon(ctx, &icon.set, icon.index, f64::from(r.x) + 2.0, f64::from(r.y) + f64::from(r.h) / 2.0 - s / 2.0, s);
        }
        if let Some(checked) = cell.checkbox {
            checkbox(ctx, checked, r, z);
            continue;
        }
        if let Some(text) = &cell.text {
            draw_text(ctx, text, r, geo, z);
        }
    }
    // Borders over fills and text, then comment marks.
    for (cell, r) in vp.cells.iter().zip(&rects) {
        for seg in &cell.borders {
            border(ctx, seg, cell, r, geo);
        }
        if let Some(mark) = cell.comment {
            let s = 6.0 * z.max(0.7);
            let (right, top) = (f64::from(r.right()), f64::from(r.y));
            ctx.set_fill_style_str(if mark == CommentMark::Threaded { THREADED } else { NOTE });
            ctx.begin_path();
            ctx.move_to(right, top);
            ctx.line_to(right, top + s);
            ctx.line_to(right - s, top);
            ctx.close_path();
            ctx.fill();
        }
    }
    for b in &vp.filter_buttons {
        filter_button(ctx, b.active, &geo.filter_button(sheet, b.cell));
    }
}

fn data_bar(ctx: &Ctx, bar: &DataBar, r: &Rect) {
    let fraction = f64::from(bar.fraction.clamp(0.0, 1.0));
    let (x, y) = (f64::from(r.x) + 2.0, f64::from(r.y) + 2.0);
    let (w, h) = (((f64::from(r.w) - 4.0) * fraction).max(0.0), (f64::from(r.h) - 4.0).max(0.0));
    let color = rgb(bar.color);
    if bar.gradient {
        let light = rgb([0, 1, 2].map(|i| ((u16::from(bar.color[i]) + 765) / 4) as u8));
        let g = ctx.create_linear_gradient(x, 0.0, x + w, 0.0);
        let _ = g.add_color_stop(0.0, &color);
        let _ = g.add_color_stop(1.0, &light);
        ctx.set_fill_style_canvas_gradient(&g);
        fill_rect(ctx, x, y, w, h);
        ctx.set_stroke_style_str(&color);
        ctx.set_line_width(1.0);
        ctx.stroke_rect(x + 0.5, y + 0.5, (w - 1.0).max(0.0), (h - 1.0).max(0.0));
    } else {
        ctx.set_fill_style_str(&color);
        fill_rect(ctx, x, y, w, h);
    }
}

fn cf_icon(ctx: &Ctx, set: &str, index: usize, x: f64, y: f64, s: f64) {
    // The engine gives the set's name and the icon's rank; the set's size isn't in the model,
    // so the rank is shown against the common three-icon sets.
    let n = if set.contains('5') || set.to_lowercase().contains("five") {
        5
    } else if set.contains('4') || set.to_lowercase().contains("four") {
        4
    } else {
        3
    };
    let i = index.min(n - 1);
    let t = i as f64 / (n - 1).max(1) as f64;
    let rgb_c = if n <= 3 {
        ["#d13b2f", "#f2b82e", "#2e9e4f"].get(i).copied().unwrap_or("#808080").to_string()
    } else {
        format!("rgb({},{},{})", (209.0 * (1.0 - t) + 46.0 * t) as u8, (59.0 * (1.0 - t) + 158.0 * t) as u8, (47.0 * (1.0 - t) + 79.0 * t) as u8)
    };
    let (cx, cy, h) = (x + s / 2.0, y + s / 2.0, s / 2.0);
    ctx.set_fill_style_str(&rgb_c);
    if set.to_lowercase().contains("arrow") {
        ctx.begin_path();
        if i + 1 == n {
            ctx.move_to(cx, cy - h);
            ctx.line_to(cx + h, cy + 0.1 * h);
            ctx.line_to(cx - h, cy + 0.1 * h);
        } else if i == 0 {
            ctx.move_to(cx, cy + h);
            ctx.line_to(cx - h, cy - 0.1 * h);
            ctx.line_to(cx + h, cy - 0.1 * h);
        } else {
            ctx.move_to(cx + h, cy);
            ctx.line_to(cx - 0.1 * h, cy - h);
            ctx.line_to(cx - 0.1 * h, cy + h);
        }
        ctx.close_path();
        ctx.fill();
    } else {
        ctx.begin_path();
        let _ = ctx.arc(cx, cy, h - 0.5, 0.0, std::f64::consts::TAU);
        ctx.fill();
    }
}

fn round_rect(ctx: &Ctx, x: f64, y: f64, w: f64, h: f64, r: f64) {
    ctx.begin_path();
    let _ = ctx.round_rect_with_f64(x, y, w, h, r);
}

fn checkbox(ctx: &Ctx, checked: bool, r: &Rect, z: f64) {
    let s = (11.0 * z * 1.2).clamp(8.0, 24.0);
    let (x, y) = (f64::from(r.x) + f64::from(r.w) / 2.0 - s / 2.0, f64::from(r.y) + f64::from(r.h) / 2.0 - s / 2.0);
    round_rect(ctx, x, y, s, s, 2.0);
    ctx.set_fill_style_str(if checked { CHECKBOX } else { SHEET });
    ctx.fill();
    ctx.set_stroke_style_str(if checked { CHECKBOX } else { SECONDARY });
    ctx.set_line_width(1.0);
    ctx.stroke();
    if checked {
        ctx.set_stroke_style_str("#ffffff");
        ctx.set_line_width((s / 8.0).max(1.5));
        ctx.begin_path();
        ctx.move_to(x + s * 0.25, y + s * 0.5);
        ctx.line_to(x + s * 0.43, y + s * 0.72);
        ctx.line_to(x + s * 0.78, y + s * 0.28);
        ctx.stroke();
    }
}

fn border(ctx: &Ctx, b: &BorderSeg, cell: &CellView, r: &Rect, geo: &Geometry) {
    let (rx, ry, rr, rb) = (f64::from(r.x), f64::from(r.y), f64::from(r.right()), f64::from(r.bottom()));
    // A merged area's edges come in segments, one per visible cell along them.
    let (x0, x1) = if cell.merged { (f64::from(geo.x(b.from)), f64::from(geo.x(b.to.saturating_add(1)))) } else { (rx, rr) };
    let (y0, y1) = if cell.merged { (f64::from(geo.y(b.from)), f64::from(geo.y(b.to.saturating_add(1)))) } else { (ry, rb) };
    let (a, e) = match b.edge {
        Edge::Top => ((x0, ry), (x1, ry)),
        Edge::Bottom => ((x0, rb), (x1, rb)),
        Edge::Left => ((rx, y0), (rx, y1)),
        Edge::Right => ((rr, y0), (rr, y1)),
        Edge::DiagDown => ((rx, ry), (rr, rb)),
        Edge::DiagUp => ((rx, rb), (rr, ry)),
    };
    ctx.save();
    ctx.set_stroke_style_str(&rgb(b.color));
    ctx.set_line_width(f64::from(b.style.width()));
    match b.style {
        BorderStyle::None => {}
        BorderStyle::Double => {
            let off = if (a.1 - e.1).abs() < 0.5 { (0.0, 1.5) } else { (1.5, 0.0) };
            ctx.set_line_width(1.0);
            line(ctx, (a.0 - off.0, a.1 - off.1), (e.0 - off.0, e.1 - off.1));
            line(ctx, (a.0 + off.0, a.1 + off.1), (e.0 + off.0, e.1 + off.1));
        }
        style => {
            match style {
                BorderStyle::Dashed | BorderStyle::MediumDashed => dash(ctx, &[4.0, 2.0]),
                BorderStyle::DashDot | BorderStyle::MediumDashDot | BorderStyle::SlantDashDot => dash(ctx, &[4.0, 1.5, 1.0, 1.5]),
                BorderStyle::DashDotDot | BorderStyle::MediumDashDotDot => dash(ctx, &[4.0, 1.5, 1.0, 1.5, 1.0, 1.5]),
                BorderStyle::Dotted | BorderStyle::Hair => dash(ctx, &[1.0, 1.0]),
                _ => {}
            }
            line(ctx, a, e);
        }
    }
    ctx.restore();
}

fn filter_button(ctx: &Ctx, active: bool, r: &Rect) {
    if r.w <= 0.0 {
        return;
    }
    let (x, y, w, h) = (f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
    round_rect(ctx, x, y, w, h, 2.0);
    ctx.set_fill_style_str("#fafafa");
    ctx.fill();
    ctx.set_stroke_style_str("#ababab");
    ctx.set_line_width(1.0);
    ctx.stroke();
    ctx.set_stroke_style_str("#454545");
    ctx.set_line_width(1.2);
    let (cx, cy, s) = (x + w / 2.0, y + h / 2.0, w * 0.22);
    ctx.begin_path();
    if active {
        // A funnel.
        ctx.move_to(cx - 1.6 * s, cy - s);
        ctx.line_to(cx + 1.6 * s, cy - s);
        ctx.line_to(cx + 0.3 * s, cy + 0.2 * s);
        ctx.line_to(cx + 0.3 * s, cy + 1.3 * s);
        ctx.line_to(cx - 0.3 * s, cy + 1.3 * s);
        ctx.line_to(cx - 0.3 * s, cy + 0.2 * s);
        ctx.close_path();
    } else {
        ctx.move_to(cx - 1.4 * s, cy - 0.6 * s);
        ctx.line_to(cx, cy + 0.7 * s);
        ctx.line_to(cx + 1.4 * s, cy - 0.6 * s);
    }
    ctx.stroke();
}

fn text_width(ctx: &Ctx, s: &str) -> f64 {
    ctx.measure_text(s).map(|m| m.width()).unwrap_or(0.0)
}

/// `text` broken into lines `width` wide (and at its line breaks).
fn wrapped_lines(ctx: &Ctx, text: &str, width: f64) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split(' ') {
            let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
            if current.is_empty() || text_width(ctx, &candidate) <= width {
                current = candidate;
            } else {
                out.push(std::mem::take(&mut current));
                current = word.to_string();
            }
            // A word wider than the cell breaks anywhere.
            while text_width(ctx, &current) > width && current.chars().count() > 1 {
                let mut cut = current.chars().count() - 1;
                while cut > 1 && text_width(ctx, &current.chars().take(cut).collect::<String>()) > width {
                    cut -= 1;
                }
                out.push(current.chars().take(cut).collect());
                current = current.chars().skip(cut).collect();
            }
        }
        out.push(current);
        if out.len() > 2000 {
            break;
        }
    }
    out
}

fn line_top(valign: VAlign, area: &Rect, height: f64, z: f64, top_pad: f64, bottom_pad: f64) -> f64 {
    let top = f64::from(area.y) + top_pad * z;
    let bottom = top.max(f64::from(area.bottom()) - height - bottom_pad * z);
    match valign {
        VAlign::Top => top,
        VAlign::Center | VAlign::Justify | VAlign::Distributed => (top + bottom) / 2.0,
        VAlign::Bottom => bottom,
    }
}

fn draw_text(ctx: &Ctx, t: &CellText, r: &Rect, geo: &Geometry, z: f64) {
    let size = f64::from(t.size);
    let color = rgb(t.color);
    // The content area stops before a filter button.
    let area = Rect { x: r.x, y: r.y, w: t.right.max(0.0), h: r.h };
    let (ascent, descent) = (size * 0.8, size * 0.2);
    ctx.save();
    ctx.set_font(&font_css(&t.font, t.size));
    ctx.set_fill_style_str(&color);
    ctx.set_text_baseline("alphabetic");
    ctx.set_text_align("left");
    match t.layout {
        TextLayout::Wrap { width, anchor } => {
            clip(ctx, f64::from(area.x), f64::from(area.y), f64::from(area.w), f64::from(area.h));
            let lines = wrapped_lines(ctx, &t.text, f64::from(width));
            let lh = size * 1.2;
            let mut y = line_top(t.align.v, &area, lh * lines.len() as f64, z, 1.5, 1.5);
            for l in &lines {
                let w = text_width(ctx, l);
                let x = f64::from(r.x)
                    + match t.halign {
                        HAlign::Center | HAlign::CenterAcross => f64::from(anchor) - w / 2.0,
                        HAlign::Right => f64::from(anchor) - w,
                        _ => f64::from(anchor),
                    };
                let _ = ctx.fill_text(l, x, y + ascent);
                y += lh;
            }
        }
        TextLayout::Rotated { .. } if t.align.rotation != 0 && t.align.rotation != 255 => {
            clip(ctx, f64::from(area.x), f64::from(area.y), f64::from(area.w), f64::from(area.h));
            let angle = f64::from(t.align.rotation).to_radians();
            let w = text_width(ctx, &t.text);
            let _ = ctx.translate(f64::from(area.x) + f64::from(area.w) / 2.0, f64::from(area.y) + f64::from(area.h) / 2.0);
            let _ = ctx.rotate(-angle);
            let _ = ctx.fill_text(&t.text, -w / 2.0, (ascent - descent) / 2.0);
        }
        _ => {
            let (cx, cy, cw, ch) = match t.spill {
                Some((first, last)) => {
                    let left = f64::from(geo.x(first));
                    (left, f64::from(r.y), f64::from(geo.x(last.saturating_add(1))) - left, f64::from(r.h))
                }
                None => (f64::from(area.x), f64::from(area.y), f64::from(area.w), f64::from(area.h)),
            };
            if t.cover_gridlines {
                // Text overflowing into empty neighbours covers the gridlines it crosses.
                ctx.set_fill_style_str(SHEET);
                fill_rect(ctx, cx + 1.0, cy + 0.5, (cw - 2.0).max(0.0), (ch - 1.5).max(0.0));
                ctx.set_fill_style_str(&color);
            }
            clip(ctx, cx, cy, cw, ch);
            let x = f64::from(r.x) + f64::from(t.x);
            let baseline = line_top(t.align.v, &area, ascent + descent, z, 1.0, 1.5) + ascent;
            let _ = ctx.fill_text(&t.text, x, baseline);
            let w = f64::from(t.width);
            ctx.set_stroke_style_str(&color);
            ctx.set_line_width(1.0);
            match t.font.underline {
                Underline::None => {}
                Underline::Double | Underline::DoubleAccounting => {
                    line(ctx, (x, baseline + 1.5), (x + w, baseline + 1.5));
                    line(ctx, (x, baseline + 3.5), (x + w, baseline + 3.5));
                }
                _ => line(ctx, (x, baseline + 1.5), (x + w, baseline + 1.5)),
            }
            if t.font.strike {
                let y = baseline - ascent * 0.32;
                line(ctx, (x, y), (x + w, y));
            }
        }
    }
    ctx.restore();
}

fn selection(ctx: &Ctx, f: &Frame, sheet: &Sheet, pane: &Pane) {
    let Some(sel) = f.doc.selection() else { return };
    let geo = &f.layout.geo;
    ctx.set_fill_style_str(SELECTION_FILL);
    for range in sel.ranges.iter().filter(|r| !r.is_single()) {
        let r = geo.range_rect(sheet, *range);
        fill_rect(ctx, f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
    }
    let current = sel.current();
    let r = geo.range_rect(sheet, current);
    ctx.set_stroke_style_str(ACCENT);
    ctx.set_line_width(2.0);
    ctx.stroke_rect(f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
    // The fill handle, in the pane that shows the selection's corner.
    if !f.editing && geo.range_panes(gridcraft_engine::core::RangeRef::cell(current.end)).contains(&pane.rect) {
        let h = geo.fill_handle(sheet, sel);
        ctx.set_fill_style_str(ACCENT);
        fill_rect(ctx, f64::from(h.x) + 1.5, f64::from(h.y) + 1.5, f64::from(h.w) - 3.0, f64::from(h.h) - 3.0);
    }
}

fn frozen_lines(ctx: &Ctx, geo: &Geometry, size: (f64, f64)) {
    let (fr, fc) = geo.frozen;
    ctx.set_stroke_style_str(FROZEN_LINE);
    ctx.set_line_width(1.0);
    let (left, top) = (f64::from(geo.origin.0 + geo.frozen_w), f64::from(geo.origin.1 + geo.frozen_h));
    if fr > 0 {
        line(ctx, (f64::from(geo.origin.0), top - 0.5), (size.0, top - 0.5));
    }
    if fc > 0 {
        line(ctx, (left - 0.5, f64::from(geo.origin.1)), (left - 0.5, size.1));
    }
}

fn headers(ctx: &Ctx, f: &Frame) {
    let Some(sheet) = f.doc.sheet() else { return };
    let geo = &f.layout.geo;
    let (hw, hh) = (f64::from(f.layout.header_w), f64::from(f.layout.header_h));
    let ch = if f.dark { &DARK } else { &LIGHT };
    let current = f.doc.selection().map(|s| s.current());
    ctx.set_fill_style_str(ch.header);
    fill_rect(ctx, 0.0, 0.0, f.size.0, hh);
    fill_rect(ctx, 0.0, 0.0, hw, f.size.1);
    ctx.set_font(&format!("{:.1}px {UI_FONT}", 12.0 * (f.layout.scale / f.doc.display_scale).max(0.75)));
    ctx.set_text_baseline("middle");
    ctx.set_text_align("center");
    ctx.set_line_width(1.0);
    let mut cols: Vec<u32> = Vec::new();
    let mut rows: Vec<u32> = Vec::new();
    for (pane, _) in f.panes {
        cols.extend(pane.cols.0..=pane.cols.1);
        rows.extend(pane.rows.0..=pane.rows.1);
    }
    cols.sort_unstable();
    cols.dedup();
    rows.sort_unstable();
    rows.dedup();
    for col in cols {
        let c = geo.cell_rect(sheet, gridcraft_engine::core::CellRef::new(0, col));
        let (x0, x1) = (f64::from(c.x), f64::from(c.right()));
        if x1 <= x0 || x1 <= hw || x0 >= f.size.0 {
            continue;
        }
        let left = x0.max(hw);
        if current.is_some_and(|r| col >= r.start.col && col <= r.end.col) {
            ctx.set_fill_style_str(ch.selected);
            fill_rect(ctx, left, 0.0, x1 - left, hh);
        }
        ctx.set_stroke_style_str(ch.line);
        line(ctx, (x1 - 0.5, 0.0), (x1 - 0.5, hh));
        let label = column_label(col);
        if x1 - x0 > text_width(ctx, &label) {
            ctx.save();
            clip(ctx, left, 0.0, x1 - left, hh);
            ctx.set_fill_style_str(ch.text);
            let _ = ctx.fill_text(&label, (x0 + x1) / 2.0, hh / 2.0 + 0.5);
            ctx.restore();
        }
    }
    for row in rows {
        let c = geo.cell_rect(sheet, gridcraft_engine::core::CellRef::new(row, 0));
        let (y0, y1) = (f64::from(c.y), f64::from(c.bottom()));
        if y1 <= y0 || y1 <= hh || y0 >= f.size.1 {
            continue;
        }
        let top = y0.max(hh);
        if current.is_some_and(|r| row >= r.start.row && row <= r.end.row) {
            ctx.set_fill_style_str(ch.selected);
            fill_rect(ctx, 0.0, top, hw, y1 - top);
        }
        ctx.set_stroke_style_str(ch.line);
        line(ctx, (0.0, y1 - 0.5), (hw, y1 - 0.5));
        if y1 - y0 > 9.0 {
            ctx.save();
            clip(ctx, 0.0, top, hw, y1 - top);
            ctx.set_fill_style_str(ch.text);
            let _ = ctx.fill_text(&(row + 1).to_string(), hw / 2.0, (y0 + y1) / 2.0 + 0.5);
            ctx.restore();
        }
    }
    ctx.set_stroke_style_str(ch.line);
    line(ctx, (0.0, hh - 0.5), (f.size.0, hh - 0.5));
    line(ctx, (hw - 0.5, 0.0), (hw - 0.5, f.size.1));
    ctx.set_fill_style_str(ch.header);
    fill_rect(ctx, 0.0, 0.0, hw - 1.0, hh - 1.0);
}

// MARK: - Floating objects

thread_local! {
    /// Decoded pictures by object id and byte count (`None` while loading or when undecodable).
    static IMAGES: RefCell<HashMap<(u32, usize), Option<web_sys::HtmlImageElement>>> = RefCell::new(HashMap::new());
}

fn objects(ctx: &Ctx, f: &Frame) {
    if f.scene.objects.is_empty() {
        return;
    }
    let geo = &f.layout.geo;
    let cells = geo.cells();
    ctx.save();
    clip(ctx, f64::from(cells.x), f64::from(cells.y), f64::from(cells.w), f64::from(cells.h));
    let z = f64::from(f.layout.scale);
    for o in &f.scene.objects {
        let r = geo.anchor_rect(&o.anchor);
        if r.right() < cells.x || r.x > cells.right() || r.bottom() < cells.y || r.y > cells.bottom() {
            continue;
        }
        match &o.kind {
            ObjectKind::Shape(shape) => draw_shape(ctx, shape, &r, z),
            ObjectKind::Picture { .. } => draw_picture(ctx, f, o.id, &r),
            ObjectKind::Chart => {
                // A white card with a soft shadow, then the chart's primitives.
                ctx.save();
                ctx.set_shadow_color("rgba(0, 0, 0, 0.25)");
                ctx.set_shadow_blur(8.0);
                ctx.set_shadow_offset_y(1.5);
                ctx.set_fill_style_str(SHEET);
                fill_rect(ctx, f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
                ctx.restore();
                if let Some(prims) = f.scene.charts.get(&o.id) {
                    draw_prims(ctx, prims, (f64::from(r.x), f64::from(r.y)), z);
                }
            }
            ObjectKind::Slicer { caption } => {
                ctx.set_fill_style_str(SHEET);
                fill_rect(ctx, f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
                ctx.set_stroke_style_str("#ababab");
                ctx.set_line_width(1.0);
                ctx.stroke_rect(f64::from(r.x) + 0.5, f64::from(r.y) + 0.5, f64::from(r.w) - 1.0, f64::from(r.h) - 1.0);
                ctx.set_fill_style_str("#333333");
                ctx.set_font(&format!("600 {:.1}px {UI_FONT}", 12.0 * z));
                ctx.set_text_baseline("top");
                ctx.set_text_align("left");
                let _ = ctx.fill_text(caption, f64::from(r.x) + 6.0 * z, f64::from(r.y) + 5.0 * z);
            }
        }
        if f.selected == Some(ObjectRef { kind: kind_name(&o.kind), id: o.id }) {
            selection_frame(ctx, &r);
        }
    }
    ctx.restore();
}

/// The frame of the selected object, with a round handle at each corner.
fn selection_frame(ctx: &Ctx, r: &Rect) {
    ctx.set_stroke_style_str(ACCENT);
    ctx.set_line_width(1.5);
    ctx.stroke_rect(f64::from(r.x) - 1.0, f64::from(r.y) - 1.0, f64::from(r.w) + 2.0, f64::from(r.h) + 2.0);
    for (x, y) in [(r.x, r.y), (r.right(), r.y), (r.x, r.bottom()), (r.right(), r.bottom())] {
        ctx.begin_path();
        let _ = ctx.arc(f64::from(x), f64::from(y), 4.0, 0.0, std::f64::consts::TAU);
        ctx.set_fill_style_str("#ffffff");
        ctx.fill();
        ctx.stroke();
    }
}

fn draw_picture(ctx: &Ctx, f: &Frame, id: u32, r: &Rect) {
    let Some(bytes) = f.doc.picture(id) else { return };
    let key = (id, bytes.len());
    let known = IMAGES.with(|c| c.borrow().get(&key).cloned());
    match known {
        Some(Some(img)) => {
            if img.complete() && img.natural_width() > 0 {
                let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(&img, f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
                return;
            }
        }
        Some(None) => {}
        None => load_picture(key, &bytes, f.scene.objects.iter().find(|o| o.id == id).map(|o| &o.kind), f.repaint),
    }
    ctx.set_fill_style_str("#e6e6e6");
    fill_rect(ctx, f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
}

fn load_picture(key: (u32, usize), bytes: &[u8], kind: Option<&ObjectKind>, repaint: &Rc<dyn Fn()>) {
    let mime = match kind {
        Some(ObjectKind::Picture { mime, .. }) => mime.as_str(),
        _ => "image/png",
    };
    let mime = if mime == "image/jpeg" { "image/jpeg" } else { "image/png" };
    let image = web_sys::HtmlImageElement::new().ok();
    let url = (|| {
        let array = js_sys::Uint8Array::from(bytes);
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&js_sys::Array::of1(&array), &opts).ok()?;
        web_sys::Url::create_object_url_with_blob(&blob).ok()
    })();
    let (Some(image), Some(url)) = (image, url) else {
        IMAGES.with(|c| c.borrow_mut().insert(key, None));
        return;
    };
    let again = repaint.clone();
    let revoke = url.clone();
    let on_load = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
        let _ = web_sys::Url::revoke_object_url(&revoke);
        again();
    });
    image.set_onload(Some(on_load.as_ref().unchecked_ref()));
    on_load.forget();
    image.set_src(&url);
    IMAGES.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 64 {
            c.clear();
        }
        c.insert(key, Some(image));
    });
}

fn draw_shape(ctx: &Ctx, s: &ShapeView, r: &Rect, z: f64) {
    let (x, y, w, h) = (f64::from(r.x), f64::from(r.y), f64::from(r.w), f64::from(r.h));
    let fill = s.fill.map(rgb);
    let stroke = s.line.map(rgb);
    ctx.save();
    ctx.begin_path();
    match s.shape {
        ShapeKind::Rectangle | ShapeKind::TextBox => ctx.rect(x, y, w, h),
        ShapeKind::RoundedRectangle => {
            let _ = ctx.round_rect_with_f64(x, y, w, h, (10.0 * z).min(w / 2.0).min(h / 2.0));
        }
        ShapeKind::Ellipse => {
            let _ = ctx.ellipse(x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0, 0.0, 0.0, std::f64::consts::TAU);
        }
        ShapeKind::Triangle => {
            ctx.move_to(x + w / 2.0, y);
            ctx.line_to(x + w, y + h);
            ctx.line_to(x, y + h);
            ctx.close_path();
        }
        ShapeKind::Line | ShapeKind::Arrow => {
            let color = stroke.unwrap_or_else(|| "#404040".into());
            ctx.set_stroke_style_str(&color);
            ctx.set_line_width(2.0);
            line(ctx, (x, y), (x + w, y + h));
            if s.shape == ShapeKind::Arrow {
                let angle = h.atan2(w);
                let (tip, back) = ((x + w, y + h), (x + w - 10.0 * angle.cos(), y + h - 10.0 * angle.sin()));
                let n = (-angle.sin() * 5.0, angle.cos() * 5.0);
                ctx.set_fill_style_str(&color);
                ctx.begin_path();
                ctx.move_to(tip.0, tip.1);
                ctx.line_to(back.0 + n.0, back.1 + n.1);
                ctx.line_to(back.0 - n.0, back.1 - n.1);
                ctx.close_path();
                ctx.fill();
            }
            ctx.restore();
            return;
        }
        ShapeKind::Ink => {
            if let Some((points, width)) = &s.ink
                && points.len() >= 2
            {
                ctx.set_stroke_style_str(&stroke.unwrap_or_else(|| "#000000".into()));
                ctx.set_line_width(f64::from(*width) * z);
                ctx.set_line_cap("round");
                ctx.set_line_join("round");
                ctx.begin_path();
                for (i, p) in points.iter().enumerate() {
                    let (px, py) = (x + f64::from(p[0]) * z, y + f64::from(p[1]) * z);
                    if i == 0 {
                        ctx.move_to(px, py);
                    } else {
                        ctx.line_to(px, py);
                    }
                }
                ctx.stroke();
            }
            ctx.restore();
            return;
        }
        ShapeKind::Icon => {
            // GridCraft's own icon set is drawn in code in egui (icons.rs); until it is ported,
            // a framed placeholder with the icon's name.
            ctx.set_stroke_style_str("#808080");
            ctx.stroke_rect(x + 0.5, y + 0.5, w - 1.0, h - 1.0);
            ctx.set_fill_style_str("#666666");
            ctx.set_font(&format!("10px {UI_FONT}"));
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            let _ = ctx.fill_text(s.icon.as_deref().unwrap_or(""), x + w / 2.0, y + h / 2.0);
            ctx.restore();
            return;
        }
    }
    if let Some(fill) = &fill {
        ctx.set_fill_style_str(fill);
        ctx.fill();
    }
    if let Some(stroke) = &stroke {
        ctx.set_stroke_style_str(stroke);
        ctx.set_line_width(1.0);
        ctx.stroke();
    }
    if !s.text.is_empty() {
        let size = f64::from(s.text_size) * z;
        let family = s.font.as_deref().map(|f| f.replace('"', "")).unwrap_or_else(|| "system-ui".into());
        ctx.set_font(&format!(
            "{}{}{:.1}px \"{family}\", {UI_FONT}",
            if s.italic { "italic " } else { "" },
            if s.bold { "bold " } else { "" },
            size.max(1.0)
        ));
        ctx.set_fill_style_str(&rgb(s.text_color));
        ctx.set_text_baseline("alphabetic");
        ctx.set_text_align("left");
        let (inner_x, inner_w) = (x + 4.0, (w - 8.0).max(1.0));
        let lines = wrapped_lines(ctx, &s.text, inner_w);
        let lh = size * 1.2;
        let total = lh * lines.len() as f64;
        let top = match s.anchor {
            ShapeTextAnchor::Top => y + 4.0,
            ShapeTextAnchor::Middle => y + h / 2.0 - total / 2.0,
            ShapeTextAnchor::Bottom => y + h - 4.0 - total,
        };
        ctx.begin_path();
        ctx.rect(x, y, w, h);
        ctx.clip();
        for (i, l) in lines.iter().enumerate() {
            let lw = text_width(ctx, l);
            let lx = match s.align {
                ShapeTextAlign::Left => inner_x,
                ShapeTextAlign::Center => inner_x + (inner_w - lw) / 2.0,
                ShapeTextAlign::Right => inner_x + inner_w - lw,
            };
            let _ = ctx.fill_text(l, lx, top + lh * i as f64 + size * 0.85);
        }
    }
    ctx.restore();
}

fn rgba(c: [u8; 4]) -> String {
    format!("rgba({},{},{},{:.3})", c[0], c[1], c[2], f32::from(c[3]) / 255.0)
}

/// Chart primitives from `origin`, `k` screen units per chart unit.
fn draw_prims(ctx: &Ctx, prims: &[Prim], origin: (f64, f64), k: f64) {
    let pt = |x: f32, y: f32| (origin.0 + f64::from(x) * k, origin.1 + f64::from(y) * k);
    let stroke_width = |w: f32| (f64::from(w) * k).max(0.5);
    for prim in prims {
        match prim {
            Prim::Rect { x, y, w, h, fill, stroke, radius } => {
                let (px, py) = pt(*x, *y);
                let (pw, ph) = (f64::from(*w) * k, f64::from(*h) * k);
                ctx.begin_path();
                let _ = ctx.round_rect_with_f64(px, py, pw, ph, (f64::from(*radius) * k).min(pw / 2.0).min(ph / 2.0).max(0.0));
                if let Some(c) = fill {
                    ctx.set_fill_style_str(&rgba(*c));
                    ctx.fill();
                }
                if let Some((c, w)) = stroke {
                    ctx.set_stroke_style_str(&rgba(*c));
                    ctx.set_line_width(stroke_width(*w));
                    ctx.stroke();
                }
            }
            Prim::Line { pts, color, width, dash: dashed } => {
                ctx.save();
                if *dashed {
                    dash(ctx, &[3.0 * k, 2.0 * k]);
                }
                ctx.begin_path();
                for (i, p) in pts.iter().enumerate() {
                    let (px, py) = pt(p[0], p[1]);
                    if i == 0 {
                        ctx.move_to(px, py);
                    } else {
                        ctx.line_to(px, py);
                    }
                }
                ctx.set_stroke_style_str(&rgba(*color));
                ctx.set_line_width(stroke_width(*width));
                ctx.stroke();
                ctx.restore();
            }
            Prim::Polygon { pts, fill, stroke } => {
                ctx.begin_path();
                for (i, p) in pts.iter().enumerate() {
                    let (px, py) = pt(p[0], p[1]);
                    if i == 0 {
                        ctx.move_to(px, py);
                    } else {
                        ctx.line_to(px, py);
                    }
                }
                ctx.close_path();
                ctx.set_fill_style_str(&rgba(*fill));
                ctx.fill();
                if let Some((c, w)) = stroke {
                    ctx.set_stroke_style_str(&rgba(*c));
                    ctx.set_line_width(stroke_width(*w));
                    ctx.stroke();
                }
            }
            Prim::Wedge { cx, cy, r_outer, r_inner, a0, a1, fill, stroke } => {
                // 0 rad at 12 o'clock, clockwise.
                let (c, ro, ri) = (pt(*cx, *cy), f64::from(*r_outer) * k, f64::from(*r_inner) * k);
                let (start, end) = (f64::from(*a0) - std::f64::consts::FRAC_PI_2, f64::from(*a1) - std::f64::consts::FRAC_PI_2);
                ctx.begin_path();
                let _ = ctx.arc(c.0, c.1, ro, start, end);
                if ri > 0.0 {
                    let _ = ctx.arc_with_anticlockwise(c.0, c.1, ri, end, start, true);
                } else {
                    ctx.line_to(c.0, c.1);
                }
                ctx.close_path();
                ctx.set_fill_style_str(&rgba(*fill));
                ctx.fill();
                if let Some((col, w)) = stroke {
                    ctx.set_stroke_style_str(&rgba(*col));
                    ctx.set_line_width(stroke_width(*w));
                    ctx.stroke();
                }
            }
            Prim::Circle { cx, cy, r, fill, stroke } => {
                let c = pt(*cx, *cy);
                ctx.begin_path();
                let _ = ctx.arc(c.0, c.1, f64::from(*r) * k, 0.0, std::f64::consts::TAU);
                if let Some(col) = fill {
                    ctx.set_fill_style_str(&rgba(*col));
                    ctx.fill();
                }
                if let Some((col, w)) = stroke {
                    ctx.set_stroke_style_str(&rgba(*col));
                    ctx.set_line_width(stroke_width(*w));
                    ctx.stroke();
                }
            }
            Prim::Text { x, y, text, size, color, bold, align, valign, rotation } => {
                ctx.save();
                let a = pt(*x, *y);
                let _ = ctx.translate(a.0, a.1);
                let _ = ctx.rotate(if rotation.is_finite() { f64::from(*rotation) } else { 0.0 });
                ctx.set_font(&format!("{}{:.2}px {UI_FONT}", if *bold { "bold " } else { "" }, (f64::from(*size) * k).max(1.0)));
                ctx.set_fill_style_str(&rgba(*color));
                ctx.set_text_align(match align {
                    PHAlign::Left => "left",
                    PHAlign::Center => "center",
                    PHAlign::Right => "right",
                });
                ctx.set_text_baseline(match valign {
                    PVAlign::Top => "top",
                    PVAlign::Middle => "middle",
                    PVAlign::Bottom => "bottom",
                });
                let _ = ctx.fill_text(text, 0.0, 0.0);
                ctx.restore();
            }
        }
    }
}
