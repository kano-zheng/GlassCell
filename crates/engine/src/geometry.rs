//! Grid geometry for a frontend in Normal view: where rows and columns are on screen for a
//! scroll position and zoom, the frozen panes, scrolling a cell into view, and hit-testing
//! (cells, row and column edges, the fill handle, the selection's border, filter buttons and
//! floating objects).
//!
//! Screen units are sheet units × [`Geometry::scale`], measured from [`Geometry::origin`], the
//! top-left corner of the cells area (below the column headers, right of the row headers). The
//! scroll offset is in sheet units past the frozen rows and columns. A right-to-left sheet is
//! laid out left to right here and mirrored by the frontend.

use std::sync::Arc;

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_model::{Anchor, LineIndex, Sheet, Workbook};

use crate::paper::PageGrid;
use serde::Serialize;

use crate::Selection;

/// How close (screen units) to a column header's right edge the pointer resizes the column.
pub const COL_EDGE_SLOP: f32 = 4.0;
/// How close to a row header's bottom edge the pointer resizes the row.
pub const ROW_EDGE_SLOP: f32 = 3.0;
/// Side of the fill handle at the selection's bottom-right corner.
pub const FILL_HANDLE: f32 = 9.0;
/// How far either side of the selection's outline the pointer grabs it to move the cells.
pub const BORDER_SLOP: f32 = 3.0;
/// How close to a selected object's bottom-right corner the pointer resizes it.
pub const OBJECT_RESIZE_RADIUS: f32 = 10.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn from_min_max(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
    pub fn expand(&self, d: f32) -> Rect {
        Rect { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }
}

/// One of up to four panes (frozen rows and columns, the scrolling part): its rows and columns
/// (first, last) and where it is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Pane {
    pub rows: (u32, u32),
    pub cols: (u32, u32),
    pub rect: Rect,
}

/// What a point over the cells area is on, in the order a click picks it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Hit {
    /// A floating object (`chart`, `image`, `slicer`, `shape`), topmost first.
    Object {
        object: String,
        id: u32,
        rect: Rect,
    },
    /// The fill handle of the current selection area.
    FillHandle,
    /// The current selection area's outline (drag to move the cells).
    SelectionBorder,
    /// A filter dropdown in a header cell.
    FilterButton {
        cell: CellRef,
    },
    Cell {
        cell: CellRef,
    },
}

#[derive(Clone, Debug)]
pub struct Geometry {
    /// Top-left of the cells area on screen.
    pub origin: (f32, f32),
    /// Size of the cells area.
    pub size: (f32, f32),
    /// Screen units per sheet unit.
    pub scale: f32,
    /// Sheet units scrolled past the frozen panes.
    pub scroll: (f32, f32),
    /// Frozen rows and columns (counts).
    pub frozen: (u32, u32),
    /// Screen size of the frozen columns and rows.
    pub frozen_w: f32,
    pub frozen_h: f32,
    rows: Arc<LineIndex>,
    cols: Arc<LineIndex>,
    /// Page Layout view: cells sit on printed pages (`scroll` is then in layout units).
    paper: Option<Arc<PageGrid>>,
}

/// One printed page in Page Layout view: the paper, where its cells go, and whether it holds
/// printed data (else it is a faded "add data" page).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PageView {
    pub paper: Rect,
    pub content: Rect,
    pub data: bool,
}

impl Geometry {
    /// Geometry of `sh` scrolled to `scroll`, with its cells area at `origin` and `size` big, at
    /// `scale` screen units per sheet unit.
    pub fn new(sh: &Sheet, origin: (f32, f32), size: (f32, f32), scroll: (f32, f32), scale: f32) -> Geometry {
        let scale = if scale.is_finite() { scale.clamp(0.01, 64.0) } else { 1.0 };
        let finite = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let rows = Arc::new(LineIndex::new(&sh.rows, sh.default_row_height, MAX_ROWS));
        let cols = Arc::new(LineIndex::new(&sh.cols, sh.default_col_width, MAX_COLS));
        let (fr, fc) = sh.freeze.unwrap_or((0, 0));
        let (fr, fc) = (fr.min(MAX_ROWS - 1), fc.min(MAX_COLS - 1));
        Geometry {
            origin,
            size: (finite(size.0), finite(size.1)),
            scale,
            scroll: (finite(scroll.0), finite(scroll.1)),
            frozen: (fr, fc),
            frozen_w: (cols.start(fc) as f32) * scale,
            frozen_h: (rows.start(fr) as f32) * scale,
            rows,
            cols,
            paper: None,
        }
    }

