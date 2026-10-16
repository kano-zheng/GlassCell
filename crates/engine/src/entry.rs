//! Cell entry rules shared by every frontend's cell editor and formula bar: what an edit starts
//! from, point mode (clicking or arrowing to insert references), F4 (cycling `$` anchors),
//! function autocomplete and argument hints, the references to colour and outline, and column
//! AutoComplete.
//!
//! Functions work on the editor's text and caret. Carets are character (Unicode scalar) indices;
//! spans in the text are byte ranges.

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_formula::FormulaLocale;
use gridcraft_model::{Sheet, Workbook};
use serde::Serialize;

/// How many colours references cycle through (same reference text, same colour).
pub const REF_COLOR_COUNT: usize = 8;

/// The text after an editing operation, and where the caret and the pointed reference are.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Edit {
    pub text: String,
    /// Character index.
    pub caret: usize,
    /// Byte span of the reference point mode inserted last (arrows and clicks replace it).
    pub point: Option<(usize, usize)>,
}

/// Where editing the active cell starts: the cell (a merged area's first cell) and its text in
/// the interface's formula syntax. `None` for a picture cell, whose fallback value isn't editable
/// text (typing still replaces it).
pub fn start(wb: &Workbook, sheet: usize, cell: CellRef, locale: FormulaLocale) -> Option<(CellRef, String)> {
    let sh = wb.sheet(sheet)?;
    let at = sh.merge_at(cell).map(|m| m.start).unwrap_or(cell);
    if sh.cell_pictures.contains_key(&at) {
        return None;
    }
    let text = match sh.cell(at) {
        Some(c) if c.formula.is_some() => crate::formula_locale::display(&c.input_text(), locale, wb, sheet),
        _ => crate::display::edit_text(wb, sh, at),
    };
    Some((at, text))
}

/// The entry is a formula (`=…`, or `+`/`-` followed by something other than a number).
pub fn is_formula(text: &str) -> bool {
    text.starts_with('=')
        || (text.starts_with('+') && text.len() > 1)
        || (text.starts_with('-') && text.len() > 1 && !text.get(1..).is_some_and(|t| t.starts_with(|c: char| c.is_ascii_digit())))
}

fn byte_at(text: &str, caret: usize) -> usize {
    text.char_indices().nth(caret).map(|(i, _)| i).unwrap_or(text.len())
}

fn char_at(text: &str, byte: usize) -> usize {
    text.get(..byte).map_or(text.chars().count(), |s| s.chars().count())
}

