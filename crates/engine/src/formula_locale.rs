//! The interface's formula syntax boundary: what the user types and sees in the interface
//! language's formula syntax (`=SUMA(A1;B1)` in Spanish) against the canonical English syntax
//! that commands, the control channel, MCP and XLSX use.

use gridcraft_core::CellRef;
use gridcraft_formula::{self as formula, FormulaLocale, ParseError};
use gridcraft_model::Workbook;

const UDF_PREFIX: &str = "_xludf.";

/// The formula locale a command parameter names: `"es"` is Spanish, anything else English.
pub fn locale_param(name: Option<&str>) -> FormulaLocale {
    match name.map(str::to_ascii_lowercase).as_deref() {
        Some("es") => FormulaLocale::Es,
        _ => FormulaLocale::En,
    }
}

/// The name [`locale_param`] reads for `locale`.
pub fn locale_id(locale: FormulaLocale) -> &'static str {
    match locale {
        FormulaLocale::Es => "es",
        FormulaLocale::En => "en",
    }
}

/// Entry `text` typed in `locale` for `cell`, as canonical input for `cell.set`: formulas are
/// reparsed into English syntax, decimal-comma numbers get a decimal point; other text is kept.
pub fn canonical(text: &str, locale: FormulaLocale, wb: &Workbook, sheet: usize, cell: CellRef) -> Result<String, ParseError> {
    // The editor hid the `_xludf.` prefixes of this cell's formula; confirming it unchanged keeps
    // the user-defined calls instead of binding them to the built-ins of the same name.
    if let Some(stored) = wb.sheet(sheet).and_then(|sh| sh.cell(cell)).filter(|c| c.formula.is_some()).map(|c| c.input_text())
        && has_udf_prefix(&stored)
        && text == display(&stored, locale, wb, sheet)
    {
        return Ok(stored);
    }
    if locale == FormulaLocale::En || wb.sheet(sheet).is_some_and(|sh| wb.styles.get(sh.style_id(cell)).num_fmt.as_str() == "@") {
        return Ok(text.to_string());
    }
    // Same formula detection as the engine's `input_to_cell` (no trimming: " =1" is text).
    let body = text;
    if body.starts_with('=')
        || ((body.starts_with('+') || body.starts_with('-')) && body.chars().nth(1).is_some_and(|c| c.is_ascii_alphabetic() || c == '('))
    {
        let known = |name: &str| wb.name(name, sheet).is_some() || wb.table(name).is_some();
        // Match the engine's existing convenience for missing closing parentheses.
        let mut body = body.to_string();
        let mut expr = formula::parse_input(&body, locale, known);
        for _ in 0..8 {
            if expr.is_ok() {
                break;
            }
            body.push(')');
            expr = formula::parse_input(&body, locale, known);
        }
        return Ok(format!("={}", formula::print(&expr?)));
    }
    // Decimal-comma number entry follows the same preference, without rewriting text or dates.
    let normalized = body.trim().replace(locale.decimal_separator(), ".");
    let number = normalized.strip_suffix('%').unwrap_or(&normalized);
    if number.parse::<f64>().is_ok_and(f64::is_finite) {
        return Ok(normalized);
    }
    Ok(text.to_string())
}

/// A stored (English) formula as the interface shows it in `locale` (the editor, the formula
/// bar, Show Formulas). `_xludf.` prefixes of user-defined calls are hidden.
pub fn display(text: &str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> String {
    // English (and every language without a formula table) shows the stored text, `_xludf.`
    // prefixes aside, and skips a parse per frame.
    if locale == FormulaLocale::En {
        return hide_udf_prefix(text, locale);
    }
    let Some(body) = text.strip_prefix('=') else { return text.to_string() };
    let shown = formula::parse(body)
        .and_then(|expr| formula::print_input(&expr, locale, |name| wb.name(name, sheet).is_some() || wb.table(name).is_some()))
        .map(|body| format!("={body}"))
        .unwrap_or_else(|_| text.to_string());
    hide_udf_prefix(&shown, locale)
}

/// A function name to suggest in `locale`: its alias, unless the alias would call a workbook or
/// table name instead of the function.
pub fn completion_name<'a>(name: &'a str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> &'a str {
    let alias = locale.function_name(name);
    if wb.name(alias, sheet).is_some() || wb.table(alias).is_some() { name } else { alias }
}

/// Engine-generated edit templates (`UiRequest::EditCell`) use canonical syntax. The only
/// incomplete template is the bare function opening emitted by Insert Function; typed user input
/// never enters here.
pub fn display_template(text: &str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> String {
    if let Some(name) = text.strip_prefix('=').and_then(|s| s.strip_suffix('('))
        && !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
    {
        return format!("={}(", completion_name(name, locale, wb, sheet));
    }
    display(text, locale, wb, sheet)
}

fn has_udf_prefix(text: &str) -> bool {
    text.to_ascii_lowercase().contains(UDF_PREFIX)
}

/// Excel shows a user-defined call such as `_xludf.EOMONTH()` as `=EOMONTH()`. Only function
/// names lose the prefix: text literals and names keep it.
fn hide_udf_prefix(text: &str, locale: FormulaLocale) -> String {
    let Some(body) = text.strip_prefix('=').filter(|b| has_udf_prefix(b)) else { return text.to_string() };
    let Ok(tokens) = formula::lexer::tokenize_locale(body, locale) else { return text.to_string() };
    let mut out = String::from("=");
    let mut from = 0;
    for t in tokens {
        if matches!(t.tok, formula::lexer::Tok::Func(_))
            && body.get(t.start..t.start + UDF_PREFIX.len()).is_some_and(|p| p.eq_ignore_ascii_case(UDF_PREFIX))
        {
            out.push_str(body.get(from..t.start).unwrap_or(""));
            from = t.start + UDF_PREFIX.len();
        }
    }
    out.push_str(body.get(from..).unwrap_or(""));
    out
}