    /// Page Layout view geometry of sheet `si`: the sheet paginated as printed, pages side by
    /// side on a desk, scrolled by `scroll` layout units, cells at `scale` × the print scale.
    /// No frozen panes (as in Excel). `None` when the sheet doesn't exist.
    pub fn paged(wb: &Workbook, si: usize, origin: (f32, f32), size: (f32, f32), scroll: (f32, f32), scale: f32) -> Option<Geometry> {
        let sh = wb.sheet(si)?;
        let mut g = Geometry::new(sh, origin, size, scroll, scale);
        // Layout units never run ahead of sheet units: covering the view in sheet units is enough.
        let probe = PageGrid::build(wb, si, 0, 0)?;
        let z = (g.scale * probe.scale.clamp(0.1, 4.0)).clamp(0.01, 64.0);
        let (need_row, need_col) = crate::paper::need_for(sh, f64::from(g.scroll.0 + g.size.0 / z), f64::from(g.scroll.1 + g.size.1 / z));
        let grid = PageGrid::build(wb, si, need_row, need_col)?;
        g.scale = z;
        g.frozen = (0, 0);
        g.frozen_w = 0.0;
        g.frozen_h = 0.0;
        g.paper = Some(Arc::new(grid));
        Some(g)
    }

    /// Page Layout view: the visible pages.
    pub fn pages(&self) -> Vec<PageView> {
        let Some(pg) = &self.paper else { return vec![] };
        let z = f64::from(self.scale);
        let (sx, sy) = (f64::from(self.scroll.0), f64::from(self.scroll.1));
        let ((i0, i1), (j0, j1)) = pg.pages_in(sx, sy, sx + f64::from(self.size.0) / z, sy + f64::from(self.size.1) / z);
        let screen = |(x, y, w, h): (f64, f64, f64, f64)| Rect {
            x: self.origin.0 + ((x - sx) * z) as f32,
            y: self.origin.1 + ((y - sy) * z) as f32,
            w: (w * z) as f32,
            h: (h * z) as f32,
        };
        let mut out = Vec::new();
        for i in i0..=i1.min(i0 + 64) {
            for j in j0..=j1.min(j0 + 64) {
                out.push(PageView { paper: screen(pg.page_rect(i, j)), content: screen(pg.content_rect(i, j)), data: pg.is_data_page(i, j) });
            }
        }
        out
    }

    /// How far the scrolled panes can be scrolled now, in `scroll` units: through the used range
    /// and some more (`scroll_span`), or to where they are already. What a scroll bar spans.
    pub fn scroll_extent(&self, sh: &Sheet) -> (f32, f32) {
        let (last_row, last_col) = scroll_span(sh);
        let (left, top) = match &self.paper {
            Some(pg) => (pg.col_x(last_col) as f32, pg.row_y(last_row) as f32),
            None => (
                (self.cols.start(last_col) - self.cols.start(self.frozen.1)) as f32,
                (self.rows.start(last_row) - self.rows.start(self.frozen.0)) as f32,
            ),
        };
        let visible = ((self.size.0 - self.frozen_w) / self.scale, (self.size.1 - self.frozen_h) / self.scale);
        ((left - visible.0).max(self.scroll.0).max(0.0), (top - visible.1).max(self.scroll.1).max(0.0))
    }

    /// The cells area.
    pub fn cells(&self) -> Rect {
        Rect { x: self.origin.0, y: self.origin.1, w: self.size.0, h: self.size.1 }
    }

    /// Left edge of column `col`.
    pub fn x(&self, col: u32) -> f32 {
        if let Some(pg) = &self.paper {
            return self.origin.0 + ((pg.col_x(col) - f64::from(self.scroll.0)) * f64::from(self.scale)) as f32;
        }
        let (left, z, fc) = (self.origin.0, self.scale, self.frozen.1);
        if col < fc {
            left + self.cols.start(col) as f32 * z
        } else {
            left + self.frozen_w + (self.cols.start(col) as f32 - self.cols.start(fc) as f32 - self.scroll.0) * z
        }
    }

