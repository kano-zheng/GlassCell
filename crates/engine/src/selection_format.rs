//! The selection's formatting, as a toolbar, an inspector or Format Cells shows it: the active
//! cell's style (what toggles such as Bold flip), and for every property the value all selected
//! cells share, or `None` when they differ ("mixed").
//!
//! This is the cells' own formatting (what Format Cells edits), not table styles or conditional
//! formats. Borders are read as drawn: an edge set on either of the two cells beside it counts.

use std::collections::BTreeSet;

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_model::{BorderLine, Color, Fill, HAlign, PatternType, Sheet, Style, StyleId, Underline, VAlign, VertAlign, Workbook};
use serde::Serialize;

use crate::Selection;
use crate::viewport::Rgb;

/// Selections up to this many cells have their inside borders compared cell by cell; larger
/// ones are summarized from their distinct styles.
const EXACT_CELLS: u64 = 250_000;
/// Edges up to this many cells long are compared cell by cell.
const EXACT_EDGE: u32 = 65_536;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionFormat {
    /// The active cell's style. Toggles (Bold, Italic, Wrap Text…) act on this.
    pub active: Style,
    /// Number of selected cells (overlapping areas counted twice).
    pub cells: u64,
    pub font: FontFormat,
    pub fill: FillFormat,
    pub alignment: AlignmentFormat,
    pub number: NumberFormatState,
    pub borders: BorderFormat,
    pub protection: ProtectionFormat,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFormat {
    pub name: Option<String>,
    pub size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: Option<bool>,
    /// Superscript / subscript.
    pub vert: Option<VertAlign>,
    pub color: Option<Color>,
    /// `color` resolved against the theme; `None` also for Automatic.
    pub rgb: Option<Rgb>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FillFormat {
    pub fill: Option<Fill>,
    /// A shared solid fill's colour, resolved against the theme.
    pub rgb: Option<Rgb>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlignmentFormat {
    pub h: Option<HAlign>,
    pub v: Option<VAlign>,
    pub wrap: Option<bool>,
    pub shrink: Option<bool>,
    pub indent: Option<u8>,
    pub rotation: Option<i16>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberFormatState {
    pub code: Option<String>,
    /// The category the code belongs to (`General`, `Currency`, `Date`, `Custom`…).
    pub kind: Option<String>,
}

/// Lines along the selection's outline and between its cells. An inside line is `None` when it
/// is mixed or when there is none (a single row has no inside horizontal lines: `has_inside_h`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BorderFormat {
    pub top: Option<BorderLine>,
    pub bottom: Option<BorderLine>,
    pub left: Option<BorderLine>,
    pub right: Option<BorderLine>,
    pub inside_h: Option<BorderLine>,
    pub inside_v: Option<BorderLine>,
    pub has_inside_h: bool,
    pub has_inside_v: bool,
    pub diag_down: Option<BorderLine>,
    pub diag_up: Option<BorderLine>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionFormat {
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
}

/// The style of the selection's active cell on `sheet` (the default style without that sheet).
pub fn active_style(wb: &Workbook, sheet: usize, sel: &Selection) -> Style {
    wb.sheet(sheet).map(|sh| wb.styles.get(sh.style_id(sel.active)).clone()).unwrap_or_default()
}

/// The formatting of `sel` on `sheet`.
pub fn selection_format(wb: &Workbook, sheet: usize, sel: &Selection) -> SelectionFormat {
    let Some(sh) = wb.sheet(sheet) else { return SelectionFormat::default() };
    let ranges: Vec<RangeRef> = sel.ranges.iter().map(|r| clamp(*r)).collect();
    let ids = distinct_styles(sh, &ranges);
    let styles: Vec<&Style> = ids.iter().map(|id| wb.styles.get(*id)).collect();
    let color: Option<Color> = uniform(styles.iter().map(|s| s.font.color));
    let fill: Option<Fill> = uniform(styles.iter().map(|s| s.fill));
    let code: Option<String> = uniform(styles.iter().map(|s| s.num_fmt.as_str().to_string()));
    let kind = code.as_deref().map(|c| format!("{:?}", crate::display::number_format(c).kind()));
    SelectionFormat {
        active: active_style(wb, sheet, sel),
        cells: ranges.iter().map(RangeRef::count).sum(),
        font: FontFormat {
            name: uniform(styles.iter().map(|s| s.font.name.clone())),
            size: uniform(styles.iter().map(|s| s.font.size)),
            bold: uniform(styles.iter().map(|s| s.font.bold)),
            italic: uniform(styles.iter().map(|s| s.font.italic)),
            underline: uniform(styles.iter().map(|s| s.font.underline)),
            strike: uniform(styles.iter().map(|s| s.font.strike)),
            vert: uniform(styles.iter().map(|s| s.font.vert)),
            rgb: color.and_then(|c| c.resolve(&wb.theme)),
            color,
        },
        fill: FillFormat { rgb: fill.filter(|f| f.pattern == PatternType::Solid).and_then(|f| f.fg.resolve(&wb.theme)), fill },
        alignment: AlignmentFormat {
            h: uniform(styles.iter().map(|s| s.align.h)),
            v: uniform(styles.iter().map(|s| s.align.v)),
            wrap: uniform(styles.iter().map(|s| s.align.wrap)),
            shrink: uniform(styles.iter().map(|s| s.align.shrink)),
            indent: uniform(styles.iter().map(|s| s.align.indent)),
            rotation: uniform(styles.iter().map(|s| s.align.rotation)),
        },
        number: NumberFormatState { code, kind },
        borders: borders(wb, sh, &ranges, &styles),
        protection: ProtectionFormat {
            locked: uniform(styles.iter().map(|s| s.protection.locked)),
            hidden: uniform(styles.iter().map(|s| s.protection.hidden)),
        },
    }
}

/// The value every item shares, or `None` when they differ (or there are none).
fn uniform<T: PartialEq>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.all(|x| x == first).then_some(first)
}

fn clamp(r: RangeRef) -> RangeRef {
    let c = |x: CellRef| CellRef::new(x.row.min(MAX_ROWS - 1), x.col.min(MAX_COLS - 1));
    RangeRef::new(c(r.start), c(r.end))
}

/// The styles in use across `ranges`: stored cells', and for the cells not stored, their row's,
/// column's or the default style.
fn distinct_styles(sh: &Sheet, ranges: &[RangeRef]) -> BTreeSet<StyleId> {
    let mut ids = BTreeSet::new();
    for r in ranges {
        if r.count() <= EXACT_CELLS {
            ids.extend(r.iter().map(|c| sh.style_id(c)));
            continue;
        }
        let mut stored = 0u64;
        for (_, cell) in sh.cells.iter_range(*r) {
            ids.insert(cell.style);
            stored += 1;
        }
        if stored >= r.count() {
            continue;
        }
        let rows = sh.rows.range(r.start.row..=r.end.row).filter_map(|(_, l)| l.style);
        let styled_rows = rows.clone().count() as u64;
        ids.extend(rows);
        if styled_rows < u64::from(r.end.row - r.start.row) + 1 {
            let cols = sh.cols.range(r.start.col..=r.end.col).filter_map(|(_, l)| l.style);
            let styled_cols = cols.clone().count() as u64;
            ids.extend(cols);
            if styled_cols < u64::from(r.end.col - r.start.col) + 1 {
                ids.insert(StyleId::DEFAULT);
            }
        }
    }
    ids
}

/// Collects one value per item and tells whether they all agree.
struct Agree<T> {
    first: Option<T>,
    mixed: bool,
}

impl<T: PartialEq> Agree<T> {
    fn new() -> Self {
        Agree { first: None, mixed: false }
    }
    fn add(&mut self, v: T) {
        match &self.first {
            None => self.first = Some(v),
            Some(f) if *f != v => self.mixed = true,
            Some(_) => {}
        }
    }
    fn get(self) -> Option<T> {
        if self.mixed { None } else { self.first }
    }
}

/// The line drawn between two cells: the first one's side, or else the second one's.
fn shared(a: BorderLine, b: BorderLine) -> BorderLine {
    if a.is_none() { b } else { a }
}

fn borders(wb: &Workbook, sh: &Sheet, ranges: &[RangeRef], styles: &[&Style]) -> BorderFormat {
    let b = |c: CellRef| wb.styles.get(sh.style_id(c)).borders;
    let (mut top, mut bottom, mut left, mut right) = (Agree::new(), Agree::new(), Agree::new(), Agree::new());
    let (mut inside_h, mut inside_v) = (Agree::new(), Agree::new());
    let (mut has_h, mut has_v) = (false, false);
    for r in ranges {
        let (rows, cols) = (r.start.row..=r.end.row, r.start.col..=r.end.col);
        if r.end.col - r.start.col < EXACT_EDGE {
            for col in cols.clone() {
                let above = r.start.row.checked_sub(1).map(|row| b(CellRef::new(row, col)).bottom).unwrap_or_default();
                top.add(shared(b(CellRef::new(r.start.row, col)).top, above));
                let below = if r.end.row + 1 < MAX_ROWS { b(CellRef::new(r.end.row + 1, col)).top } else { BorderLine::default() };
                bottom.add(shared(b(CellRef::new(r.end.row, col)).bottom, below));
            }
        } else {
            styles.iter().for_each(|s| top.add(s.borders.top));
            styles.iter().for_each(|s| bottom.add(s.borders.bottom));
        }
        if r.end.row - r.start.row < EXACT_EDGE {
            for row in rows.clone() {
                let before = r.start.col.checked_sub(1).map(|col| b(CellRef::new(row, col)).right).unwrap_or_default();
                left.add(shared(b(CellRef::new(row, r.start.col)).left, before));
                let after = if r.end.col + 1 < MAX_COLS { b(CellRef::new(row, r.end.col + 1)).left } else { BorderLine::default() };
                right.add(shared(b(CellRef::new(row, r.end.col)).right, after));
            }
        } else {
            styles.iter().for_each(|s| left.add(s.borders.left));
            styles.iter().for_each(|s| right.add(s.borders.right));
        }
        has_h |= r.end.row > r.start.row;
        has_v |= r.end.col > r.start.col;
        if r.count() <= EXACT_CELLS {
            for c in r.iter() {
                let here = b(c);
                if c.row < r.end.row {
                    inside_h.add(shared(here.bottom, b(CellRef::new(c.row + 1, c.col)).top));
                }
                if c.col < r.end.col {
                    inside_v.add(shared(here.right, b(CellRef::new(c.row, c.col + 1)).left));
                }
            }
        } else {
            styles.iter().for_each(|s| inside_h.add(shared(s.borders.bottom, s.borders.top)));
            styles.iter().for_each(|s| inside_v.add(shared(s.borders.right, s.borders.left)));
        }
    }
    BorderFormat {
        top: top.get(),
        bottom: bottom.get(),
        left: left.get(),
        right: right.get(),
        inside_h: inside_h.get(),
        inside_v: inside_v.get(),
        has_inside_h: has_h,
        has_inside_v: has_v,
        diag_down: uniform(styles.iter().map(|s| s.borders.diag_down)),
        diag_up: uniform(styles.iter().map(|s| s.borders.diag_up)),
    }
}

#[cfg(test)]
mod tests {
    use gridcraft_model::BorderStyle;
    use serde_json::{Value as Json, json};

    use crate::Session;

    fn session() -> Session {
        let mut s = Session::new();
        s.new_workbook();
        s
    }

    fn format(s: &mut Session, range: &str) -> Json {
        s.execute("selection.set", json!({"range": range})).unwrap();
        s.execute("selection.format", json!({})).unwrap()
    }

    #[test]
    fn shared_values_show_and_differing_ones_are_mixed() {
        let mut s = session();
        s.execute("home.bold", json!({"range": "A1:B2", "on": true})).unwrap();
        s.execute("home.italic", json!({"range": "A1", "on": true})).unwrap();
        s.execute("home.fillColor", json!({"range": "A1:B2", "color": "#FFFF00"})).unwrap();
        let f = format(&mut s, "A1:B2");
        assert_eq!(f["cells"], 4);
        assert_eq!(f["font"]["bold"], true);
        assert_eq!(f["font"]["italic"], Json::Null, "mixed");
        assert_eq!(f["font"]["size"], 11.0);
        assert_eq!(f["fill"]["rgb"], json!([255, 255, 0]));
        assert_eq!(f["number"]["kind"], "General");
        // The active cell's own style is what a toggle flips.
        assert_eq!(f["active"]["font"]["italic"], true);
        // Beyond the formatted cells, bold is mixed too.
        let f = format(&mut s, "A1:C3");
        assert_eq!(f["font"]["bold"], Json::Null);
        assert_eq!(f["font"]["name"], "Calibri");
    }

    #[test]
    fn number_format_kind_follows_the_code() {
        let mut s = session();
        s.execute("home.numberFormat", json!({"range": "A1:A3", "format": "Percentage"})).unwrap();
        let f = format(&mut s, "A1:A3");
        assert_eq!(f["number"]["kind"], "Percentage");
        assert_eq!(format(&mut s, "A1:A4")["number"]["code"], Json::Null);
    }

    #[test]
    fn borders_are_read_as_drawn_on_the_outline_and_inside() {
        let mut s = session();
        s.execute("home.borders", json!({"range": "B2:C3", "preset": "all"})).unwrap();
        let f = format(&mut s, "B2:C3");
        let thin = |v: &Json| v["style"] == json!(BorderStyle::Thin);
        for edge in ["top", "bottom", "left", "right", "insideH", "insideV"] {
            assert!(thin(&f["borders"][edge]), "{edge}: {}", f["borders"]);
        }
        assert_eq!(f["borders"]["hasInsideH"], true);
        // Below the box, B4's top edge is B3's bottom line.
        let f = format(&mut s, "B4");
        assert!(thin(&f["borders"]["top"]), "{}", f["borders"]);
        assert_eq!(f["borders"]["bottom"]["style"], json!(BorderStyle::None));
        assert_eq!(f["borders"]["hasInsideH"], false);
        // Half of the inside lines present: mixed.
        let f = format(&mut s, "B2:C5");
        assert_eq!(f["borders"]["insideH"], Json::Null);
    }

    #[test]
    fn whole_columns_and_the_whole_sheet_are_summarized() {
        let mut s = session();
        s.execute("home.bold", json!({"range": "A:A", "on": true})).unwrap();
        s.execute("cell.set", json!({"cell": "A5", "input": "x"})).unwrap();
        let f = format(&mut s, "A:A");
        assert_eq!(f["cells"], 1_048_576);
        assert_eq!(f["font"]["bold"], true);
        let f = format(&mut s, "A1:XFD1048576");
        assert_eq!(f["font"]["bold"], Json::Null);
        assert_eq!(f["font"]["size"], 11.0);
    }
}