/// Point mode: the caret follows an operator, `(`, `,` or `=` in a formula (or the reference
/// point mode just inserted), so clicking a cell or pressing an arrow inserts a reference.
pub fn can_point(text: &str, caret: usize, point: Option<(usize, usize)>) -> bool {
    if !is_formula(text) {
        return false;
    }
    let at = byte_at(text, caret);
    if point.is_some_and(|(_, e)| e == at) {
        return true;
    }
    let before = text.get(..at).unwrap_or("");
    matches!(before.trim_end().chars().last(), Some('=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | ';' | '{'))
}

/// Inserts reference `r` at the caret, or replaces the reference point mode inserted last.
pub fn insert_ref(text: &str, caret: usize, point: Option<(usize, usize)>, r: &str) -> Edit {
    let at = byte_at(text, caret);
    let (start, end) = match point {
        Some((s, e)) if e == at && s <= e => (s, e),
        _ => (at, at),
    };
    let (Some(a), Some(b)) = (text.get(..start), text.get(end..)) else {
        return Edit { text: text.to_string(), caret, point };
    };
    let text = format!("{a}{r}{b}");
    let end = start + r.len();
    Edit { caret: char_at(&text, end), text, point: Some((start, end)) }
}

/// The reference text point mode inserts for `range` on sheet `on` while editing a cell on
/// sheet `editing`: sheet-qualified (and quoted when needed) when they differ.
pub fn point_reference(wb: &Workbook, editing: usize, on: usize, range: RangeRef) -> String {
    let r = if range.is_single() { range.start.a1() } else { range.a1() };
    match wb.sheet(on).filter(|_| on != editing) {
        Some(sh) => format!("{}!{r}", gridcraft_formula::quote_sheet(&sh.name)),
        None => r,
    }
}

/// The function-name word ending at the caret.
fn word_before(text: &str, caret: usize) -> (usize, String) {
    let at = byte_at(text, caret);
    let before = text.get(..at).unwrap_or("");
    let len: usize = before.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_').map(char::len_utf8).sum();
    (at - len, before.get(at - len..).unwrap_or("").to_string())
}

/// Function names (from `names`, in the interface's syntax) the word before the caret starts,
/// at most 12; none outside a formula, after a number, or for an exact single match.
pub fn autocomplete(text: &str, caret: usize, names: &[String]) -> Vec<String> {
    if !is_formula(text) {
        return vec![];
    }
    let (_, word) = word_before(text, caret);
    if word.is_empty() || word.starts_with(|c: char| c.is_ascii_digit()) {
        return vec![];
    }
    let upper = word.to_ascii_uppercase();
    let found: Vec<String> = names.iter().filter(|n| n.starts_with(&upper)).take(12).cloned().collect();
    if found.len() == 1 && found.first() == Some(&upper) { vec![] } else { found }
}

/// Accepts autocomplete entry `name`: replaces the word before the caret and opens the call.
pub fn accept_autocomplete(text: &str, caret: usize, name: &str) -> Edit {
    let at = byte_at(text, caret);
    let (start, _) = word_before(text, caret);
    let (Some(a), Some(b)) = (text.get(..start), text.get(at..)) else {
        return Edit { text: text.to_string(), caret, point: None };
    };
    let text = format!("{a}{name}({b}");
    Edit { caret: char_at(&text, start + name.len() + 1), text, point: None }
}

/// The function call the caret is inside, and which argument (0-based), for the hint.
pub fn current_function(text: &str, caret: usize, locale: FormulaLocale) -> Option<(String, usize)> {
    if !is_formula(text) {
        return None;
    }
    let before = text.get(..byte_at(text, caret))?;
    let chars: Vec<char> = before.chars().collect();
    let (mut depth, mut arg, mut in_str) = (0i32, 0usize, false);
    for i in (0..chars.len()).rev() {
        let c = chars.get(i).copied().unwrap_or(' ');
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        match c {
            ')' | '}' | ']' => depth += 1,
            '{' | '[' => depth -= 1,
            '(' if depth == 0 => {
                let name: String = chars.get(..i)?.iter().rev().take_while(|c| c.is_ascii_alphanumeric() || **c == '.' || **c == '_').collect();
                let name: String = name.chars().rev().collect();
                return (!name.is_empty()).then(|| (name.to_ascii_uppercase(), arg));
            }
            '(' => depth -= 1,
            c if c == locale.list_separator() && depth == 0 => arg += 1,
            _ => {}
        }
    }
    None
}

/// A reference in formula text being edited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FormulaRef {
    /// Byte span in the text.
    pub start: usize,
    pub end: usize,
    pub range: RangeRef,
    /// The sheet it names, if any.
    pub sheet: Option<String>,
    /// Colour index, `0..REF_COLOR_COUNT`: the same reference text gets the same colour.
    pub color: usize,
}

/// The references in formula `text`, for colouring the editor and outlining them on the grid.
pub fn formula_refs(text: &str) -> Vec<FormulaRef> {
    use gridcraft_formula::lexer::Tok;
    let body = text.strip_prefix('=').or_else(|| text.strip_prefix('+')).unwrap_or(text);
    let offset = text.len() - body.len();
    let Ok(toks) = gridcraft_formula::lexer::tokenize(body) else { return vec![] };
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let tok = |i: usize| toks.get(i).map(|t| &t.tok);
    let mut i = 0;
    while let Some(t) = toks.get(i) {
        let (sheet, start, j) = match &t.tok {
            Tok::Sheet(a, _) => (Some(a.clone()), t.start, i + 1),
            _ => (None, t.start, i),
        };
        if let Some(Tok::Word(w)) = tok(j)
            && let Some(c) = CellRef::parse(w)
        {
            let mut end = toks.get(j).map_or(start, |t| t.end);
            let mut range = RangeRef::cell(c);
            if let (Some(Tok::Colon | Tok::Trim(_)), Some(Tok::Word(w2))) = (tok(j + 1), tok(j + 2))
                && let Some(c2) = CellRef::parse(w2)
            {
                range = RangeRef::new(c, c2);
                end = toks.get(j + 2).map_or(end, |t| t.end);
                i = j + 3;
            } else {
                i = j + 1;
            }
            let key = body.get(start..end).unwrap_or("").replace('$', "").to_ascii_uppercase();
            let color = match seen.iter().position(|k| *k == key) {
                Some(k) => k,
                None => {
                    seen.push(key);
                    seen.len() - 1
                }
            } % REF_COLOR_COUNT;
            out.push(FormulaRef { start: start + offset, end: end + offset, range, sheet, color });
            continue;
        }
        i += 1;
    }
    out
}

/// F4: cycles the reference at the caret through `$A$1`, `A$1`, `$A1` and `A1`.
pub fn cycle_anchor(text: &str, caret: usize) -> Option<Edit> {
    let at = byte_at(text, caret);
    let r = formula_refs(text).into_iter().find(|r| r.start <= at && at <= r.end)?;
    let original = text.get(r.start..r.end)?;
    let cycled = original
        .split(':')
        .map(|part| {
            let p = part.rsplit('!').next().unwrap_or(part);
            let prefix = part.get(..part.len() - p.len()).unwrap_or("");
            let bare = p.replace('$', "");
            let letters: String = bare.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            let digits = bare.get(letters.len()..).unwrap_or("");
            let col_fixed = p.starts_with('$');
            let row_fixed = p.trim_start_matches('$').contains('$');
            let next = match (col_fixed, row_fixed) {
                (false, false) => format!("${letters}${digits}"),
                (true, true) => format!("{letters}${digits}"),
                (false, true) => format!("${letters}{digits}"),
                (true, false) => format!("{letters}{digits}"),
            };
            format!("{prefix}{next}")
        })
        .collect::<Vec<_>>()
        .join(":");
    let text = format!("{}{cycled}{}", text.get(..r.start)?, text.get(r.end..)?);
    Some(Edit { caret: char_at(&text, r.start + cycled.len()), text, point: None })
}

/// Excel-style AutoComplete: the one distinct text entry in the same column (the contiguous
/// data above and below) that starts with `typed` (case-insensitive); `None` when ambiguous.
pub fn column_completion(sheet: &Sheet, at: CellRef, typed: &str) -> Option<String> {
    if typed.is_empty() || typed.starts_with('=') || typed.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
        return None;
    }
    let lower = typed.to_lowercase();
    let mut found: Option<String> = None;
    let mut ambiguous = false;
    let mut scan = |row: u32| -> bool {
        match sheet.value(CellRef::new(row, at.col)) {
            Value::Empty => false,
            Value::Text(t) => {
                let folded = t.to_lowercase();
                if folded.starts_with(&lower) && folded.len() > lower.len() {
                    match &found {
                        Some(f) if f.to_lowercase() != folded => ambiguous = true,
                        None => found = Some(t.to_string()),
                        _ => {}
                    }
                }
                true
            }
            _ => true,
        }
    };
    let (mut r, mut n) = (at.row, 0);
    while r > 0 && n < 5000 {
        r -= 1;
        n += 1;
        if !scan(r) {
            break;
        }
    }
    let (mut r, mut n) = (at.row.saturating_add(1), 0);
    while n < 5000 && r < gridcraft_core::MAX_ROWS && scan(r) {
        r += 1;
        n += 1;
    }
    if ambiguous { None } else { found }
}

/// The part of `full` after the `typed` prefix, counted in characters (case folding can change a
/// character's byte length: "ß" is two bytes, "ẞ" three).
pub fn completion_rest<'a>(full: &'a str, typed: &str) -> Option<&'a str> {
    let start = full.char_indices().nth(typed.chars().count())?.0;
    full.get(start..)
}