    /// Top edge of row `row`.
    pub fn y(&self, row: u32) -> f32 {
        if let Some(pg) = &self.paper {
            return self.origin.1 + ((pg.row_y(row) - f64::from(self.scroll.1)) * f64::from(self.scale)) as f32;
        }
        let (top, z, fr) = (self.origin.1, self.scale, self.frozen.0);
        if row < fr {
            top + self.rows.start(row) as f32 * z
        } else {
            top + self.frozen_h + (self.rows.start(row) as f32 - self.rows.start(fr) as f32 - self.scroll.1) * z
        }
    }

    pub fn col_at(&self, x: f32) -> u32 {
        if let Some(pg) = &self.paper {
            return pg.col_at(f64::from(self.scroll.0) + f64::from((x - self.origin.0) / self.scale)).min(MAX_COLS - 1);
        }
        let dx = (x - self.origin.0) / self.scale;
        if dx < self.frozen_w / self.scale {
            self.cols.at(dx.max(0.0) as f64)
        } else {
            self.cols.at(self.cols.start(self.frozen.1) + self.scroll.0 as f64 + (dx - self.frozen_w / self.scale) as f64)
        }
    }

    pub fn row_at(&self, y: f32) -> u32 {
        if let Some(pg) = &self.paper {
            return pg.row_at(f64::from(self.scroll.1) + f64::from((y - self.origin.1) / self.scale)).min(MAX_ROWS - 1);
        }
        let dy = (y - self.origin.1) / self.scale;
        if dy < self.frozen_h / self.scale {
            self.rows.at(dy.max(0.0) as f64)
        } else {
            self.rows.at(self.rows.start(self.frozen.0) + self.scroll.1 as f64 + (dy - self.frozen_h / self.scale) as f64)
        }
    }

    pub fn cell_at(&self, x: f32, y: f32) -> CellRef {
        CellRef::new(self.row_at(y), self.col_at(x))
    }

    pub fn cell_rect(&self, sh: &Sheet, c: CellRef) -> Rect {
        let (x, y) = (self.x(c.col), self.y(c.row));
        Rect::from_min_max(x, y, x + sh.col_width(c.col) * self.scale, y + sh.row_height(c.row) * self.scale)
    }

    pub fn range_rect(&self, sh: &Sheet, r: RangeRef) -> Rect {
        let (a, b) = (self.cell_rect(sh, r.start), self.cell_rect(sh, r.end));
        Rect::from_min_max(a.x, a.y, b.right(), b.bottom())
    }

    /// Rows (first, last) visible in the scrolling area, after the frozen ones.
    pub fn scroll_rows(&self) -> (u32, u32) {
        let top = self.rows.start(self.frozen.0) + self.scroll.1 as f64;
        let r0 = self.rows.at(top).max(self.frozen.0);
        let r1 = self.rows.at(top + ((self.size.1 - self.frozen_h) / self.scale) as f64).min(MAX_ROWS - 1);
        (r0, r1)
    }

    /// Columns (first, last) visible in the scrolling area, after the frozen ones.
    pub fn scroll_cols(&self) -> (u32, u32) {
        let left = self.cols.start(self.frozen.1) + self.scroll.0 as f64;
        let c0 = self.cols.at(left).max(self.frozen.1);
        let c1 = self.cols.at(left + ((self.size.0 - self.frozen_w) / self.scale) as f64).min(MAX_COLS - 1);
        (c0, c1)
    }

    fn split(&self) -> (f32, f32) {
        (self.origin.0 + self.frozen_w, self.origin.1 + self.frozen_h)
    }

