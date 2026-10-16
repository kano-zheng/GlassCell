//! UI formula syntax boundary. Engine commands, control/API callers and XLSX keep English.
use gridcraft_engine::formula::FormulaLocale;
use gridcraft_engine::model::Workbook;

pub(crate) fn display(text: &str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> String {
    gridcraft_engine::formula_locale::display(text, locale, wb, sheet)
}

// Descriptions/signatures for the supported Spanish function table; untranslated functions
// retain their English help. Parameter punctuation follows the same locale as entry.
const SPANISH_HELP: &[(&str, &str, &str)] = &[
    ("SUM", "SUMA(número1; [número2]; ...)", "Suma los números."),
    ("AVERAGE", "PROMEDIO(número1; [número2]; ...)", "Devuelve el promedio de los números."),
    ("IF", "SI(prueba_lógica; valor_si_verdadero; [valor_si_falso])", "Devuelve un valor según una condición."),
    ("COUNT", "CONTAR(valor1; [valor2]; ...)", "Cuenta las celdas que contienen números."),
    ("SUMIF", "SUMAR.SI(rango; criterio; [rango_suma])", "Suma las celdas que cumplen un criterio."),
    ("COUNTIF", "CONTAR.SI(rango; criterio)", "Cuenta las celdas que cumplen un criterio."),
    ("VLOOKUP", "BUSCARV(valor_buscado; matriz; indicador_columnas; [ordenado])", "Busca un valor en la primera columna de una tabla."),
    ("TODAY", "HOY()", "Devuelve la fecha actual."),
    ("TRUE", "VERDADERO()", "Devuelve el valor lógico VERDADERO."),
    ("FALSE", "FALSO()", "Devuelve el valor lógico FALSO."),
];

pub(crate) fn signature(name: &str, locale: FormulaLocale) -> Option<String> {
    let canonical = locale.canonical_name(name);
    if locale == FormulaLocale::Es
        && let Some((_, signature, _)) = SPANISH_HELP.iter().find(|(name, ..)| *name == canonical)
    {
        return Some((*signature).into());
    }
    crate::formula_bar::function_signature(canonical).map(|s| s.replace(',', &locale.list_separator().to_string()))
}

pub(crate) fn description(name: &str, locale: FormulaLocale) -> Option<String> {
    let canonical = locale.canonical_name(name);
    if locale == FormulaLocale::Es
        && let Some((_, _, description)) = SPANISH_HELP.iter().find(|(name, ..)| *name == canonical)
    {
        return Some((*description).into());
    }
    crate::formula_bar::function_description(canonical)
}

pub(crate) fn completion_name<'a>(name: &'a str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> &'a str {
    gridcraft_engine::formula_locale::completion_name(name, locale, wb, sheet)
}

pub(crate) fn display_template(text: &str, locale: FormulaLocale, wb: &Workbook, sheet: usize) -> String {
    gridcraft_engine::formula_locale::display_template(text, locale, wb, sheet)
}
