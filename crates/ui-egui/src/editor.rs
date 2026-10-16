//! In-cell / formula-bar editing state: point mode, reference colouring, function
//! autocomplete and argument hints.

use egui::Color32;
use gridcraft_engine::core::{CellRef, RangeRef};
use gridcraft_engine::entry;

/// Colours for references in a formula being edited (our own palette).
pub const REF_COLORS: [Color32; entry::REF_COLOR_COUNT] = [
    Color32::from_rgb(0x2E, 0x6F, 0xD8),
    Color32::from_rgb(0xD0, 0x3C, 0x3C),
    Color32::from_rgb(0x7A, 0x4F, 0xC9),
    Color32::from_rgb(0x2B, 0x9A, 0x4A),
    Color32::from_rgb(0xC2, 0x5E, 0x1E),
    Color32::from_rgb(0x1D, 0x92, 0xA6),
    Color32::from_rgb(0xB8, 0x3B, 0x8E),
    Color32::from_rgb(0x8A, 0x7A, 0x1F),
];

#[derive(Clone, Debug)]
pub struct EditState {
    pub sheet: usize,
    /// Syntax of this edit, captured when it starts (prevents a language switch reinterpreting it).
    pub locale: gridcraft_engine::formula::FormulaLocale,
    pub cell: CellRef,
    pub text: String,
    /// Started by typing (Enter mode: arrows commit) rather than F2 (Edit mode: arrows move the caret).
    pub enter_mode: bool,
    pub from_formula_bar: bool,
    /// Caret position (char index), mirrored from the text widget.
    pub caret: usize,
    /// The reference most recently inserted by point mode (byte range) — arrows/clicks replace it.
    pub point: Option<(usize, usize)>,
    /// Where point mode's keyboard cursor is.
    pub point_cell: Option<CellRef>,
    pub autocomplete: Vec<String>,
    pub ac_index: usize,
    pub request_focus: bool,
    /// The text changed programmatically: push `caret` into the widget on the next frame.
    pub sync_caret: bool,
    /// Column AutoComplete: the full entry the typed text completes to.
    pub completion: Option<String>,
    /// Move the caret to the end on the next frame.
    pub caret_to_end: bool,
}

impl EditState {
    pub fn new(sheet: usize, cell: CellRef, text: String, enter_mode: bool, from_formula_bar: bool) -> EditState {
        let caret = text.chars().count();
        EditState {
            sheet,
            locale: Default::default(),
            cell,
            text,
            enter_mode,
            from_formula_bar,
            caret,
            point: None,
            point_cell: None,
            autocomplete: vec![],
            ac_index: 0,
            request_focus: true,
            sync_caret: true,
            caret_to_end: true,
            completion: None,
        }
    }

    pub fn is_formula(&self) -> bool {
        entry::is_formula(&self.text)
    }

    /// Point mode: the caret follows an operator, `(`, `,` or `=` in a formula, so clicking a
    /// cell or pressing an arrow inserts a reference.
    pub fn can_point(&self) -> bool {
        entry::can_point(&self.text, self.caret, self.point)
    }

    /// Inserts (or replaces the last pointed) reference text at the caret.
    pub fn insert_ref(&mut self, r: &str) {
        self.apply(entry::insert_ref(&self.text, self.caret, self.point, r));
    }

    /// Updates function autocomplete for the word before the caret.
    pub fn update_autocomplete(&mut self, names: &[String]) {
        self.autocomplete = entry::autocomplete(&self.text, self.caret, names);
        self.ac_index = self.ac_index.min(self.autocomplete.len().saturating_sub(1));
    }

    /// Accepts the highlighted autocomplete entry: replaces the word and adds `(`.
    pub fn accept_autocomplete(&mut self) -> bool {
        let Some(name) = self.autocomplete.get(self.ac_index).cloned() else { return false };
        self.apply(entry::accept_autocomplete(&self.text, self.caret, &name));
        self.point = None;
        self.autocomplete.clear();
        true
    }

    /// F4: cycles the `$` anchors of the reference at the caret.
    pub fn cycle_anchor(&mut self) {
        if let Some(e) = entry::cycle_anchor(&self.text, self.caret) {
            self.apply(e);
            self.request_focus = true;
        }
    }

    /// The function call the caret is inside and the argument index (for the hint tooltip).
    pub fn current_function(&self) -> Option<(String, usize)> {
        entry::current_function(&self.text, self.caret, self.locale)
    }

    fn apply(&mut self, e: entry::Edit) {
        self.text = e.text;
        self.caret = e.caret;
        self.point = e.point;
        self.caret_to_end = false;
        self.sync_caret = true;
    }
}

/// References in formula text with their byte spans and colour index (same reference text ⇒
/// same colour), for colouring the editor and outlining ranges on the grid.
pub fn formula_refs(text: &str) -> Vec<(usize, usize, RangeRef, Option<String>, usize)> {
    entry::formula_refs(text).into_iter().map(|r| (r.start, r.end, r.range, r.sheet, r.color)).collect()
}

pub use entry::{column_completion, completion_rest};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_mode() {
        let mut e = EditState::new(0, CellRef::new(0, 0), "=SUM(".into(), true, false);
        assert!(e.can_point());
        e.insert_ref("A1");
        assert_eq!(e.text, "=SUM(A1");
        e.insert_ref("A2");
        assert_eq!(e.text, "=SUM(A2");
        e.text.push(':');
        e.caret += 1;
        e.point = None;
        assert!(e.can_point());
        let e2 = EditState::new(0, CellRef::new(0, 0), "hello".into(), true, false);
        assert!(!e2.can_point());
    }

    #[test]
    fn autocomplete_and_hints() {
        let names: Vec<String> = ["SUM", "SUMIF", "SUMPRODUCT", "AVERAGE"].iter().map(|s| s.to_string()).collect();
        let mut e = EditState::new(0, CellRef::new(0, 0), "=su".into(), true, false);
        e.update_autocomplete(&names);
        assert_eq!(e.autocomplete.len(), 3);
        assert!(e.accept_autocomplete());
        assert_eq!(e.text, "=SUM(");
        let e = EditState::new(0, CellRef::new(0, 0), "=IF(A1>0,SUM(B1,".into(), true, false);
        assert_eq!(e.current_function(), Some(("SUM".into(), 1)));
    }

    #[test]
    fn ref_spans() {
        let r = formula_refs("=A1+B2:C3*a1+Sheet2!D4");
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].4, r[2].4); // A1 and a1 share a colour
        assert_eq!(r[1].2.a1(), "B2:C3");
        assert_eq!(r[3].3.as_deref(), Some("Sheet2"));
    }
}