    /// The panes to paint, scrolling one first: each pane's rows, columns and clip. In Page
    /// Layout view, one per visible page (its cells, clipped to its margins).
    pub fn panes(&self) -> Vec<Pane> {
        if let Some(pg) = &self.paper {
            let cells = self.cells();
            let z = f64::from(self.scale);
            let (sx, sy) = (f64::from(self.scroll.0), f64::from(self.scroll.1));
            let ((i0, i1), (j0, j1)) = pg.pages_in(sx, sy, sx + f64::from(self.size.0) / z, sy + f64::from(self.size.1) / z);
            let mut out = Vec::new();
            for i in i0..=i1.min(i0 + 64) {
                for j in j0..=j1.min(j0 + 64) {
                    let (Some((r0, r1)), Some((c0, c1))) = (pg.row_band(i), pg.col_band(j)) else { continue };
                    let (x, y, w, h) = pg.content_rect(i, j);
                    let clip = Rect {
                        x: self.origin.0 + ((x - sx) * z) as f32,
                        y: self.origin.1 + ((y - sy) * z) as f32,
                        w: (w * z) as f32,
                        h: (h * z) as f32,
                    };
                    let left = clip.x.max(cells.x);
                    let top = clip.y.max(cells.y);
                    let right = clip.right().min(cells.right());
                    let bottom = clip.bottom().min(cells.bottom());
                    if right <= left || bottom <= top {
                        continue;
                    }
                    let rows = (self.row_at(top).max(r0), self.row_at(bottom - 0.5).min(r1));
                    let cols = (self.col_at(left).max(c0), self.col_at(right - 0.5).min(c1));
                    if rows.0 <= rows.1 && cols.0 <= cols.1 {
                        out.push(Pane { rows, cols, rect: Rect::from_min_max(left, top, right, bottom) });
                    }
                }
            }
            return out;
        }
        let cells = self.cells();
        let (sx, sy) = self.split();
        let (rows, cols) = (self.scroll_rows(), self.scroll_cols());
        let frozen_rows = (0, self.frozen.0.saturating_sub(1));
        let frozen_cols = (0, self.frozen.1.saturating_sub(1));
        let mut out = vec![Pane { rows, cols, rect: Rect::from_min_max(sx, sy, cells.right(), cells.bottom()) }];
        if self.frozen.0 > 0 {
            out.push(Pane { rows: frozen_rows, cols, rect: Rect::from_min_max(sx, cells.y, cells.right(), sy) });
        }
        if self.frozen.1 > 0 {
            out.push(Pane { rows, cols: frozen_cols, rect: Rect::from_min_max(cells.x, sy, sx, cells.bottom()) });
        }
        if self.frozen.0 > 0 && self.frozen.1 > 0 {
            out.push(Pane { rows: frozen_rows, cols: frozen_cols, rect: Rect::from_min_max(cells.x, cells.y, sx, sy) });
        }
        out.retain(|p| p.rows.0 <= p.rows.1 && p.cols.0 <= p.cols.1);
        out
    }

    /// Screen areas of the panes that hold part of `r`. The panes share one coordinate system, so
    /// a range not clipped to these is also drawn over the frozen panes when scrolled under them.
    pub fn range_panes(&self, r: RangeRef) -> Vec<Rect> {
        let cells = self.cells();
        if self.paper.is_some() {
            return vec![cells];
        }
        let (sx, sy) = self.split();
        let mut xs = Vec::new();
        if r.start.col < self.frozen.1 {
            xs.push((cells.x, sx));
        }
        if r.end.col >= self.frozen.1 {
            xs.push((sx, cells.right()));
        }
        let mut ys = Vec::new();
        if r.start.row < self.frozen.0 {
            ys.push((cells.y, sy));
        }
        if r.end.row >= self.frozen.0 {
            ys.push((sy, cells.bottom()));
        }
        ys.iter().flat_map(|&(y0, y1)| xs.iter().map(move |&(x0, x1)| Rect::from_min_max(x0, y0, x1, y1))).collect()
    }

    /// The scroll offset that brings `target` into view (unchanged when it is, or when it is in
    /// the frozen rows or columns).
    pub fn reveal(&self, sh: &Sheet, target: CellRef) -> (f32, f32) {
        let cr = self.cell_rect(sh, target);
        let cells = self.cells();
        let (left, top) = self.split();
        let (right, bottom) = (cells.right(), cells.bottom());
        let z = self.scale;
        let (mut sx, mut sy) = self.scroll;
        if target.row >= self.frozen.0 {
            if cr.y < top {
                sy -= (top - cr.y) / z;
            } else if cr.bottom() > bottom - 2.0 {
                sy += (cr.bottom() - bottom + 2.0) / z;
            }
        }
        if target.col >= self.frozen.1 {
            if cr.x < left {
                sx -= (left - cr.x) / z;
            } else if cr.right() > right - 2.0 {
                // A column wider than the view keeps its left edge in view.
                sx += ((cr.right() - right + 2.0) / z).min(cr.x - left);
            }
        }
        (sx.max(0.0), sy.max(0.0))
    }

    /// The column whose right edge is within [`COL_EDGE_SLOP`] of `x` (in the column headers),
    /// and that edge.
    pub fn col_edge(&self, sh: &Sheet, x: f32) -> Option<(u32, f32)> {
        let c = self.col_at(x);
        [c, c.saturating_sub(1)].into_iter().map(|cc| (cc, self.x(cc) + sh.col_width(cc) * self.scale)).find(|(_, e)| (x - e).abs() <= COL_EDGE_SLOP)
    }

