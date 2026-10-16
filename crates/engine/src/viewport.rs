//! The viewport read model: everything a frontend needs to paint a block of cells, with Excel's
//! appearance rules already applied: number formats, General narrowing, `####`, Shrink to fit,
//! repeat fills, overflow into empty neighbours, Center Across Selection, merges, table styles,
//! conditional formats (fills, fonts, data bars, icons), borders, checkboxes, comment marks,
//! sparklines and filter buttons.
//!
//! The frontend only places and draws: it supplies text widths through [`Measure`] (the one
//! thing the engine can't know) and maps columns and rows to screen positions. Lengths in the
//! result are in the frontend's screen units, [`ViewportRequest::scale`] of them per sheet unit,
//! relative to the left edge of the cell (or merged area).

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_formula::FormulaLocale;
use gridcraft_model::{Alignment, BorderLine, BorderStyle, Color, Font, HAlign, PatternType, Sheet, SparklineKind, Style, Underline, Workbook};
use serde::Serialize;

use crate::cf::{CfCache, CfLook};

pub type Rgb = [u8; 3];

/// How many columns past the requested block overflowing text and Center Across Selection may
/// reach for empty neighbours.
pub const REACH: u32 = 20;
/// The most cells one request lays out; taller requests are cut short at the bottom.
pub const MAX_VIEWPORT_CELLS: u64 = 1 << 20;
/// Longest run of `#` or repeated fill characters one cell gets.
const MAX_REPEAT: usize = 8192;

/// Text measurement, supplied by the frontend.
pub trait Measure {
    /// Advance width of `text` on one line, in screen units, in `font`'s face (name, bold,
    /// italic) at `size` screen units (the zoomed font size; `font.size` is the unzoomed points).
    fn text_width(&self, text: &str, font: &Font, size: f32) -> f32;
}

