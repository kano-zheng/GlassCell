//! The records and enums Swift sees, and their conversions from the engine's types. Names avoid
//! Swift's and Foundation's (`CellRange`, not `Range`; `GridRect`, not `Rect`).

use gridcraft_engine::core::{CellRef, RangeRef};
use gridcraft_engine::model::{self as model, Color, PatternType, Style, Workbook};
use gridcraft_engine::{geometry as geo, viewport as vp};

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GridPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GridSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GridRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<geo::Rect> for GridRect {
    fn from(r: geo::Rect) -> Self {
        GridRect { x: r.x, y: r.y, width: r.w, height: r.h }
    }
}

/// A cell (0-based row and column).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CellAddress {
    pub row: u32,
    pub col: u32,
}

impl From<CellRef> for CellAddress {
    fn from(c: CellRef) -> Self {
        CellAddress { row: c.row, col: c.col }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CellRange {
    pub first: CellAddress,
    pub last: CellAddress,
}

impl From<RangeRef> for CellRange {
    fn from(r: RangeRef) -> Self {
        CellRange { first: r.start.into(), last: r.end.into() }
    }
}

/// A run of rows or columns, first and last included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LineSpan {
    pub first: u32,
    pub last: u32,
}

impl From<(u32, u32)> for LineSpan {
    fn from((first, last): (u32, u32)) -> Self {
        LineSpan { first, last }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<[u8; 3]> for Rgb {
    fn from([r, g, b]: [u8; 3]) -> Self {
        Rgb { r, g, b }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, uniffi::Record)]
pub struct FontFace {
    pub name: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HAlign {
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterAcross,
    Distributed,
}

impl From<model::HAlign> for HAlign {
    fn from(h: model::HAlign) -> Self {
        use model::HAlign as H;
        match h {
            H::General => HAlign::General,
            H::Left => HAlign::Left,
            H::Center => HAlign::Center,
            H::Right => HAlign::Right,
            H::Fill => HAlign::Fill,
            H::Justify => HAlign::Justify,
            H::CenterAcross => HAlign::CenterAcross,
            H::Distributed => HAlign::Distributed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
    Justify,
    Distributed,
}

impl From<model::VAlign> for VAlign {
    fn from(v: model::VAlign) -> Self {
        use model::VAlign as V;
        match v {
            V::Top => VAlign::Top,
            V::Center => VAlign::Center,
            V::Bottom => VAlign::Bottom,
            V::Justify => VAlign::Justify,
            V::Distributed => VAlign::Distributed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Underline {
    None,
    Single,
    Double,
    SingleAccounting,
    DoubleAccounting,
}

impl From<model::Underline> for Underline {
    fn from(u: model::Underline) -> Self {
        use model::Underline as U;
        match u {
            U::None => Underline::None,
            U::Single => Underline::Single,
            U::Double => Underline::Double,
            U::SingleAccounting => Underline::SingleAccounting,
            U::DoubleAccounting => Underline::DoubleAccounting,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum BorderStyle {
    None,
    Thin,
    Medium,
    Thick,
    Dashed,
    Dotted,
    Double,
    Hair,
    MediumDashed,
    DashDot,
    MediumDashDot,
    DashDotDot,
    MediumDashDotDot,
    SlantDashDot,
}

impl From<model::BorderStyle> for BorderStyle {
    fn from(s: model::BorderStyle) -> Self {
        use model::BorderStyle as B;
        match s {
            B::None => BorderStyle::None,
            B::Thin => BorderStyle::Thin,
            B::Medium => BorderStyle::Medium,
            B::Thick => BorderStyle::Thick,
            B::Dashed => BorderStyle::Dashed,
            B::Dotted => BorderStyle::Dotted,
            B::Double => BorderStyle::Double,
            B::Hair => BorderStyle::Hair,
            B::MediumDashed => BorderStyle::MediumDashed,
            B::DashDot => BorderStyle::DashDot,
            B::MediumDashDot => BorderStyle::MediumDashDot,
            B::DashDotDot => BorderStyle::DashDotDot,
            B::MediumDashDotDot => BorderStyle::MediumDashDotDot,
            B::SlantDashDot => BorderStyle::SlantDashDot,
        }
    }
}

// ------------------------------------------------------------------ viewport

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ViewportQuery {
    /// Sheet index; the active sheet when `None`.
    pub sheet: Option<u32>,
    pub rows: LineSpan,
    pub cols: LineSpan,
    /// Screen units per sheet unit (zoom × display scale).
    pub scale: f32,
    /// Formula syntax for Show Formulas: `en` or `es`.
    pub formula_locale: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Viewport {
    pub rows: LineSpan,
    pub cols: LineSpan,
    /// Cells with something to draw; a merged area once, at its first visible cell.
    pub cells: Vec<CellView>,
    pub sparklines: Vec<SparklineView>,
    pub filter_buttons: Vec<FilterButton>,
}

impl From<vp::Viewport> for Viewport {
    fn from(v: vp::Viewport) -> Self {
        Viewport {
            rows: v.rows.into(),
            cols: v.cols.into(),
            cells: v.cells.into_iter().map(CellView::from).collect(),
            sparklines: v.sparklines.into_iter().map(SparklineView::from).collect(),
            filter_buttons: v.filter_buttons.into_iter().map(|b| FilterButton { cell: b.cell.into(), active: b.active }).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CellView {
    pub cell: CellAddress,
    /// The area painted: the merged area, or the cell.
    pub span: CellRange,
    /// A merged area: painted blank (no inner gridlines) even without a fill.
    pub merged: bool,
    pub fill: Option<Rgb>,
    pub data_bar: Option<DataBar>,
    pub icon: Option<CfIcon>,
    /// A checkbox (checked or not), drawn instead of text.
    pub checkbox: Option<bool>,
    /// The cell holds a picture, drawn instead of text.
    pub picture: bool,
    pub text: Option<CellText>,
    /// Border lines, drawn over fills and text in this order.
    pub borders: Vec<BorderSegment>,
    pub comment: Option<CommentMark>,
}

impl From<vp::CellView> for CellView {
    fn from(c: vp::CellView) -> Self {
        CellView {
            cell: c.cell.into(),
            span: c.span.into(),
            merged: c.merged,
            fill: c.fill.map(Rgb::from),
            data_bar: c.data_bar.map(|b| DataBar { fraction: b.fraction, color: b.color.into(), gradient: b.gradient }),
            icon: c.icon.map(|i| CfIcon {
                count: u32::try_from(gridcraft_engine::cf::icon_count(&i.set)).unwrap_or(u32::MAX),
                set: i.set,
                index: u32::try_from(i.index).unwrap_or(u32::MAX),
            }),
            checkbox: c.checkbox,
            picture: c.picture,
            text: c.text.map(CellText::from),
            borders: c.borders.into_iter().map(BorderSegment::from).collect(),
            comment: c.comment.map(|m| match m {
                vp::CommentMark::Note => CommentMark::Note,
                vp::CommentMark::Threaded => CommentMark::Threaded,
            }),
        }
    }
}

/// A cell's text, laid out. Lengths are screen units from the left of the cell's span.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CellText {
    pub text: String,
    pub face: FontFace,
    /// Font size in screen units (zoomed; smaller under Shrink to fit).
    pub size: f32,
    pub underline: Underline,
    pub strike: bool,
    pub color: Rgb,
    /// The horizontal alignment General resolves to.
    pub halign: HAlign,
    /// The cell's own horizontal alignment (rotated text places itself by it).
    pub own_halign: HAlign,
    pub valign: VAlign,
    /// Degrees counter-clockwise, -90…90 (255: stacked, drawn as a line).
    pub rotation: i16,
    pub layout: TextLayout,
    /// The content area across the span (after a conditional-format icon, before a filter button).
    pub left: f32,
    pub right: f32,
    pub pad: f32,
    pub indent: f32,
    /// One line and rotated text: the text's left edge and width.
    pub x: f32,
    pub width: f32,
    /// Columns the text may be drawn over (clip to them) when it overflows its cell.
    pub spill: Option<LineSpan>,
    /// With `spill`: cover the gridlines the text crosses with the sheet background.
    pub cover_gridlines: bool,
}

impl From<vp::CellText> for CellText {
    fn from(t: vp::CellText) -> Self {
        CellText {
            face: FontFace { name: t.font.name.clone(), bold: t.font.bold, italic: t.font.italic },
            underline: t.font.underline.into(),
            strike: t.font.strike,
            text: t.text,
            size: t.size,
            color: t.color.into(),
            halign: t.halign.into(),
            own_halign: t.align.h.into(),
            valign: t.align.v.into(),
            rotation: t.align.rotation,
            layout: match t.layout {
                vp::TextLayout::Line => TextLayout::Line,
                vp::TextLayout::Wrap { width, anchor } => TextLayout::Wrap { width, anchor },
                vp::TextLayout::Rotated { wrap } => TextLayout::Rotated { wrap },
            },
            left: t.left,
            right: t.right,
            pad: t.pad,
            indent: t.indent,
            x: t.x,
            width: t.width,
            spill: t.spill.map(LineSpan::from),
            cover_gridlines: t.cover_gridlines,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum TextLayout {
    /// One line at `x`, clipped to the content area (or `spill`).
    Line,
    /// Wrapped to `width`, aligned by `halign` at `anchor` (left edge, centre or right edge).
    Wrap { width: f32, anchor: f32 },
    /// Rotated by `rotation`; upright wrapped text wraps to `wrap`.
    Rotated { wrap: Option<f32> },
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct DataBar {
    pub fraction: f32,
    pub color: Rgb,
    pub gradient: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CfIcon {
    pub set: String,
    /// 0 = lowest.
    pub index: u32,
    /// Icons in the set.
    pub count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum BorderEdge {
    Top,
    Bottom,
    Left,
    Right,
    DiagDown,
    DiagUp,
}

/// A border line along columns `from…to` (top, bottom) or rows `from…to` (left, right) of a
/// span's edge; diagonals cross the whole span.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct BorderSegment {
    pub edge: BorderEdge,
    pub from: u32,
    pub to: u32,
    pub style: BorderStyle,
    /// Line width at 100% (screen units before zoom).
    pub width: f32,
    pub color: Rgb,
}

impl From<vp::BorderSeg> for BorderSegment {
    fn from(b: vp::BorderSeg) -> Self {
        let edge = match b.edge {
            vp::Edge::Top => BorderEdge::Top,
            vp::Edge::Bottom => BorderEdge::Bottom,
            vp::Edge::Left => BorderEdge::Left,
            vp::Edge::Right => BorderEdge::Right,
            vp::Edge::DiagDown => BorderEdge::DiagDown,
            vp::Edge::DiagUp => BorderEdge::DiagUp,
        };
        BorderSegment { edge, from: b.from, to: b.to, style: b.style.into(), width: b.style.width(), color: b.color.into() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CommentMark {
    Note,
    Threaded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SparklineKind {
    Line,
    Column,
    WinLoss,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SparklineView {
    pub cell: CellAddress,
    pub kind: SparklineKind,
    pub values: Vec<Option<f64>>,
    pub color: Rgb,
    pub markers: bool,
}

impl From<vp::SparklineView> for SparklineView {
    fn from(s: vp::SparklineView) -> Self {
        let kind = match s.kind {
            model::SparklineKind::Line => SparklineKind::Line,
            model::SparklineKind::Column => SparklineKind::Column,
            model::SparklineKind::WinLoss => SparklineKind::WinLoss,
        };
        SparklineView { cell: s.cell.into(), kind, values: s.values, color: s.color.into(), markers: s.markers }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FilterButton {
    pub cell: CellAddress,
    pub active: bool,
}

// ------------------------------------------------------------------ selection format

/// A colour as set: Automatic, or resolved against the theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ColorValue {
    Automatic,
    Rgb { rgb: Rgb },
}

fn color_value(c: Color, wb: &Workbook) -> ColorValue {
    match c.resolve(&wb.theme) {
        Some(rgb) if c != Color::Auto => ColorValue::Rgb { rgb: rgb.into() },
        _ => ColorValue::Automatic,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FillValue {
    None,
    Solid {
        rgb: Rgb,
    },
    /// A pattern (the Format Cells Fill page edits it).
    Pattern,
}

fn fill_value(f: model::Fill, wb: &Workbook) -> FillValue {
    match f.pattern {
        PatternType::None => FillValue::None,
        PatternType::Solid => f.fg.resolve(&wb.theme).map_or(FillValue::None, |rgb| FillValue::Solid { rgb: rgb.into() }),
        _ => FillValue::Pattern,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct BorderLine {
    pub style: BorderStyle,
    pub color: ColorValue,
}

fn border_line(b: model::BorderLine, wb: &Workbook) -> BorderLine {
    BorderLine { style: b.style.into(), color: color_value(b.color, wb) }
}

/// Formatting where `None` means the selected cells differ ("mixed").
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FormatState {
    pub font_name: Option<String>,
    pub font_size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: Option<bool>,
    pub font_color: Option<ColorValue>,
    pub fill: Option<FillValue>,
    pub halign: Option<HAlign>,
    pub valign: Option<VAlign>,
    pub wrap: Option<bool>,
    pub shrink: Option<bool>,
    pub indent: Option<u8>,
    pub rotation: Option<i16>,
    pub number_format: Option<String>,
    /// The number format's category (`General`, `Currency`, `Date`, `Custom`…).
    pub number_kind: Option<String>,
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
}

impl FormatState {
    fn of_style(st: &Style, wb: &Workbook) -> Self {
        let code = st.num_fmt.as_str().to_string();
        FormatState {
            font_name: Some(st.font.name.clone()),
            font_size: Some(st.font.size),
            bold: Some(st.font.bold),
            italic: Some(st.font.italic),
            underline: Some(st.font.underline.into()),
            strike: Some(st.font.strike),
            font_color: Some(color_value(st.font.color, wb)),
            fill: Some(fill_value(st.fill, wb)),
            halign: Some(st.align.h.into()),
            valign: Some(st.align.v.into()),
            wrap: Some(st.align.wrap),
            shrink: Some(st.align.shrink),
            indent: Some(st.align.indent),
            rotation: Some(st.align.rotation),
            number_kind: Some(format!("{:?}", gridcraft_engine::display::number_format(&code).kind())),
            number_format: Some(code),
            locked: Some(st.protection.locked),
            hidden: Some(st.protection.hidden),
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct BorderFormat {
    pub top: Option<BorderLine>,
    pub bottom: Option<BorderLine>,
    pub left: Option<BorderLine>,
    pub right: Option<BorderLine>,
    /// Lines between rows / columns: `None` when mixed, or when there are none (`has_inside_*`).
    pub inside_h: Option<BorderLine>,
    pub inside_v: Option<BorderLine>,
    pub has_inside_h: bool,
    pub has_inside_v: bool,
    pub diag_down: Option<BorderLine>,
    pub diag_up: Option<BorderLine>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SelectionFormat {
    /// The active cell's formatting (never mixed): what toggles such as Bold flip.
    pub active: FormatState,
    /// Number of selected cells.
    pub cells: u64,
    /// What the selected cells share.
    pub selection: FormatState,
    pub borders: BorderFormat,
}

impl SelectionFormat {
    pub(crate) fn new(f: &gridcraft_engine::selection_format::SelectionFormat, wb: &Workbook) -> Self {
        let line = |b: Option<model::BorderLine>| b.map(|b| border_line(b, wb));
        let (font, al) = (&f.font, &f.alignment);
        SelectionFormat {
            active: FormatState::of_style(&f.active, wb),
            cells: f.cells,
            selection: FormatState {
                font_name: font.name.clone(),
                font_size: font.size,
                bold: font.bold,
                italic: font.italic,
                underline: font.underline.map(Underline::from),
                strike: font.strike,
                font_color: font.color.map(|c| color_value(c, wb)),
                fill: f.fill.fill.map(|x| fill_value(x, wb)),
                halign: al.h.map(HAlign::from),
                valign: al.v.map(VAlign::from),
                wrap: al.wrap,
                shrink: al.shrink,
                indent: al.indent,
                rotation: al.rotation,
                number_format: f.number.code.clone(),
                number_kind: f.number.kind.clone(),
                locked: f.protection.locked,
                hidden: f.protection.hidden,
            },
            borders: BorderFormat {
                top: line(f.borders.top),
                bottom: line(f.borders.bottom),
                left: line(f.borders.left),
                right: line(f.borders.right),
                inside_h: line(f.borders.inside_h),
                inside_v: line(f.borders.inside_v),
                has_inside_h: f.borders.has_inside_h,
                has_inside_v: f.borders.has_inside_v,
                diag_down: line(f.borders.diag_down),
                diag_up: line(f.borders.diag_up),
            },
        }
    }
}

// ------------------------------------------------------------------ session

/// Where editing starts: the cell and the text the editor opens with.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EntryStart {
    pub cell: CellAddress,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct History {
    /// Undo labels, most recent first.
    pub undo: Vec<String>,
    /// Redo labels, the next redo first.
    pub redo: Vec<String>,
    /// Moves whenever an undoable command changes the workbook.
    pub revision: u64,
    /// Changed since it was opened or saved.
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SelectionState {
    pub active: CellAddress,
    /// The selected areas; the last holds the active cell.
    pub ranges: Vec<CellRange>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SheetInfo {
    pub index: u32,
    pub name: String,
    pub sheet_names: Vec<String>,
    /// Percent, 10–400.
    pub zoom: u16,
    pub show_gridlines: bool,
    pub show_headings: bool,
    /// Frozen rows (`row`) and columns (`col`).
    pub frozen: CellAddress,
    pub right_to_left: bool,
    /// Normal view (not Page Layout or Page Break Preview).
    pub normal_view: bool,
    pub default_row_height: f32,
    pub default_col_width: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum UiRequest {
    /// Open dialog `name` (an inspector page or sheet in the Mac app) with JSON `params`.
    Dialog {
        name: String,
        params: String,
    },
    Message {
        text: String,
    },
    /// Start editing the active cell, optionally with this text.
    EditCell {
        text: Option<String>,
    },
    OpenUrl {
        url: String,
    },
}

impl From<gridcraft_engine::UiRequest> for UiRequest {
    fn from(r: gridcraft_engine::UiRequest) -> Self {
        use gridcraft_engine::UiRequest as R;
        match r {
            R::Dialog(name, params) => UiRequest::Dialog { name, params: params.to_string() },
            R::Message(text) => UiRequest::Message { text },
            R::EditCell(text) => UiRequest::EditCell { text },
            R::OpenUrl(url) => UiRequest::OpenUrl { url },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandInfo {
    pub id: String,
    pub label: String,
    /// The Excel ribbon path (tab, group…); the Mac placement table maps it.
    pub menu: Vec<String>,
    pub shortcut: Option<String>,
    /// The parameters, documented.
    pub params: String,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}

impl From<gridcraft_engine::CommandInfo> for CommandInfo {
    fn from(c: gridcraft_engine::CommandInfo) -> Self {
        CommandInfo {
            id: c.id.into(),
            label: c.label.into(),
            menu: c.menu.into_iter().map(String::from).collect(),
            shortcut: c.shortcut.map(String::from),
            params: c.params.into(),
            enabled: c.enabled,
            disabled_reason: c.disabled_reason,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoticeKind {
    Info,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Notice {
    pub text: String,
    pub kind: NoticeKind,
    /// A button: its label and the dialog it opens.
    pub action_label: Option<String>,
    pub action_dialog: Option<String>,
}

impl From<gridcraft_engine::notice::Notice> for Notice {
    fn from(n: gridcraft_engine::notice::Notice) -> Self {
        let kind = match n.kind {
            gridcraft_engine::notice::NoticeKind::Info => NoticeKind::Info,
            gridcraft_engine::notice::NoticeKind::Warning => NoticeKind::Warning,
        };
        let (action_label, action_dialog) = n.action.map_or((None, None), |(l, d)| (Some(l), Some(d)));
        Notice { text: n.text, kind, action_label, action_dialog }
    }
}

/// What a paste pastes.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PasteSource {
    /// GridCraft's own clipboard: `edit.paste` without text.
    Internal,
    /// This system clipboard text (with its HTML flavour).
    Text { text: String },
}

impl From<gridcraft_engine::host_clipboard::Paste> for PasteSource {
    fn from(p: gridcraft_engine::host_clipboard::Paste) -> Self {
        match p {
            gridcraft_engine::host_clipboard::Paste::Internal => PasteSource::Internal,
            gridcraft_engine::host_clipboard::Paste::Text(text) => PasteSource::Text { text },
        }
    }
}

// ------------------------------------------------------------------ geometry

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GridPane {
    pub rows: LineSpan,
    pub cols: LineSpan,
    pub rect: GridRect,
}

impl From<geo::Pane> for GridPane {
    fn from(p: geo::Pane) -> Self {
        GridPane { rows: p.rows.into(), cols: p.cols.into(), rect: p.rect.into() }
    }
}

/// A row or column edge: the row or column, and where its edge is.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct LineEdge {
    pub index: u32,
    pub at: f32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum GridHit {
    /// A floating object: `chart`, `image`, `slicer` or `shape`.
    Object {
        object: String,
        id: u32,
        rect: GridRect,
    },
    FillHandle,
    SelectionBorder,
    FilterButton {
        cell: CellAddress,
    },
    Cell {
        cell: CellAddress,
    },
}

impl From<geo::Hit> for GridHit {
    fn from(h: geo::Hit) -> Self {
        match h {
            geo::Hit::Object { object, id, rect } => GridHit::Object { object, id, rect: rect.into() },
            geo::Hit::FillHandle => GridHit::FillHandle,
            geo::Hit::SelectionBorder => GridHit::SelectionBorder,
            geo::Hit::FilterButton { cell } => GridHit::FilterButton { cell: cell.into() },
            geo::Hit::Cell { cell } => GridHit::Cell { cell: cell.into() },
        }
    }
}