    /// The row whose bottom edge is within [`ROW_EDGE_SLOP`] of `y` (in the row headers), and that edge.
    pub fn row_edge(&self, sh: &Sheet, y: f32) -> Option<(u32, f32)> {
        let r = self.row_at(y);
        [r, r.saturating_sub(1)].into_iter().map(|rr| (rr, self.y(rr) + sh.row_height(rr) * self.scale)).find(|(_, e)| (y - e).abs() <= ROW_EDGE_SLOP)
    }

    /// Where a floating object anchored at `a` is.
    pub fn anchor_rect(&self, a: &Anchor) -> Rect {
        let (x, y) = (self.x(a.cell.col) + a.dx * self.scale, self.y(a.cell.row) + a.dy * self.scale);
        Rect { x, y, w: a.width * self.scale, h: a.height.max(1.0) * self.scale }
    }

    /// The topmost floating object at a point: slicers, then charts, pictures and shapes (later
    /// ones are drawn over earlier ones). Charts and shapes are easier to grab by their edge.
    pub fn object_at(&self, sh: &Sheet, x: f32, y: f32) -> Option<(&'static str, u32, Rect)> {
        if !self.cells().contains(x, y) {
            return None;
        }
        let find = |kind: &'static str, grab: f32, objects: Vec<(u32, &Anchor)>| {
            objects.into_iter().rev().map(|(id, a)| (kind, id, self.anchor_rect(a))).find(|(_, _, r)| r.expand(grab).contains(x, y))
        };
        find("slicer", 0.0, sh.slicers.iter().map(|o| (o.id, &o.anchor)).collect())
            .or_else(|| find("chart", 4.0, sh.charts.iter().map(|o| (o.id, &o.anchor)).collect()))
            .or_else(|| find("image", 0.0, sh.images.iter().map(|o| (o.id, &o.anchor)).collect()))
            .or_else(|| find("shape", 3.0, sh.shapes.iter().map(|o| (o.id, &o.anchor)).collect()))
    }

    /// The fill handle of the selection's current area. It only reacts in the panes where it is
    /// painted, not over frozen panes a scrolled selection lies under.
    pub fn fill_handle(&self, sh: &Sheet, sel: &Selection) -> Rect {
        let r = self.range_rect(sh, sel.current());
        Rect { x: r.right() - FILL_HANDLE / 2.0, y: r.bottom() - FILL_HANDLE / 2.0, w: FILL_HANDLE, h: FILL_HANDLE }
    }

    /// The filter dropdown in header cell `c` (empty without one): within the room AutoFit
    /// leaves for it, at the cell's bottom-right.
    pub fn filter_button(&self, sh: &Sheet, c: CellRef) -> Rect {
        let rect = self.cell_rect(sh, c);
        let pad = 2.0 * self.scale;
        let s = (rect.h - 4.0).clamp(10.0, 18.0).min((sh.filter_button_space(c) * self.scale - pad).max(0.0));
        Rect { x: rect.right() - s - pad, y: rect.bottom() - s - pad, w: s, h: s }
    }

    /// What a click at `(x, y)` in the cells area is on.
    pub fn hit(&self, sh: &Sheet, sel: &Selection, x: f32, y: f32) -> Hit {
        if let Some((object, id, rect)) = self.object_at(sh, x, y) {
            return Hit::Object { object: object.to_string(), id, rect };
        }
        let in_panes = |r: RangeRef| self.range_panes(r).iter().any(|p| p.contains(x, y));
        let cur = sel.current();
        if self.fill_handle(sh, sel).contains(x, y) && in_panes(RangeRef::cell(cur.end)) {
            return Hit::FillHandle;
        }
        let outline = self.range_rect(sh, cur);
        let shrunk = outline.expand(-BORDER_SLOP);
        let on_border = outline.expand(BORDER_SLOP).contains(x, y) && !(shrunk.w > 0.0 && shrunk.h > 0.0 && shrunk.contains(x, y));
        if on_border && in_panes(cur) {
            return Hit::SelectionBorder;
        }
        let cell = self.cell_at(x, y);
        if sh.has_filter_button(cell) && self.filter_button(sh, cell).contains(x, y) {
            return Hit::FilterButton { cell };
        }
        Hit::Cell { cell }
    }
}