/// Which cells to lay out, and how big they are on screen.
#[derive(Clone, Copy, Debug)]
pub struct ViewportRequest {
    pub sheet: usize,
    /// First and last row (inclusive).
    pub rows: (u32, u32),
    /// First and last column (inclusive).
    pub cols: (u32, u32),
    /// Screen units per sheet unit (zoom × display scale).
    pub scale: f32,
    /// Formula syntax shown when the sheet shows formulas.
    pub formula_locale: FormulaLocale,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Viewport {
    /// The rows and columns laid out (the request, clamped to the sheet).
    pub rows: (u32, u32),
    pub cols: (u32, u32),
    /// Cells with something to draw, row by row. A merged area appears once, at its first
    /// visible cell, with `cell` its top-left cell.
    pub cells: Vec<CellView>,
    pub sparklines: Vec<SparklineView>,
    pub filter_buttons: Vec<FilterButton>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CellView {
    pub cell: CellRef,
    /// The area painted: the merged area, or the cell.
    pub span: RangeRef,
    /// A merged area (painted blank, without inner gridlines, even without a fill).
    pub merged: bool,
    pub fill: Option<Rgb>,
    pub data_bar: Option<DataBar>,
    pub icon: Option<CfIcon>,
    /// A checkbox cell showing TRUE / FALSE (or empty: unchecked), drawn instead of text.
    pub checkbox: Option<bool>,
    /// The cell holds a picture (`Sheet::cell_pictures`), drawn instead of text.
    pub picture: bool,
    pub text: Option<CellText>,
    /// Border lines to draw over fills and text, in paint order.
    pub borders: Vec<BorderSeg>,
    pub comment: Option<CommentMark>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CellText {
    /// What to draw: one line (already narrowed, `####`ed or repeat-filled), or for wrapped and
    /// upright-wrapped text the whole text.
    pub text: String,
    /// The face, underline and strikethrough (the colour is `color`).
    pub font: Font,
    /// Font size in screen units (zoomed, and smaller under Shrink to fit).
    pub size: f32,
    pub color: Rgb,
    /// The horizontal alignment General resolves to.
    pub halign: HAlign,
    /// The cell's own alignment (vertical alignment, rotation, wrap, indent).
    pub align: Alignment,
    pub layout: TextLayout,
    /// The content area across the span: after a conditional-format icon, before a filter button.
    pub left: f32,
    pub right: f32,
    pub pad: f32,
    pub indent: f32,
    /// One line and rotated text: left edge of the text, and its width.
    pub x: f32,
    pub width: f32,
    /// Overflow or Center Across Selection: the columns (first, last) the text may be drawn over
    /// (clipped to), when that is more than its own cell.
    pub spill: Option<(u32, u32)>,
    /// With `spill`: the cell has no fill, so the text covers the gridlines it crosses with the
    /// sheet background.
    pub cover_gridlines: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum TextLayout {
    /// One line at `CellText::x`, clipped to the content area (or `spill`).
    Line,
    /// Wrapped to `width` and aligned by `halign` to `anchor`: the left edge, centre or right
    /// edge of the lines; clipped to the content area.
    Wrap { width: f32, anchor: f32 },
    /// Rotated by `align.rotation` degrees (counter-clockwise); upright (±90°) wrapped text wraps
    /// to `wrap` (the row height less padding).
    Rotated { wrap: Option<f32> },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DataBar {
    /// Share of the cell width, 0–1.
    pub fraction: f32,
    pub color: Rgb,
    pub gradient: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CfIcon {
    pub set: String,
    /// 0 = lowest.
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
    /// Top-left to bottom-right.
    DiagDown,
    /// Bottom-left to top-right.
    DiagUp,
}

/// A border line along part of a span's edge: columns `from..=to` for the top and bottom
/// edges, rows for the left and right edges; diagonals cross the whole span.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BorderSeg {
    pub edge: Edge,
    pub from: u32,
    pub to: u32,
    pub style: BorderStyle,
    pub color: Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CommentMark {
    Note,
    Threaded,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SparklineView {
    pub cell: CellRef,
    pub kind: SparklineKind,
    pub values: Vec<Option<f64>>,
    pub color: Rgb,
    pub markers: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FilterButton {
    pub cell: CellRef,
    /// The column is filtered.
    pub active: bool,
}

/// A cell's effective style (own style, table style, conditional formats) and fill.
#[derive(Clone, Debug, PartialEq)]
pub struct CellLook {
    pub style: Style,
    pub fill: Option<Rgb>,
    pub cf: Option<CfLook>,
}

/// The effective look of cell `c`; `cf` evaluates conditional formats (skipped without it).
pub fn cell_look(wb: &Workbook, si: usize, sh: &Sheet, c: CellRef, cf: Option<&mut CfCache>) -> CellLook {
    let mut style = match sh.table_at(c) {
        Some(t) => crate::tables::table_cell_style(wb, si, t, c),
        None => wb.styles.get(sh.style_id(c)).clone(),
    };
    let cfl = cf.and_then(|cache| cache.look(wb, si, c));
    if let Some(st) = cfl.as_ref().and_then(|l| l.style.as_ref()) {
        if st.fill.pattern != PatternType::None {
            style.fill = st.fill;
        }
        if st.font.color != Color::Auto {
            style.font.color = st.font.color;
        }
        style.font.bold |= st.font.bold;
        style.font.italic |= st.font.italic;
        style.font.strike |= st.font.strike;
        if st.font.underline != Underline::None {
            style.font.underline = st.font.underline;
        }
        for (dst, src) in [
            (&mut style.borders.left, st.borders.left),
            (&mut style.borders.right, st.borders.right),
            (&mut style.borders.top, st.borders.top),
            (&mut style.borders.bottom, st.borders.bottom),
        ] {
            if !src.is_none() {
                *dst = src;
            }
        }
    }
    let mut fill = match style.fill.pattern {
        PatternType::None => None,
        PatternType::Solid => style.fill.fg.resolve(&wb.theme),
        _ => {
            // Patterns: approximated by a blend of foreground and background.
            let fg = style.fill.fg.resolve(&wb.theme).unwrap_or([0, 0, 0]);
            let bg = style.fill.bg.resolve(&wb.theme).unwrap_or([255, 255, 255]);
            Some([0, 1, 2].map(|i| ((fg[i] as u16 + bg[i] as u16 * 3) / 4) as u8))
        }
    };
    if let Some(rgb) = cfl.as_ref().and_then(|l| l.scale_fill) {
        fill = Some(rgb);
    }
    CellLook { style, fill, cf: cfl }
}

/// The colour a number format's `[Red]`-style section names.
pub fn numfmt_rgb(c: gridcraft_numfmt::FormatColor) -> Rgb {
    use gridcraft_numfmt::FormatColor as F;
    match c {
        F::Black | F::Indexed(_) => [0, 0, 0],
        F::Blue => [0, 0, 255],
        F::Cyan => [0, 255, 255],
        F::Green => [0, 128, 0],
        F::Magenta => [255, 0, 255],
        F::Red => [255, 0, 0],
        F::White => [255, 255, 255],
        F::Yellow => [255, 255, 0],
    }
}

/// Last column of a Center Across Selection run starting at `start`: it extends over following
/// columns (up to `limit`) while `continues` holds.
pub fn center_across_end(start: u32, limit: u32, continues: impl Fn(u32) -> bool) -> u32 {
    let mut end = start;
    while end < limit && end + 1 < MAX_COLS && continues(end + 1) {
        end += 1;
    }
    end
}

/// Lays out `req`'s block of cells. A sheet that doesn't exist gives an empty viewport.
pub fn viewport(wb: &Workbook, req: &ViewportRequest, cf: &mut CfCache, m: &dyn Measure) -> Viewport {
    let Some(sh) = wb.sheet(req.sheet) else { return Viewport::default() };
    let z = if req.scale.is_finite() { req.scale.clamp(0.01, 64.0) } else { 1.0 };
    let (c0, c1) = clamp_span(req.cols, MAX_COLS);
    let (r0, mut r1) = clamp_span(req.rows, MAX_ROWS);
    let width = u64::from(c1 - c0) + 1;
    let max_rows = (MAX_VIEWPORT_CELLS / width).max(1);
    if u64::from(r1 - r0) + 1 > max_rows {
        r1 = r0.saturating_add(u32::try_from(max_rows - 1).unwrap_or(u32::MAX)).min(r1);
    }
    let range = RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1));
    let ctx = Ctx::new(wb, req.sheet, sh, range, z, req.formula_locale);
    let mut out = Viewport { rows: (r0, r1), cols: (c0, c1), ..Viewport::default() };
    for r in r0..=r1 {
        if sh.row_height(r) == 0.0 {
            continue;
        }
        for c in c0..=c1 {
            if sh.col_width(c) == 0.0 {
                continue;
            }
            let c = CellRef::new(r, c);
            let span = match ctx.merge_of(c) {
                // A merged area is laid out once, at its first visible cell.
                Some(m) if m.start != c && !(c.row == r0.max(m.start.row) && c.col == c0.max(m.start.col)) => continue,
                Some(m) => m,
                None => RangeRef::cell(c),
            };
            if let Some(view) = ctx.cell(span, cf, m) {
                out.cells.push(view);
            }
        }
    }
    for sp in &sh.sparklines {
        if !range.contains(sp.cell) {
            continue;
        }
        let values = match gridcraft_calc::evaluate(wb, req.sheet, sp.cell, &sp.source) {
            Value::Array(a) => a.data.iter().map(Value::as_f64).collect(),
            v => vec![v.as_f64()],
        };
        let color = sp.color.resolve(&wb.theme).unwrap_or([0x2B, 0x7C, 0xD3]);
        out.sparklines.push(SparklineView { cell: sp.cell, kind: sp.kind, values, color, markers: sp.markers });
    }
    if let Some(af) = &sh.autofilter {
        for col in af.range.start.col..=af.range.end.col {
            let active = af.criteria.iter().any(|(o, _)| af.range.start.col.saturating_add(*o) == col);
            out.filter_buttons.push(FilterButton { cell: CellRef::new(af.range.start.row, col), active });
        }
    }
    for tb in sh.tables.iter().filter(|t| t.filter_button && t.header_row) {
        for col in tb.range.start.col..=tb.range.end.col {
            out.filter_buttons.push(FilterButton { cell: CellRef::new(tb.range.start.row, col), active: false });
        }
    }
    out.filter_buttons.retain(|b| range.contains(b.cell));
    out
}

fn clamp_span((a, b): (u32, u32), max: u32) -> (u32, u32) {
    let (a, b) = (a.min(max - 1), b.min(max - 1));
    (a.min(b), a.max(b))
}

/// What one request shares across its cells.
struct Ctx<'a> {
    wb: &'a Workbook,
    si: usize,
    sh: &'a Sheet,
    range: RangeRef,
    z: f32,
    locale: FormulaLocale,
    /// Merged areas within reach of the block (overflow looks past it).
    merges: Vec<RangeRef>,
}

impl<'a> Ctx<'a> {
    fn new(wb: &'a Workbook, si: usize, sh: &'a Sheet, range: RangeRef, z: f32, locale: FormulaLocale) -> Self {
        let reach = RangeRef::new(
            CellRef::new(range.start.row, range.start.col.saturating_sub(REACH)),
            CellRef::new(range.end.row, range.end.col.saturating_add(REACH).min(MAX_COLS - 1)),
        );
        let merges = sh.merges.iter().filter(|m| m.intersects(&reach)).copied().collect();
        Ctx { wb, si, sh, range, z, locale, merges }
    }

    fn merge_of(&self, c: CellRef) -> Option<RangeRef> {
        self.merges.iter().find(|m| m.contains(c)).copied()
    }

    /// Empty and not merged: text may overflow into it.
    fn empty(&self, c: CellRef) -> bool {
        self.sh.value_ref(c).is_none_or(Value::is_empty) && self.merge_of(c).is_none()
    }

    /// Screen x of column `col`'s left edge, relative to column `from`'s.
    fn x(&self, from: u32, col: u32) -> f32 {
        ((self.sh.col_left(col) - self.sh.col_left(from)) as f32) * self.z
    }

    fn look(&self, c: CellRef, cf: &mut CfCache) -> CellLook {
        cell_look(self.wb, self.si, self.sh, c, Some(cf))
    }

    fn cell(&self, span: RangeRef, cf: &mut CfCache, m: &dyn Measure) -> Option<CellView> {
        let c = span.start;
        let merged = span.start != span.end;
        let lk = self.look(c, cf);
        let v = self.sh.value(c);
        let st = &lk.style;
        let picture = self.sh.cell_pictures.contains_key(&c);
        let checkbox =
            (!picture && st.num_fmt.as_str() == "checkbox" && matches!(v, Value::Bool(_) | Value::Empty)).then_some(v == Value::Bool(true));
        let cfl = lk.cf.as_ref();
        let data_bar = (!picture).then(|| cfl.and_then(|l| l.bar)).flatten().map(|(fraction, color, gradient)| DataBar { fraction, color, gradient });
        let icon = (!picture).then(|| cfl.and_then(|l| l.icon.clone())).flatten().map(|(set, index)| CfIcon { set, index });
        let text = if picture || checkbox.is_some() || cfl.is_some_and(|l| l.hide_value) || v.is_empty() {
            None
        } else {
            self.text(c, span, &v, &lk, icon.is_some(), m)
        };
        let borders = self.borders(span, &lk, cf);
        let comment = self.sh.comments.get(&c).map(|cm| if cm.threaded { CommentMark::Threaded } else { CommentMark::Note });
        let blank = !merged && lk.fill.is_none() && data_bar.is_none() && icon.is_none() && checkbox.is_none() && !picture && text.is_none();
        if blank && borders.is_empty() && comment.is_none() {
            return None;
        }
        Some(CellView { cell: c, span, merged, fill: lk.fill, data_bar, icon, checkbox, picture, text, borders, comment })
    }

    fn text(&self, c: CellRef, span: RangeRef, v: &Value, lk: &CellLook, has_icon: bool, m: &dyn Measure) -> Option<CellText> {
        let (wb, sh, z) = (self.wb, self.sh, self.z);
        let st = &lk.style;
        let merged = span.start != span.end;
        // The content area: the span less a filter button, after an icon.
        let filter_space = sh.filter_button_space(c) * z;
        let right = (self.x(span.start.col, span.end.col.saturating_add(1)) - filter_space).max(0.0);
        let left = if has_icon { 15.0 * z } else { 0.0 };
        let avail_w = right - left;
        let size = st.font.size * 96.0 / 72.0 * z;
        // Digits fit by the cell's own font and size, not the default 11 pt.
        let char_w = m.text_width("0", &st.font, size).max(1.0);
        let shown = shown_text(wb, sh, c, v, st, avail_w, char_w);
        let mut text = shown.text;
        if sh.show_formulas && sh.cell(c).is_some_and(|cell| cell.formula.is_some()) {
            text = crate::formula_locale::display(&text, self.locale, wb, self.si);
        }
        if text.is_empty() {
            return None;
        }
        let color = shown.color.or_else(|| st.font.color.resolve(&wb.theme)).unwrap_or([0, 0, 0]);
        let numeric = shown.numeric;
        let halign = match st.align.h {
            HAlign::General => match v {
                Value::Number(_) => HAlign::Right,
                Value::Bool(_) | Value::Error(_) => HAlign::Center,
                _ if numeric => HAlign::Right,
                _ => HAlign::Left,
            },
            h => h,
        };
        let indent = st.align.indent as f32 * 9.0 * z;
        let pad = 2.5 * z;
        let rotated = st.align.rotation != 0 && st.align.rotation != 255;
        let mut out = CellText {
            text: String::new(),
            font: st.font.clone(),
            size,
            color,
            halign,
            align: st.align,
            layout: TextLayout::Line,
            left,
            right,
            pad,
            indent,
            x: 0.0,
            width: 0.0,
            spill: None,
            cover_gridlines: false,
        };
        // Rotated text wraps along its own direction (below), not across the column width.
        if !rotated && (st.align.wrap || st.align.h == HAlign::Justify) {
            let anchor = match halign {
                HAlign::Center | HAlign::CenterAcross => right / 2.0,
                HAlign::Right => right - pad,
                _ => left + pad + indent,
            };
            out.layout = TextLayout::Wrap { width: (right - 2.0 * pad - indent).max(4.0), anchor };
            out.text = text;
            return Some(out);
        }
        let first_line = text.lines().next().unwrap_or("").to_string();
        // Shrink to fit: too-wide content is drawn in a smaller font so it fits the cell (or the
        // merged area): it neither spills, nor is clipped, nor turns into ####.
        if st.align.shrink && !rotated && st.align.h != HAlign::Fill && shown.fill.is_none() {
            let natural = m.text_width(&first_line, &st.font, size);
            let room = (avail_w - 2.0 * pad - indent).max(1.0);
            if !natural.is_nan() && natural > room {
                // Glyph advances don't scale exactly linearly (rounding), so leave a little slack.
                out.size = (size * room / natural * 0.97).max(1.0);
            }
        }
        let shrunk = out.size != size;
        let mut line = first_line.clone();
        let mut w = m.text_width(&line, &st.font, out.size);
        // Numbers and dates never overflow: they show #### when too narrow.
        if !shrunk && (numeric || matches!(v, Value::Number(_))) && w > avail_w - 2.0 * pad && st.align.h != HAlign::Fill {
            let n = ((avail_w - 2.0 * pad) / char_w).floor().max(1.0) as usize;
            line = "#".repeat(n.min(MAX_REPEAT));
            w = m.text_width(&line, &st.font, out.size);
        }
        // Repeat fill (`*` in number formats) / Fill alignment.
        if let Some((ch, pos)) = shown.fill
            && pos <= first_line.len()
        {
            let cw = m.text_width(&ch.to_string(), &st.font, out.size).max(1.0);
            let n = (((avail_w - 2.0 * pad) - w) / cw).floor().max(0.0) as usize;
            line = first_line.clone();
            if first_line.is_char_boundary(pos) {
                line.insert_str(pos, &ch.to_string().repeat(n.min(MAX_REPEAT)));
            }
            w = m.text_width(&line, &st.font, out.size);
        }
        // Center Across Selection: centred over this cell and the empty cells to its right that
        // also use it.
        let across_end = if halign == HAlign::CenterAcross && !rotated && !merged {
            center_across_end(c.col, self.range.end.col.saturating_add(REACH), |col| {
                let n = CellRef::new(c.row, col);
                self.empty(n) && wb.styles.get(sh.style_id(n)).align.h == HAlign::CenterAcross
            })
        } else {
            c.col
        };
        // Overflow into empty neighbours for text (not upright text, whose width is its line height).
        let (mut clip_l, mut clip_r) = (0.0, right);
        let overflows = !numeric && !matches!(v, Value::Number(_)) && st.align.rotation.abs() != 90 && w > right - 2.0 * pad;
        if filter_space == 0.0 && (overflows || across_end > c.col) && !merged {
            let (mut lo, mut hi) = (c.col, c.col);
            if across_end > c.col {
                hi = across_end;
                clip_r = self.x(c.col, across_end + 1);
            }
            if matches!(halign, HAlign::Left | HAlign::Center | HAlign::Justify) {
                let mut cc = c.col + 1;
                while clip_r < w + 2.0 * pad && cc < MAX_COLS && cc <= self.range.end.col.saturating_add(REACH) && self.empty(CellRef::new(c.row, cc))
                {
                    hi = cc;
                    clip_r = self.x(c.col, cc + 1);
                    cc += 1;
                }
            }
            if matches!(halign, HAlign::Right | HAlign::Center) {
                let mut cc = c.col;
                while right - clip_l < w + 2.0 * pad && cc > 0 && cc + REACH > self.range.start.col && self.empty(CellRef::new(c.row, cc - 1)) {
                    cc -= 1;
                    lo = cc;
                    clip_l = self.x(c.col, cc);
                }
            }
            if (lo, hi) != (c.col, c.col) {
                out.spill = Some((lo, hi));
                out.cover_gridlines = lk.fill.is_none();
            }
        }
        out.x = match halign {
            HAlign::CenterAcross => (clip_l + clip_r) / 2.0 - w / 2.0,
            HAlign::Center | HAlign::Distributed => right / 2.0 - w / 2.0,
            HAlign::Right => right - pad - w - indent,
            _ => left + pad + indent,
        };
        out.width = w;
        if rotated {
            let upright_wrap = st.align.wrap && st.align.rotation.abs() == 90;
            let height = ((sh.row_top(span.end.row.saturating_add(1)) - sh.row_top(span.start.row)) as f32) * z;
            out.layout = TextLayout::Rotated { wrap: upright_wrap.then(|| (height - 2.0 * pad).max(4.0)) };
            out.text = if upright_wrap { text } else { line };
        } else {
            out.text = line;
        }
        Some(out)
    }

    /// Border segments of `span`. A merged area's outline lives on the cells along each of its
    /// edges (Excel writes the right edge on the last column's cells and the bottom edge on the
    /// last row's), so each edge comes cell by cell from the cell it belongs to, for the visible
    /// part of the area.
    fn borders(&self, span: RangeRef, lk: &CellLook, cf: &mut CfCache) -> Vec<BorderSeg> {
        let mut out = Vec::new();
        let mut push = |edge: Edge, from: u32, to: u32, line: BorderLine| {
            if !line.is_none() {
                let color = line.color.resolve(&self.wb.theme).unwrap_or([0, 0, 0]);
                out.push(BorderSeg { edge, from, to, style: line.style, color });
            }
        };
        let b = &lk.style.borders;
        if span.start == span.end {
            let c = span.start;
            push(Edge::Top, c.col, c.col, b.top);
            push(Edge::Bottom, c.col, c.col, b.bottom);
            push(Edge::Left, c.row, c.row, b.left);
            push(Edge::Right, c.row, c.row, b.right);
        } else {
            let (cs, ce) = (span.start.col.max(self.range.start.col), span.end.col.min(self.range.end.col));
            for col in cs..=ce {
                if self.sh.col_width(col) == 0.0 {
                    continue;
                }
                push(Edge::Top, col, col, self.look(CellRef::new(span.start.row, col), cf).style.borders.top);
                push(Edge::Bottom, col, col, self.look(CellRef::new(span.end.row, col), cf).style.borders.bottom);
            }
            let (rs, re) = (span.start.row.max(self.range.start.row), span.end.row.min(self.range.end.row));
            for row in rs..=re {
                if self.sh.row_height(row) == 0.0 {
                    continue;
                }
                push(Edge::Left, row, row, self.look(CellRef::new(row, span.start.col), cf).style.borders.left);
                push(Edge::Right, row, row, self.look(CellRef::new(row, span.end.col), cf).style.borders.right);
            }
        }
        push(Edge::DiagDown, span.start.col, span.end.col, b.diag_down);
        push(Edge::DiagUp, span.start.col, span.end.col, b.diag_up);
        out
    }
}

/// A value as the grid shows it, before measuring.
struct Shown {
    text: String,
    color: Option<Rgb>,
    /// Shown as a number (right-aligned under General, never overflows).
    numeric: bool,
    /// Repeat fill: the character and where it goes.
    fill: Option<(char, usize)>,
}

/// The text for value `v`, with General narrowing to `width` screen units of `char_w`-wide digits.
fn shown_text(wb: &Workbook, sh: &Sheet, c: CellRef, v: &Value, st: &Style, width: f32, char_w: f32) -> Shown {
    let none = |text: String| Shown { text, color: None, numeric: false, fill: None };
    if sh.show_formulas
        && let Some(f) = sh.cell(c).and_then(|x| x.formula.as_ref())
    {
        return none(format!("={}", f.text));
    }
    if !sh.show_zeros && v.as_f64() == Some(0.0) {
        return none(String::new());
    }
    let code = st.num_fmt.as_str();
    // A checkbox cell draws TRUE/FALSE (and empty) as a box; other values show as General.
    if code == "checkbox" && matches!(v, Value::Bool(_) | Value::Empty) {
        return none(String::new());
    }
    if code == "General"
        && let Value::Number(n) = v
    {
        let max_chars = ((width - 4.0) / char_w).floor().max(1.0) as usize;
        let text = gridcraft_numfmt::format_general_fit(*n, max_chars.min(11)).unwrap_or_else(|| "#".repeat(max_chars.clamp(1, MAX_REPEAT)));
        return Shown { text, color: None, numeric: true, fill: None };
    }
    let f = crate::display::format(v, code, wb);
    Shown { text: f.text, color: f.color.map(numfmt_rgb), numeric: f.numeric, fill: f.fill }
}

#[cfg(test)]
mod tests {
    use gridcraft_model::NumFmt;

    use super::*;

    /// Every character is `size / 2` wide.
    struct Mono;
    impl Measure for Mono {
        fn text_width(&self, text: &str, _font: &Font, size: f32) -> f32 {
            text.chars().count() as f32 * size / 2.0
        }
    }

    fn c(a: &str) -> CellRef {
        CellRef::parse(a).unwrap()
    }

    fn set(wb: &mut Workbook, a: &str, v: Value) {
        wb.sheet_mut(0).unwrap().set_value(c(a), v);
    }

    fn style(wb: &mut Workbook, range: &str, f: impl Fn(&mut Style)) {
        let r = RangeRef::parse(range).unwrap();
        for cell in r.iter() {
            let mut st = wb.styles.get(wb.sheet(0).unwrap().style_id(cell)).clone();
            f(&mut st);
            let id = wb.styles.intern(st);
            wb.sheet_mut(0).unwrap().set_style(cell, id);
        }
    }

    fn view(wb: &Workbook, rows: (u32, u32), cols: (u32, u32)) -> Viewport {
        let req = ViewportRequest { sheet: 0, rows, cols, scale: 1.0, formula_locale: FormulaLocale::En };
        viewport(wb, &req, &mut CfCache::new(), &Mono)
    }

    fn at(v: &Viewport, a: &str) -> CellView {
        v.cells.iter().find(|x| x.cell == c(a)).cloned().unwrap()
    }

    fn text(v: &Viewport, a: &str) -> CellText {
        at(v, a).text.unwrap()
    }

    #[test]
    fn empty_cells_are_left_out_and_general_resolves_by_value() {
        let mut wb = Workbook::new();
        set(&mut wb, "A1", Value::Text("hello".into()));
        set(&mut wb, "B1", Value::Number(42.0));
        set(&mut wb, "C1", Value::Bool(true));
        let v = view(&wb, (0, 9), (0, 9));
        assert_eq!(v.cells.len(), 3);
        assert_eq!(text(&v, "A1").halign, HAlign::Left);
        assert_eq!(text(&v, "B1").halign, HAlign::Right);
        assert_eq!(text(&v, "B1").text, "42");
        assert_eq!(text(&v, "C1").halign, HAlign::Center);
        assert_eq!(text(&v, "C1").text, "TRUE");
    }

    #[test]
    fn long_text_spills_into_empty_neighbours_but_numbers_turn_to_hashes() {
        let mut wb = Workbook::new();
        set(&mut wb, "A1", Value::Text("a rather long piece of text that needs room".into()));
        set(&mut wb, "C1", Value::Text("stop".into()));
        set(&mut wb, "A2", Value::Number(123456789012345678.0));
        style(&mut wb, "A2", |st| st.num_fmt = NumFmt::new("0"));
        let v = view(&wb, (0, 9), (0, 9));
        let a1 = text(&v, "A1");
        assert_eq!(a1.spill, Some((0, 1)), "stops before the filled C1");
        assert!(a1.cover_gridlines);
        let a2 = text(&v, "A2");
        assert!(!a2.text.is_empty() && a2.text.chars().all(|ch| ch == '#'), "{:?}", a2.text);
        assert_eq!(a2.spill, None);
    }

    #[test]
    fn general_numbers_narrow_to_the_column() {
        let mut wb = Workbook::new();
        set(&mut wb, "A1", Value::Number(std::f64::consts::PI));
        let t = text(&view(&wb, (0, 0), (0, 0)), "A1");
        assert!(t.text.starts_with("3.14") && t.text.len() <= 11, "{:?}", t.text);
        assert!(t.width <= t.right - 2.0 * t.pad, "fits: {t:?}");
    }

    #[test]
    fn shrink_to_fit_makes_the_font_smaller_instead_of_spilling() {
        let mut wb = Workbook::new();
        set(&mut wb, "A1", Value::Text("a rather long piece of text".into()));
        style(&mut wb, "A1", |st| st.align.shrink = true);
        let t = text(&view(&wb, (0, 0), (0, 3)), "A1");
        assert!(t.size < 11.0 * 96.0 / 72.0, "{t:?}");
        assert_eq!(t.spill, None);
    }

    #[test]
    fn merged_area_appears_once_with_its_outline() {
        let mut wb = Workbook::new();
        set(&mut wb, "B2", Value::Text("merged".into()));
        wb.sheet_mut(0).unwrap().merges.push(RangeRef::parse("B2:D3").unwrap());
        let thin = BorderLine { style: BorderStyle::Thin, color: Color::Auto };
        style(&mut wb, "B2:D2", |st| st.borders.top = thin);
        style(&mut wb, "B3:D3", |st| st.borders.bottom = thin);
        style(&mut wb, "B2:B3", |st| st.borders.left = thin);
        style(&mut wb, "D2:D3", |st| st.borders.right = thin);
        let v = view(&wb, (0, 9), (0, 9));
        assert_eq!(v.cells.iter().filter(|c| c.merged).count(), 1);
        let b2 = at(&v, "B2");
        assert_eq!(b2.span, RangeRef::parse("B2:D3").unwrap());
        // Three columns × top and bottom, two rows × left and right.
        assert_eq!(b2.borders.len(), 10, "{:?}", b2.borders);
        assert!(b2.borders.iter().all(|b| b.style == BorderStyle::Thin && b.color == [0, 0, 0]));
        // Scrolled so the area starts above and left of the block: laid out at its first visible
        // cell, with only the visible part of its outline.
        let v = view(&wb, (2, 9), (2, 9));
        let b2 = at(&v, "B2");
        assert_eq!(b2.span, RangeRef::parse("B2:D3").unwrap());
        assert_eq!(b2.borders.len(), 2 * 2 + 2, "{:?}", b2.borders);
    }

    #[test]
    fn center_across_selection_spans_following_empty_cells() {
        let mut wb = Workbook::new();
        set(&mut wb, "A1", Value::Text("title".into()));
        style(&mut wb, "A1:C1", |st| st.align.h = HAlign::CenterAcross);
        let v = view(&wb, (0, 5), (0, 5));
        let t = text(&v, "A1");
        assert_eq!(t.spill, Some((0, 2)));
        let w = wb.sheet(0).unwrap().col_left(3) as f32;
        assert!((t.x + t.width / 2.0 - w / 2.0).abs() < 0.01, "{t:?}");
        // Borders and blank styled cells draw nothing on their own.
        assert_eq!(v.cells.len(), 1);
    }

    #[test]
    fn hidden_rows_and_columns_are_skipped() {
        let mut wb = Workbook::new();
        set(&mut wb, "A1", Value::Text("x".into()));
        set(&mut wb, "B2", Value::Text("y".into()));
        let sh = wb.sheet_mut(0).unwrap();
        sh.rows.entry(1).or_default().hidden = true;
        let v = view(&wb, (0, 5), (0, 5));
        assert_eq!(v.cells.len(), 1);
    }

    #[test]
    fn center_across_run_stops_at_first_non_continuing_column() {
        assert_eq!(center_across_end(0, 20, |c| c <= 2), 2);
        assert_eq!(center_across_end(0, 20, |_| false), 0);
        assert_eq!(center_across_end(0, 1, |_| true), 1);
    }

    #[test]
    fn hostile_requests_are_clamped() {
        let wb = Workbook::new();
        let req = ViewportRequest { sheet: 0, rows: (u32::MAX, 0), cols: (0, u32::MAX), scale: f32::NAN, formula_locale: FormulaLocale::En };
        let v = viewport(&wb, &req, &mut CfCache::new(), &Mono);
        assert!(u64::from(v.rows.1 - v.rows.0 + 1) * u64::from(v.cols.1 - v.cols.0 + 1) <= MAX_VIEWPORT_CELLS);
        let req = ViewportRequest { sheet: 99, ..req };
        assert_eq!(viewport(&wb, &req, &mut CfCache::new(), &Mono), Viewport::default());
    }
}