/// The worksheet functions' names as `locale` spells them, for autocomplete: the alias, unless
/// it would call a workbook or table name of the same spelling.
pub fn function_names(wb: &Workbook, sheet: usize, locale: FormulaLocale) -> Vec<String> {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES
        .get_or_init(|| crate::cmd::formulas::function_list().iter().filter_map(|f| f["name"].as_str().map(str::to_string)).collect())
        .iter()
        .map(|n| crate::formula_locale::completion_name(n, locale, wb, sheet).to_string())
        .collect()
}

/// A function's signature (`SUM(number1, [number2], ...)`) and description, as `locale` writes
/// them, for the argument hint.
pub fn function_help(name: &str, locale: FormulaLocale) -> Option<(String, String)> {
    static LIST: std::sync::OnceLock<Vec<serde_json::Value>> = std::sync::OnceLock::new();
    let canonical = locale.canonical_name(name);
    let info = LIST
        .get_or_init(crate::cmd::formulas::function_list)
        .iter()
        .find(|f| f["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(canonical)))?;
    let signature = info["signature"].as_str()?.replace(',', &locale.list_separator().to_string());
    let description = info["description"].as_str().unwrap_or("").to_string();
    Some((signature, description))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_mode_inserts_then_replaces_the_pointed_reference() {
        assert!(can_point("=SUM(", 5, None));
        let e = insert_ref("=SUM(", 5, None, "A1");
        assert_eq!(e.text, "=SUM(A1");
        let e = insert_ref(&e.text, e.caret, e.point, "A2");
        assert_eq!(e.text, "=SUM(A2");
        assert!(can_point("=SUM(A2:", 8, None));
        assert!(!can_point("hello", 5, None));
        assert!(!is_formula("-5") && is_formula("-A1") && is_formula("=1"));
    }

    #[test]
    fn autocomplete_and_argument_hints() {
        let names: Vec<String> = ["SUM", "SUMIF", "SUMPRODUCT", "AVERAGE"].iter().map(|s| s.to_string()).collect();
        assert_eq!(autocomplete("=su", 3, &names).len(), 3);
        assert_eq!(accept_autocomplete("=su", 3, "SUM").text, "=SUM(");
        assert_eq!(current_function("=IF(A1>0,SUM(B1,", 16, FormulaLocale::En), Some(("SUM".into(), 1)));
        assert_eq!(current_function("=SUMA(B1;", 9, FormulaLocale::Es), Some(("SUMA".into(), 1)));
        assert!(autocomplete("plain", 5, &names).is_empty());
    }

    #[test]
    fn references_are_found_and_coloured_and_f4_cycles_them() {
        let r = formula_refs("=A1+B2:C3*a1+Sheet2!D4");
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].color, r[2].color, "A1 and a1 share a colour");
        assert_eq!(r[1].range.a1(), "B2:C3");
        assert_eq!(r[3].sheet.as_deref(), Some("Sheet2"));
        let e = cycle_anchor("=A1+1", 2).unwrap();
        assert_eq!(e.text, "=$A$1+1");
        let e = cycle_anchor(&e.text, 2).unwrap();
        assert_eq!(e.text, "=A$1+1");
        assert_eq!(cycle_anchor(&cycle_anchor("=A$1", 2).unwrap().text, 2).unwrap().text, "=A1");
        assert_eq!(cycle_anchor("=1+2", 2), None);
    }

    #[test]
    fn editing_starts_from_the_text_and_column_autocomplete_finds_one_match() {
        let mut wb = Workbook::new();
        if let Some(sh) = wb.sheet_mut(0) {
            sh.set_value(CellRef::new(0, 0), Value::Text("Apples".into()));
            sh.set_value(CellRef::new(1, 0), Value::Text("Apricots".into()));
            sh.set_value(CellRef::new(2, 0), Value::Text("Bananas".into()));
        }
        let sh = wb.sheet(0).unwrap();
        assert_eq!(column_completion(sh, CellRef::new(3, 0), "ba").as_deref(), Some("Bananas"));
        assert_eq!(column_completion(sh, CellRef::new(3, 0), "ap"), None, "ambiguous");
        assert_eq!(completion_rest("Bananas", "ba"), Some("nanas"));
        assert_eq!(start(&wb, 0, CellRef::new(0, 0), FormulaLocale::En), Some((CellRef::new(0, 0), "Apples".into())));
        assert!(function_names(&wb, 0, FormulaLocale::En).iter().any(|n| n == "SUM"));
        assert!(function_help("SUM", FormulaLocale::En).is_some_and(|(s, _)| s.starts_with("SUM(")));
    }
}