/// Where a fill-handle drag of `source` to the cell under the pointer fills: the source
/// extended along the axis the pointer moved further out on (the source itself while the
/// pointer is inside it).
/// How many digits the row headers are sized for, with `last_shown` (0-based) the last row in
/// view: enough for every row a scroll bar reaches too, so the headers keep their width while
/// the sheet scrolls (they used to widen at row 100 and shift every cell); at least two.
pub fn row_number_digits(sh: &Sheet, last_shown: u32) -> u32 {
    let last = scroll_span(sh).0.max(last_shown).saturating_add(1);
    last.checked_ilog10().map_or(1, |d| d + 1).max(2)
}

/// The last row and column a scroll bar reaches: ten past the used range, and at least row 100
/// and column Z.
pub fn scroll_span(sh: &Sheet) -> (u32, u32) {
    let used = sh.used_range();
    let row = used.map(|r| r.end.row.saturating_add(10)).unwrap_or(100).clamp(100, MAX_ROWS - 1);
    let col = used.map(|r| r.end.col.saturating_add(10)).unwrap_or(26).clamp(26, MAX_COLS - 1);
    (row, col)
}

pub fn fill_target(source: RangeRef, pointer: CellRef) -> RangeRef {
    let below = i64::from(pointer.row) - i64::from(source.end.row);
    let above = i64::from(source.start.row) - i64::from(pointer.row);
    let right = i64::from(pointer.col) - i64::from(source.end.col);
    let left = i64::from(source.start.col) - i64::from(pointer.col);
    let (vert, horiz) = (below.max(above), right.max(left));
    if vert <= 0 && horiz <= 0 {
        return source;
    }
    if vert >= horiz {
        if below > 0 {
            RangeRef::new(source.start, CellRef::new(pointer.row, source.end.col))
        } else {
            RangeRef::new(CellRef::new(pointer.row, source.start.col), source.end)
        }
    } else if right > 0 {
        RangeRef::new(source.start, CellRef::new(source.end.row, pointer.col))
    } else {
        RangeRef::new(CellRef::new(source.start.row, pointer.col), source.end)
    }
}

/// A double-click on the fill handle fills `source` down as far as the column beside it (left,
/// or right in column A) has data; `None` when there is nothing to fill.
pub fn fill_down_target(sh: &Sheet, source: RangeRef) -> Option<RangeRef> {
    let neighbour = if source.start.col > 0 { source.start.col - 1 } else { source.end.col.saturating_add(1) };
    let mut last = source.end.row;
    while last + 1 < MAX_ROWS && last - source.end.row < 1_000_000 && sh.value_ref(CellRef::new(last + 1, neighbour)).is_some_and(|v| !v.is_empty()) {
        last += 1;
    }
    (last > source.end.row).then(|| RangeRef::new(source.start, CellRef::new(last, source.end.col)))
}

/// The columns a resize (or AutoFit) of column `col` changes, as `home.columnWidth`'s `cols`:
/// inside a selection of whole columns every selected column (⌘-selected areas too), otherwise
/// the one dragged.
pub fn resize_columns(sel: &Selection, col: u32) -> String {
    if sel.ranges.iter().any(|r| r.is_full_cols() && col >= r.start.col && col <= r.end.col) {
        sel.ranges.iter().filter(|r| r.is_full_cols()).map(|r| r.a1()).collect::<Vec<_>>().join(",")
    } else {
        RangeRef::cols(col, col).a1()
    }
}

/// The rows a resize (or AutoFit) of row `row` changes, as `home.rowHeight`'s `rows`.
pub fn resize_rows(sel: &Selection, row: u32) -> String {
    if sel.ranges.iter().any(|r| r.is_full_rows() && row >= r.start.row && row <= r.end.row) {
        sel.ranges.iter().filter(|r| r.is_full_rows()).map(|r| format!("{}:{}", r.start.row + 1, r.end.row + 1)).collect::<Vec<_>>().join(",")
    } else {
        format!("{}:{}", row + 1, row + 1)
    }
}

/// What a resize shows while dragging: `Width: 8.43 (64 pixels)` (characters of the default
/// font, and sheet units) or `Height: 15.00 (20 pixels)` (points).
pub fn resize_label(columns: bool, size: f32) -> String {
    let size = if size.is_finite() { size.max(0.0) } else { 0.0 };
    if columns {
        format!("Width: {:.2} ({} pixels)", gridcraft_model::points_to_chars(f64::from(size)), size.round())
    } else {
        format!("Height: {:.2} ({} pixels)", size * 0.75, size.round())
    }
}

#[cfg(test)]
mod tests {
    use gridcraft_model::LineInfo;

