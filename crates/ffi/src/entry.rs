//! The cell entry rules (`gridcraft_engine::entry`) for Swift's editors. Swift counts text in
//! UTF-16 code units (`NSString`, `NSRange`); the engine counts characters and bytes, so every
//! caret and span is converted here.

use gridcraft_engine::entry;
use gridcraft_engine::formula_locale::locale_param;

use crate::CellRange;

/// A span of text in UTF-16 code units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
}

/// Editor text after an operation, with the caret and the reference point mode inserted last.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EntryEdit {
    pub text: String,
    pub caret: u32,
    pub point: Option<TextSpan>,
}

/// A reference in formula text: where it is, what it refers to, and its colour index.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FormulaReference {
    pub span: TextSpan,
    pub range: CellRange,
    pub sheet: Option<String>,
    /// `0..reference_color_count()`.
    pub color: u32,
}

/// The function call the caret is in, for the argument hint.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FunctionHint {
    pub name: String,
    /// The argument the caret is in (0-based).
    pub argument: u32,
    pub signature: Option<String>,
    pub description: Option<String>,
}

fn utf16_to_byte(text: &str, units: u32) -> usize {
    let mut count = 0u32;
    for (i, c) in text.char_indices() {
        if count >= units {
            return i;
        }
        count += c.len_utf16() as u32;
    }
    text.len()
}

fn utf16_to_char(text: &str, units: u32) -> usize {
    let mut count = 0u32;
    for (n, c) in text.chars().enumerate() {
        if count >= units {
            return n;
        }
        count += c.len_utf16() as u32;
    }
    text.chars().count()
}

fn byte_to_utf16(text: &str, byte: usize) -> u32 {
    text.get(..byte.min(text.len())).map_or(0, |s| s.encode_utf16().count() as u32)
}

fn char_to_utf16(text: &str, n: usize) -> u32 {
    text.chars().take(n).map(|c| c.len_utf16() as u32).sum()
}

fn span_to_bytes(text: &str, s: Option<TextSpan>) -> Option<(usize, usize)> {
    s.map(|s| (utf16_to_byte(text, s.start), utf16_to_byte(text, s.end)))
}

fn edit(e: entry::Edit) -> EntryEdit {
    let point = e.point.map(|(a, b)| TextSpan { start: byte_to_utf16(&e.text, a), end: byte_to_utf16(&e.text, b) });
    EntryEdit { caret: char_to_utf16(&e.text, e.caret), point, text: e.text }
}

/// How many colours formula references cycle through.
#[uniffi::export]
pub fn reference_color_count() -> u32 {
    entry::REF_COLOR_COUNT as u32
}

/// The entry is a formula.
#[uniffi::export]
pub fn entry_is_formula(text: String) -> bool {
    entry::is_formula(&text)
}

/// Point mode: clicking a cell or an arrow key inserts a reference at the caret.
#[uniffi::export]
pub fn entry_can_point(text: String, caret: u32, point: Option<TextSpan>) -> bool {
    entry::can_point(&text, utf16_to_char(&text, caret), span_to_bytes(&text, point))
}

/// Inserts `reference` at the caret (or replaces the reference point mode inserted last).
#[uniffi::export]
pub fn entry_insert_reference(text: String, caret: u32, point: Option<TextSpan>, reference: String) -> EntryEdit {
    edit(entry::insert_ref(&text, utf16_to_char(&text, caret), span_to_bytes(&text, point), &reference))
}

/// Function names starting with the word before the caret (`names`: `Engine::function_names`).
#[uniffi::export]
pub fn entry_autocomplete(text: String, caret: u32, names: Vec<String>) -> Vec<String> {
    entry::autocomplete(&text, utf16_to_char(&text, caret), &names)
}

/// Accepts autocomplete entry `name`: replaces the word before the caret and opens the call.
#[uniffi::export]
pub fn entry_accept_autocomplete(text: String, caret: u32, name: String) -> EntryEdit {
    edit(entry::accept_autocomplete(&text, utf16_to_char(&text, caret), &name))
}

/// F4: cycles the `$` anchors of the reference at the caret.
#[uniffi::export]
pub fn entry_cycle_anchor(text: String, caret: u32) -> Option<EntryEdit> {
    entry::cycle_anchor(&text, utf16_to_char(&text, caret)).map(edit)
}

/// The function call the caret is inside, with its signature and description in `locale`.
#[uniffi::export]
pub fn entry_function_hint(text: String, caret: u32, locale: String) -> Option<FunctionHint> {
    let locale = locale_param(Some(&locale));
    let (name, argument) = entry::current_function(&text, utf16_to_char(&text, caret), locale)?;
    let help = entry::function_help(&name, locale);
    Some(FunctionHint {
        argument: u32::try_from(argument).unwrap_or(u32::MAX),
        signature: help.as_ref().map(|h| h.0.clone()),
        description: help.map(|h| h.1).filter(|d| !d.is_empty()),
        name,
    })
}

/// The references in formula `text`, to colour in the editor and outline on the grid.
#[uniffi::export]
pub fn entry_formula_references(text: String) -> Vec<FormulaReference> {
    entry::formula_refs(&text)
        .into_iter()
        .map(|r| FormulaReference {
            span: TextSpan { start: byte_to_utf16(&text, r.start), end: byte_to_utf16(&text, r.end) },
            range: r.range.into(),
            sheet: r.sheet,
            color: r.color as u32,
        })
        .collect()
}

/// The rest of AutoComplete's suggestion `full` after the typed `typed`.
#[uniffi::export]
pub fn entry_completion_rest(full: String, typed: String) -> Option<String> {
    entry::completion_rest(&full, &typed).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carets_and_spans_are_utf16() {
        // "😀" is one character, two UTF-16 units, four bytes.
        let text = "=\"😀\"&".to_string();
        let caret = text.encode_utf16().count() as u32;
        assert!(entry_can_point(text.clone(), caret, None));
        let e = entry_insert_reference(text.clone(), caret, None, "B2".into());
        assert_eq!(e.text, "=\"😀\"&B2");
        assert_eq!(e.caret, caret + 2);
        assert_eq!(e.point, Some(TextSpan { start: caret, end: caret + 2 }));
        let refs = entry_formula_references(e.text.clone());
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].span, TextSpan { start: caret, end: caret + 2 });
        // A caret past the end, or in the middle of a surrogate pair, stays in the text.
        assert!(!entry_insert_reference(text.clone(), 999, None, "A1".into()).text.is_empty());
        assert!(!entry_insert_reference(text, 3, None, "A1".into()).text.is_empty());
    }

    #[test]
    fn hints_carry_the_signature() {
        let hint = entry_function_hint("=SUM(1,".into(), 7, "en".into()).unwrap();
        assert_eq!((hint.name.as_str(), hint.argument), ("SUM", 1));
        assert!(hint.signature.is_some_and(|s| s.starts_with("SUM(")));
        assert_eq!(entry_cycle_anchor("=A1".into(), 3).map(|e| e.text), Some("=$A$1".into()));
        assert_eq!(entry_completion_rest("Bananas".into(), "ba".into()).as_deref(), Some("nanas"));
    }
}