    use super::*;

    fn sheet() -> Sheet {
        let mut sh = Sheet::new("S");
        sh.cols.insert(1, LineInfo { size: Some(100.0), ..LineInfo::default() });
        sh.rows.insert(2, LineInfo { hidden: true, ..LineInfo::default() });
        sh
    }

    #[test]
    fn columns_and_rows_map_both_ways_with_frozen_panes_and_scrolling() {
        let mut sh = sheet();
        sh.freeze = Some((1, 1));
        let g = Geometry::new(&sh, (10.0, 20.0), (800.0, 600.0), (100.0, 0.0), 1.0);
        let w = sh.default_col_width;
        assert_eq!(g.x(0), 10.0);
        assert_eq!(g.frozen_w, w);
        // Column B (100 wide) is scrolled out of view: C starts right after the frozen A.
        assert_eq!(g.x(2), 10.0 + w);
        assert_eq!(g.col_at(10.0 + w + 1.0), 2);
        assert_eq!(g.col_at(15.0), 0);
        // The hidden row 3 takes no room.
        assert_eq!(g.y(3), g.y(2));
        assert_eq!(g.row_at(g.y(3) + 1.0), 3);
        assert_eq!(g.panes().len(), 4);
        assert_eq!(g.scroll_cols().0, 2);
    }

    #[test]
    fn reveal_scrolls_just_enough() {
        let sh = sheet();
        let g = Geometry::new(&sh, (0.0, 0.0), (500.0, 300.0), (0.0, 0.0), 1.0);
        let (sx, sy) = g.reveal(&sh, CellRef::new(100, 0));
        assert_eq!(sx, 0.0);
        let g = Geometry::new(&sh, (0.0, 0.0), (500.0, 300.0), (sx, sy), 1.0);
        let r = g.cell_rect(&sh, CellRef::new(100, 0));
        assert!(r.bottom() <= 300.0 && r.bottom() > 290.0, "{r:?}");
        assert_eq!(g.reveal(&sh, CellRef::new(100, 0)), (sx, sy), "already in view");
        assert_eq!(g.reveal(&sh, CellRef::new(0, 0)).1, 0.0);
    }

    #[test]
    fn hit_testing_finds_objects_the_fill_handle_the_border_and_cells() {
        let mut s = crate::Session::new();
        s.new_workbook();
        s.execute("range.setValues", serde_json::json!({"range": "A1", "values": [[1], [2], [3]]})).unwrap();
        let id =
            s.execute("insert.chart", serde_json::json!({"range": "A1:A3", "at": "D11", "type": "bar"})).unwrap()["chart"].as_u64().unwrap() as u32;
        let sh = s.doc().unwrap().wb.active().unwrap().clone();
        let g = Geometry::new(&sh, (0.0, 0.0), (1000.0, 800.0), (0.0, 0.0), 1.0);
        let mut sel = Selection::at(CellRef::new(0, 0));
        sel.ranges = vec![RangeRef::new(CellRef::new(0, 0), CellRef::new(1, 1))];
        let out = g.range_rect(&sh, sel.current());
        assert_eq!(g.hit(&sh, &sel, out.right(), out.bottom()), Hit::FillHandle);
        assert_eq!(g.hit(&sh, &sel, out.x + 20.0, out.y + 1.0), Hit::SelectionBorder);
        assert_eq!(g.hit(&sh, &sel, out.x + 20.0, out.y + 10.0), Hit::Cell { cell: CellRef::new(0, 0) });
        let chart = g.anchor_rect(&sh.charts[0].anchor);
        assert_eq!(g.hit(&sh, &sel, chart.x + 5.0, chart.y + 5.0), Hit::Object { object: "chart".into(), id, rect: chart });
        assert_eq!(g.col_edge(&sh, g.x(2) + 2.0).map(|e| e.0), Some(1));
        assert_eq!(g.col_edge(&sh, g.x(2) + 30.0), None);
    }

    #[test]
    fn fill_handle_and_resize_rules() {
        let src = RangeRef::new(CellRef::new(1, 1), CellRef::new(2, 2));
        assert_eq!(fill_target(src, CellRef::new(9, 3)).a1(), "B2:C10", "further down than right");
        assert_eq!(fill_target(src, CellRef::new(3, 9)).a1(), "B2:J3");
        assert_eq!(fill_target(src, CellRef::new(0, 1)).a1(), "B1:C3");
        assert_eq!(fill_target(src, CellRef::new(2, 2)), src);
        let mut sh = Sheet::new("S");
        for r in 0..5 {
            sh.set_value(CellRef::new(r, 0), gridcraft_core::Value::Number(1.0));
        }
        assert_eq!(fill_down_target(&sh, RangeRef::cell(CellRef::new(0, 1))).map(|r| r.a1()).as_deref(), Some("B1:B5"));
        assert_eq!(fill_down_target(&sh, RangeRef::cell(CellRef::new(4, 1))), None);
        let mut sel = Selection::at(CellRef::new(0, 2));
        assert_eq!(resize_columns(&sel, 4), "E:E");
        sel.ranges = vec![RangeRef::cols(2, 3), RangeRef::cols(6, 6)];
        assert_eq!(resize_columns(&sel, 3), "C:D,G:G");
        assert_eq!(resize_rows(&sel, 0), "1:1");
        assert_eq!(resize_label(true, 64.0), "Width: 8.43 (64 pixels)");
        assert_eq!(resize_label(false, f32::NAN), "Height: 0.00 (0 pixels)");
    }

    #[test]
    fn page_layout_puts_cells_on_pages() {
        let mut s = crate::Session::new();
        s.execute("file.new", serde_json::json!({"sample": "sales"})).unwrap();
        let d = s.active().unwrap();
        let g = Geometry::paged(&d.wb, 0, (0.0, 0.0), (1200.0, 900.0), (0.0, 0.0), 1.0).unwrap();
        let pages = g.pages();
        assert!(!pages.is_empty());
        assert!(pages[0].data);
        let a1 = g.cell_rect(d.wb.sheet(0).unwrap(), CellRef::new(0, 0));
        assert!(pages[0].content.contains(a1.x + 1.0, a1.y + 1.0), "A1 sits in the first page's margins");
        assert_eq!(g.cell_at(a1.x + 1.0, a1.y + 1.0), CellRef::new(0, 0));
        let panes = g.panes();
        assert!(!panes.is_empty() && panes[0].rows.0 == 0 && panes[0].cols.0 == 0);
        assert_eq!(g.frozen, (0, 0));
    }

    #[test]
    fn hostile_input_stays_in_range() {
        let sh = sheet();
        let g = Geometry::new(&sh, (0.0, 0.0), (f32::NAN, f32::INFINITY), (f32::NAN, -5.0), f32::NAN);
        assert!(g.col_at(f32::MAX) < MAX_COLS);
        assert!(g.row_at(f32::MIN) < MAX_ROWS);
        let _ = g.panes();
        let _ = g.reveal(&sh, CellRef::new(MAX_ROWS - 1, MAX_COLS - 1));
    }

    #[test]
    fn a_scroll_bar_spans_the_used_range_and_where_the_view_is() {
        let mut sh = Sheet::new("S");
        // Empty: to column Z, so a 10-column view can scroll 16 columns.
        let g = Geometry::new(&sh, (0.0, 0.0), (10.0 * sh.default_col_width, 100.0), (0.0, 0.0), 1.0);
        let (x, _) = g.scroll_extent(&sh);
        assert!((x - 16.0 * sh.default_col_width).abs() < 0.5, "{x}");
        // Data in column AZ (51): to column BJ (61).
        sh.set_value(CellRef::new(0, 51), gridcraft_core::Value::Number(1.0));
        assert_eq!(scroll_span(&sh).1, 61);
        // Scrolled past it, the bar reaches where the view is.
        let far = Geometry::new(&sh, (0.0, 0.0), (100.0, 100.0), (1.0e6, 0.0), 1.0);
        assert_eq!(far.scroll_extent(&sh).0, 1.0e6);
    }

    #[test]
    fn row_headers_keep_their_width_while_scrolling() {
        let mut sh = Sheet::new("S");
        // The scroll bar reaches row 101 at least: three digits from the top of the sheet on.
        assert_eq!(row_number_digits(&sh, 30), 3);
        assert_eq!(row_number_digits(&sh, 99), 3);
        assert_eq!(row_number_digits(&sh, 120), 3);
        // Scrolled far past it, or data that far down, widens them.
        assert_eq!(row_number_digits(&sh, 1_200), 4);
        sh.set_value(CellRef::new(5_000, 0), gridcraft_core::Value::Number(1.0));
        assert_eq!(row_number_digits(&sh, 30), 4);
        assert_eq!(row_number_digits(&sh, MAX_ROWS - 1), 7);
    }
}
