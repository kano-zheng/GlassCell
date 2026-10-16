//! The engine's dialogs that are forms: their titles, fields, starting values, the command OK
//! runs, and how the values become that command's parameters. Every frontend draws them from
//! here (egui's dialogs, the Mac's sheets), so a dialog behaves the same everywhere.
//!
//! A command asks for one with `UiRequest::Dialog(name, params)`; [`open`] turns that into a
//! [`Form`], or `None` for the dialogs that aren't plain forms (Find, Sort, Name Manager…).

use serde::Serialize;
use serde_json::{Map, Value as Json, json};

use crate::Session;

/// One form control. Values live in [`Form::values`] under `key`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Field {
    Text {
        key: &'static str,
        label: &'static str,
    },
    Password {
        key: &'static str,
        label: &'static str,
    },
    Number {
        key: &'static str,
        label: &'static str,
    },
    /// One of `options` (value, label).
    Choice {
        key: &'static str,
        label: &'static str,
        options: Vec<(&'static str, &'static str)>,
    },
    Check {
        key: &'static str,
        label: &'static str,
    },
    /// Text to read.
    Note(String),
}

/// A dialog that is a form.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Form {
    pub name: String,
    pub title: String,
    /// What OK runs, with [`build_params`] of the values.
    pub command: &'static str,
    pub fields: Vec<Field>,
    pub values: Map<String, Json>,
}

fn form(name: &str, title: &str, command: &'static str, fields: Vec<Field>, defaults: Json) -> Form {
    Form { name: name.into(), title: title.into(), command, fields, values: defaults.as_object().cloned().unwrap_or_default() }
}

/// The form for dialog `name` asked for with `params`, starting from the workbook's state (the
/// row height, the rule on the active cell…); `None` when `name` isn't a form.
pub fn open(s: &mut Session, name: &str, params: &Json) -> Option<Form> {
    let sel = s.active().map(|d| d.selection.current().a1()).unwrap_or_default();
    let active = s.active().map(|d| d.selection.active.a1()).unwrap_or_default();
    let p = |k: &str| params.get(k).cloned().unwrap_or(Json::Null);
    use Field::*;
    let d = match name {
        "insertCells" => form(
            "insertCells",
            "Insert",
            "home.insertCells",
            vec![Choice {
                key: "shift",
                label: "Insert",
                options: vec![("right", "Shift cells right"), ("down", "Shift cells down"), ("row", "Entire row"), ("column", "Entire column")],
            }],
            json!({"shift": "down"}),
        ),
        "insertPaste" => form(
            "insertPaste",
            "Insert Paste",
            "home.insertCopiedCells",
            vec![Choice { key: "shift", label: "Insert", options: vec![("right", "Shift cells right"), ("down", "Shift cells down")] }],
            json!({"shift": "down"}),
        ),
        "deleteCells" => form(
            "deleteCells",
            "Delete",
            "home.deleteCells",
            vec![Choice {
                key: "shift",
                label: "Delete",
                options: vec![("left", "Shift cells left"), ("up", "Shift cells up"), ("row", "Entire row"), ("column", "Entire column")],
            }],
            json!({"shift": "up"}),
        ),
        "rowHeight" => form(
            "rowHeight",
            "Row Height",
            "home.rowHeight",
            vec![Number { key: "height", label: "Row height (pixels):" }],
            json!({"height": s.active().and_then(|d| d.wb.active().map(|s| s.row_height(d.selection.active.row))).unwrap_or(20.0)}),
        ),
        "columnWidth" => form(
            "columnWidth",
            "Column Width",
            "home.columnWidth",
            vec![Number { key: "chars", label: "Column width (characters):" }],
            json!({"chars": s.active().and_then(|d| d.wb.active().map(|s| gridcraft_model::points_to_chars(s.col_width(d.selection.active.col) as f64))).unwrap_or(8.43)}),
        ),
        "defaultWidth" => form(
            "defaultWidth",
            "Standard Width",
            "home.defaultWidth",
            vec![Number { key: "width", label: "Standard column width (pixels):" }],
            json!({"width": 64}),
        ),
        "renameSheet" => form(
            "renameSheet",
            "Rename Sheet",
            "sheet.rename",
            vec![Text { key: "name", label: "Name:" }],
            json!({"name": s.active().and_then(|d| d.wb.active().map(|s| s.name.clone()))}),
        ),
        "moveSheet" => {
            let sheets: Vec<String> = s.active().map(|d| d.wb.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
            let mut d = form(
                "moveSheet",
                "Move or Copy",
                "sheet.move",
                vec![
                    Number { key: "to", label: "Before sheet (position, 0 = first):" },
                    Check { key: "copy", label: "Create a copy" },
                    Note(format!("Sheets: {}", sheets.join(", "))),
                ],
                json!({"to": 0, "copy": false}),
            );
            if let Some(s) = params.get("sheet") {
                d.values.insert("sheet".into(), s.clone());
            }
            d
        }
        "protectSheet" => form(
            "protectSheet",
            "Protect Sheet",
            "review.protectSheet",
            vec![
                Password { key: "password", label: "Password (optional):" },
                Note("Allow all users of this sheet to:".into()),
                Check { key: "formatCells", label: "Format cells" },
                Check { key: "formatColumns", label: "Format columns" },
                Check { key: "formatRows", label: "Format rows" },
                Check { key: "insertRows", label: "Insert rows" },
                Check { key: "deleteRows", label: "Delete rows" },
                Check { key: "sort", label: "Sort" },
                Check { key: "autofilter", label: "Use AutoFilter" },
            ],
            json!({}),
        ),
        "allowEditRanges" => {
            let list = s.run("review.allowEditRanges", json!({})).unwrap_or(Json::Null);
            let rows: Vec<String> = list
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|r| {
                            let lock = match (r["password"].as_bool(), r["unlocked"].as_bool()) {
                                (Some(true), Some(true)) => " (password, unlocked)",
                                (Some(true), _) => " (password)",
                                _ => "",
                            };
                            format!("{}: {}{lock}", r["title"].as_str().unwrap_or(""), r["range"].as_str().unwrap_or(""))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let summary = if rows.is_empty() { "No ranges yet.".to_string() } else { rows.join("\n") };
            form(
                "allowEditRanges",
                "Allow Edit Ranges",
                "review.allowEditRanges",
                vec![
                    Note(format!("Ranges that stay editable when the sheet is protected:\n{summary}")),
                    Choice {
                        key: "action",
                        label: "Action",
                        options: vec![("add", "New"), ("modify", "Modify"), ("delete", "Delete"), ("unlock", "Unlock with password")],
                    },
                    Text { key: "title", label: "Title:" },
                    Text { key: "range", label: "Refers to cells:" },
                    Text { key: "password", label: "Range password:" },
                    Text { key: "newTitle", label: "New title (Modify):" },
                    Check { key: "removePassword", label: "Remove the password (Modify)" },
                ],
                json!({"action": "add", "range": sel}),
            )
        }
        "protectWorkbook" => form(
            "protectWorkbook",
            "Protect Structure and Windows",
            "review.protectWorkbook",
            vec![
                Password { key: "password", label: "Password (optional):" },
                Note("Protect workbook for: Structure (sheets can't be added, moved, renamed, hidden or deleted).".into()),
            ],
            json!({"on": true}),
        ),
        "unprotectWorkbook" => form(
            "unprotectWorkbook",
            "Unprotect Workbook",
            "review.protectWorkbook",
            vec![Password { key: "password", label: "Password:" }],
            json!({"on": false}),
        ),
        "openPassword" => {
            // The file rides along in the values (`path` on the desktop, `name` and `base64`
            // on the web) and goes back to `file.open` with the password.
            let source = p("path").as_str().or(p("name").as_str()).map(str::to_string).unwrap_or_default();
            let file = std::path::Path::new(&source).file_name().and_then(|n| n.to_str()).unwrap_or("This workbook").to_string();
            // An empty password from the start, so OK on the untouched field says "incorrect"
            // rather than asking for the password again.
            let mut values = params.clone();
            values["password"] = json!("");
            form(
                "openPassword",
                "Password",
                "file.open",
                vec![Note(format!("'{file}' is protected.")), Password { key: "password", label: "Password:" }],
                values,
            )
        }
        "unprotectSheet" => {
            form("unprotectSheet", "Unprotect Sheet", "review.unprotectSheet", vec![Password { key: "password", label: "Password:" }], json!({}))
        }
        "unhideSheet" => {
            let opts: Vec<String> =
                p("sheets").as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let first = opts.first().cloned().unwrap_or_default();
            let mut d = form(
                "unhideSheet",
                "Unhide",
                "sheet.unhide",
                vec![Text { key: "sheet", label: "Unhide sheet:" }, Note(format!("Hidden: {}", opts.join(", ")))],
                json!({"sheet": first}),
            );
            d.values.insert("sheet".into(), json!(first));
            d
        }
        "pasteSpecial" => form(
            "pasteSpecial",
            "Paste Special",
            "edit.pasteSpecial",
            vec![
                Choice {
                    key: "what",
                    label: "Paste",
                    options: vec![
                        ("all", "All"),
                        ("formulas", "Formulas (F)"),
                        ("values", "Values (V)"),
                        ("formats", "Formats (T)"),
                        ("comments", "Comments and notes (C)"),
                        ("validation", "Validation"),
                        ("allExceptBorders", "All except borders"),
                        ("columnWidths", "Column widths"),
                        ("formulasAndNumberFormats", "Formulas and number formats"),
                        ("valuesAndNumberFormats", "Values and number formats"),
                    ],
                },
                Choice {
                    key: "operation",
                    label: "Operation",
                    options: vec![("none", "None"), ("add", "Add"), ("subtract", "Subtract"), ("multiply", "Multiply"), ("divide", "Divide")],
                },
                Check { key: "skipBlanks", label: "Skip blanks" },
                Check { key: "transpose", label: "Transpose (E)" },
                Check { key: "link", label: "Paste Link" },
            ],
            json!({"what": "all", "operation": "none"}),
        ),
        "series" => form(
            "series",
            "Series",
            "edit.fillSeries",
            vec![
                Choice { key: "direction", label: "Series in", options: vec![("columns", "Columns"), ("rows", "Rows")] },
                Choice { key: "type", label: "Type", options: vec![("linear", "Linear"), ("growth", "Growth"), ("date", "Date")] },
                Choice {
                    key: "dateUnit",
                    label: "Date unit",
                    options: vec![("day", "Day"), ("weekday", "Weekday"), ("month", "Month"), ("year", "Year")],
                },
                Number { key: "step", label: "Step value:" },
                Text { key: "stop", label: "Stop value:" },
            ],
            json!({"direction": "columns", "type": "linear", "dateUnit": "day", "step": 1}),
        ),
        "comment" => {
            let threaded = p("threaded").as_bool().unwrap_or(true);
            let existing =
                s.active().and_then(|d| d.wb.active().and_then(|s| s.comments.get(&d.selection.active).map(|c| c.text.clone()))).unwrap_or_default();
            form(
                if threaded { "comment" } else { "note" },
                if threaded { "New Comment" } else { "New Note" },
                if threaded { "review.newComment" } else { "review.newNote" },
                vec![Text { key: "text", label: if threaded { "Start a conversation:" } else { "Note:" } }],
                json!({"text": existing}),
            )
        }
        // Asks before a web or e-mail link leaves the workbook; OK runs `link.open`, which
        // refuses anything but http, https and mailto.
        "openLink" => {
            let url = params.get("url").and_then(Json::as_str).unwrap_or_default().to_string();
            form(
                "openLink",
                "Open Hyperlink",
                "link.open",
                vec![Note(format!(
                    "This workbook asks to open the address below. Links can lead to harmful sites or start other programs: open it only if you trust the workbook.\n\n{url}"
                ))],
                json!({"url": url}),
            )
        }
        "insertLink" => form(
            "insertLink",
            "Insert Hyperlink",
            "insert.link",
            vec![
                Text { key: "text", label: "Display:" },
                Text { key: "target", label: "Link to (web address, e-mail or Sheet!A1):" },
                Text { key: "tooltip", label: "ScreenTip:" },
            ],
            json!({"target": "https://"}),
        ),
        "sparkline" => form(
            "sparkline",
            "Create Sparklines",
            "insert.sparkline",
            vec![
                Text { key: "range", label: "Data Range:" },
                Text { key: "location", label: "Location Range:" },
                Check { key: "markers", label: "Markers" },
            ],
            json!({"range": sel, "location": active, "type": p("type")}),
        ),
        "headerFooter" => form(
            "headerFooter",
            "Header & Footer",
            "pageLayout.headerFooter",
            vec![
                Text { key: "header", label: "Header (&L left, &C center, &R right; &P page, &N pages, &D date, &A sheet):" },
                Text { key: "footer", label: "Footer:" },
            ],
            json!({"header": "", "footer": "&CPage &P of &N"}),
        ),
        "pageSetup" => {
            // Opens on the active sheet's saved margins, so OK without edits rewrites them unchanged.
            let m = s.active().and_then(|d| d.wb.active()).map(|s| s.print.margins).unwrap_or([0.7, 0.7, 0.75, 0.75, 0.3, 0.3]);
            // Round off f32 noise so the form shows 0.1, not 0.10000000149.
            let [left, right, top, bottom, header, footer] = m.map(|v| (f64::from(v) * 1e4).round() / 1e4);
            form(
                "pageSetup",
                "Page Setup",
                "pageLayout.margins",
                vec![
                    Number { key: "left", label: "Left margin (in):" },
                    Number { key: "right", label: "Right margin (in):" },
                    Number { key: "top", label: "Top margin (in):" },
                    Number { key: "bottom", label: "Bottom margin (in):" },
                    Number { key: "header", label: "Header (in):" },
                    Number { key: "footer", label: "Footer (in):" },
                ],
                json!({"left": left, "right": right, "top": top, "bottom": bottom, "header": header, "footer": footer}),
            )
        }
        "textToColumns" => form(
            "textToColumns",
            "Convert Text to Columns",
            "data.textToColumns",
            vec![
                Check { key: "tab", label: "Tab" },
                Check { key: "semicolon", label: "Semicolon" },
                Check { key: "comma", label: "Comma" },
                Check { key: "space", label: "Space" },
                Text { key: "other", label: "Other:" },
                Check { key: "treatConsecutive", label: "Treat consecutive delimiters as one" },
                Text { key: "destination", label: "Destination:" },
            ],
            json!({"tab": true, "comma": true, "destination": active}),
        ),
        "removeDuplicates" => form(
            "removeDuplicates",
            "Remove Duplicates",
            "data.removeDuplicates",
            vec![Check { key: "header", label: "My data has headers" }, Text { key: "columnsText", label: "Columns (blank = all, e.g. A,C):" }],
            json!({"header": true}),
        ),
        "forecastSheet" => form(
            "forecastSheet",
            "Create Forecast Worksheet",
            "data.forecastSheet",
            vec![
                Text { key: "timeline", label: "Timeline range (blank: first column of the selection):" },
                Text { key: "values", label: "Values range (blank: second column of the selection):" },
                Text { key: "end", label: "Forecast end (date or number; blank: a third of the history):" },
                Text { key: "forecastStart", label: "Forecast start (a date or number in the history; blank: after the last point):" },
                Check { key: "confidenceInterval", label: "Confidence interval" },
                Text { key: "confidence", label: "Confidence level (%):" },
                Text { key: "seasonality", label: "Seasonality (auto, 0 for none, or a season length):" },
                Choice {
                    key: "fillMissing",
                    label: "Fill missing points using",
                    options: vec![("interpolate", "Interpolation"), ("zeros", "Zeros")],
                },
                Choice {
                    key: "aggregate",
                    label: "Aggregate duplicates using",
                    options: vec![
                        ("average", "Average"),
                        ("count", "Count"),
                        ("counta", "CountA"),
                        ("max", "Max"),
                        ("median", "Median"),
                        ("min", "Min"),
                        ("sum", "Sum"),
                    ],
                },
                Check { key: "statistics", label: "Include forecast statistics" },
                Choice { key: "chart", label: "Chart", options: vec![("line", "Line chart"), ("column", "Column chart")] },
            ],
            json!({"confidenceInterval": true, "confidence": "95", "seasonality": "auto", "fillMissing": "interpolate", "aggregate": "average", "chart": "line"}),
        ),
        "subtotal" => form(
            "subtotal",
            "Subtotal",
            "data.subtotal",
            vec![
                Text { key: "groupBy", label: "At each change in (column letter):" },
                Choice {
                    key: "function",
                    label: "Use function",
                    options: vec![("sum", "Sum"), ("count", "Count"), ("average", "Average"), ("max", "Max"), ("min", "Min"), ("product", "Product")],
                },
                Text { key: "columnsText", label: "Add subtotal to (columns, e.g. C,D):" },
            ],
            json!({"groupBy": "A", "function": "sum"}),
        ),
        "zoom" => form(
            "zoom",
            "Zoom",
            "view.zoom",
            vec![
                Choice {
                    key: "preset",
                    label: "Magnification",
                    options: vec![("200", "200%"), ("100", "100%"), ("75", "75%"), ("50", "50%"), ("25", "25%")],
                },
                Number { key: "percent", label: "Custom (%):" },
            ],
            json!({"percent": s.active().and_then(|d| d.wb.active().map(|s| s.zoom)).unwrap_or(100)}),
        ),
        "defineName" => {
            // Name Manager › Edit… passes the name being edited (`oldName`, `oldScope`) and
            // its current fields; New… and Define Name start from the selection.
            let editing = params.get("oldName").and_then(Json::as_str).is_some();
            let selection = s
                .active()
                .and_then(|d| d.wb.active().map(|s| format!("={}!{}", gridcraft_formula::quote_sheet(&s.name), abs(&d.selection.current()))));
            let mut defaults = json!({"scope": "Workbook", "refersTo": selection});
            for key in ["name", "scope", "comment", "refersTo", "oldName", "oldScope"] {
                if let Some(v) = params.get(key).filter(|v| v.is_string()) {
                    defaults[key] = v.clone();
                }
            }
            form(
                "defineName",
                if editing { "Edit Name" } else { "New Name" },
                "formulas.defineName",
                vec![
                    Text { key: "name", label: "Name:" },
                    Text { key: "scope", label: "Scope (Workbook or sheet name):" },
                    Text { key: "comment", label: "Comment:" },
                    Text { key: "refersTo", label: "Refers to:" },
                ],
                defaults,
            )
        }
        "dataValidation" => {
            form("dataValidation", "Data Validation", "data.validation", data_validation_fields("any", "between"), validation_defaults(s))
        }
        "cfQuick" => {
            let ty = p("type").as_str().unwrap_or("cellIs").to_string();
            let title = p("title").as_str().unwrap_or("Conditional Formatting").to_string();
            let mut fields = vec![];
            match ty.as_str() {
                "cellIs" => {
                    fields.push(Text { key: "value", label: "Format cells that are (value):" });
                    if p("operator").as_str() == Some("between") {
                        fields.push(Text { key: "value2", label: "and:" });
                    }
                }
                "containsText" => fields.push(Text { key: "text", label: "Format cells that contain the text:" }),
                "timePeriod" => fields.push(Choice {
                    key: "period",
                    label: "Format cells that contain a date occurring:",
                    options: vec![
                        ("yesterday", "Yesterday"),
                        ("today", "Today"),
                        ("tomorrow", "Tomorrow"),
                        ("last7Days", "In the last 7 days"),
                        ("lastWeek", "Last week"),
                        ("thisWeek", "This week"),
                        ("nextWeek", "Next week"),
                        ("lastMonth", "Last month"),
                        ("thisMonth", "This month"),
                        ("nextMonth", "Next month"),
                    ],
                }),
                _ => fields.push(Text { key: "formula", label: "Format values where this formula is true:" }),
            }
            fields.push(Choice {
                key: "preset",
                label: "with",
                options: vec![
                    ("lightRedFillDarkRedText", "Light Red Fill with Dark Red Text"),
                    ("yellowFillDarkYellowText", "Yellow Fill with Dark Yellow Text"),
                    ("greenFillDarkGreenText", "Green Fill with Dark Green Text"),
                    ("lightRedFill", "Light Red Fill"),
                    ("redText", "Red Text"),
                    ("redBorder", "Red Border"),
                ],
            });
            let mut d = form("cfQuick", &title, "home.conditionalFormat", fields, json!({"preset": "lightRedFillDarkRedText", "period": "today"}));
            d.values.insert("type".into(), json!(ty));
            d.values.insert("operator".into(), p("operator"));
            d
        }
        "pivotCalculatedField" => form(
            "pivotCalculatedField",
            "Insert Calculated Field",
            "pivot.calculatedField",
            vec![Text { key: "name", label: "Name:" }, Text { key: "formula", label: "Formula:" }],
            json!({"pivot": p("pivot"), "name": p("name"), "formula": p("formula")}),
        ),
        "goToSpecial" => form(
            "goToSpecial",
            "Go To Special",
            "edit.goToSpecial",
            vec![Choice {
                key: "kind",
                label: "Select",
                options: vec![
                    ("blanks", "Blanks"),
                    ("constants", "Constants"),
                    ("formulas", "Formulas"),
                    ("notes", "Notes"),
                    ("errors", "Errors"),
                    ("numbers", "Numbers"),
                    ("text", "Text"),
                    ("currentRegion", "Current region"),
                    ("lastCell", "Last cell"),
                    ("visible", "Visible cells only"),
                    ("conditionalFormats", "Conditional formats"),
                    ("dataValidation", "Data validation"),
                ],
            }],
            json!({"kind": "blanks"}),
        ),
        "chartTitle" => form(
            "chartTitle",
            "Chart Title",
            "chart.set",
            vec![
                Text { key: "title", label: "Title:" },
                Text { key: "xTitle", label: "Horizontal axis title:" },
                Text { key: "yTitle", label: "Vertical axis title:" },
            ],
            json!({"chart": p("chart")}),
        ),
        _ => return None,
    };
    Some(d)
}

/// Sets a value; fields that depend on it follow (Data Validation's criteria on Allow and Data;
/// Zoom's Magnification is also the percentage).
pub fn set_value(form: &mut Form, key: &str, value: Json) {
    if form.name == "zoom"
        && key == "preset"
        && let Some(p) = value.as_str().and_then(|s| s.parse::<f64>().ok())
    {
        form.values.insert("percent".into(), json!(p));
    }
    form.values.insert(key.into(), value);
    if form.name == "dataValidation" && (key == "type" || key == "operator") {
        let kind = form.values.get("type").and_then(Json::as_str).unwrap_or("any").to_string();
        let operator = form.values.get("operator").and_then(Json::as_str).unwrap_or("between").to_string();
        form.fields = data_validation_fields(&kind, &operator);
    }
}

/// Initial values of the Data Validation form: the rule stored on the active cell, so OK without
/// edits keeps it; a plain list rule when the cell has none.
fn validation_defaults(s: &Session) -> Json {
    let mut v = json!({"type": "list", "operator": "between", "ignoreBlank": true, "dropdown": true, "errorStyle": "stop"});
    let Some(doc) = s.active() else { return v };
    let Some(dv) = doc.wb.active().and_then(|s| s.validations.iter().find(|d| d.ranges.iter().any(|r| r.contains(doc.selection.active)))) else {
        return v;
    };
    // The enums serialize as their variant names; the command takes them in camelCase.
    let camel = |x: Json| {
        let name = x.as_str().unwrap_or_default();
        let mut chars = name.chars();
        chars.next().map(|c| c.to_lowercase().chain(chars).collect::<String>()).unwrap_or_default()
    };
    v["type"] = json!(camel(json!(dv.kind)));
    v["operator"] = json!(camel(json!(dv.op)));
    v["errorStyle"] = json!(camel(json!(dv.error_style)));
    v["formula1"] = json!(dv.f1);
    if let Some(f2) = &dv.f2 {
        v["formula2"] = json!(f2);
    }
    v["ignoreBlank"] = json!(dv.allow_blank);
    v["dropdown"] = json!(dv.in_cell_dropdown);
    v["inputTitle"] = json!(dv.input_title);
    v["inputMessage"] = json!(dv.input_message);
    v["errorTitle"] = json!(dv.error_title);
    v["errorMessage"] = json!(dv.error_message);
    v
}

fn abs(r: &gridcraft_core::RangeRef) -> String {
    let a = |c: gridcraft_core::CellRef| format!("${}${}", gridcraft_core::col_to_letters(c.col), c.row + 1);
    if r.is_single() { a(r.start) } else { format!("{}:{}", a(r.start), a(r.end)) }
}

/// The criterion controls in Excel's Settings tab depend on both Allow and Data.
/// `formula1` is the sole source for a list or custom rule; `formula2` is a
/// second bound only for Between and Not between.
pub fn data_validation_fields(kind: &str, operator: &str) -> Vec<Field> {
    use Field::*;
    let mut fields = vec![Choice {
        key: "type",
        label: "Allow",
        options: vec![
            ("any", "Any value"),
            ("whole", "Whole number"),
            ("decimal", "Decimal"),
            ("list", "List"),
            ("date", "Date"),
            ("time", "Time"),
            ("textLength", "Text length"),
            ("custom", "Custom"),
        ],
    }];
    let two_bounds = matches!(operator, "between" | "notBetween");
    match kind {
        "list" => {
            fields.push(Text { key: "formula1", label: "Source:" });
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
            fields.push(Check { key: "dropdown", label: "In-cell dropdown" });
        }
        "custom" => {
            fields.push(Text { key: "formula1", label: "Formula:" });
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
        }
        "whole" | "decimal" | "date" | "time" | "textLength" => {
            fields.push(Choice {
                key: "operator",
                label: "Data",
                options: vec![
                    ("between", "between"),
                    ("notBetween", "not between"),
                    ("equal", "equal to"),
                    ("notEqual", "not equal to"),
                    ("greater", "greater than"),
                    ("less", "less than"),
                    ("greaterOrEqual", "greater than or equal to"),
                    ("lessOrEqual", "less than or equal to"),
                ],
            });
            let lower = matches!(operator, "greater" | "greaterOrEqual");
            let (first, second) = match kind {
                "date" => (
                    if two_bounds || lower {
                        "Start date:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "End date:"
                    } else {
                        "Date:"
                    },
                    "End date:",
                ),
                "time" => (
                    if two_bounds || lower {
                        "Start time:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "End time:"
                    } else {
                        "Time:"
                    },
                    "End time:",
                ),
                _ => (
                    if two_bounds || lower {
                        "Minimum:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "Maximum:"
                    } else {
                        "Value:"
                    },
                    "Maximum:",
                ),
            };
            fields.push(Text { key: "formula1", label: first });
            if two_bounds {
                fields.push(Text { key: "formula2", label: second });
            }
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
        }
        _ => {}
    }
    fields.push(Text { key: "inputMessage", label: "Input message:" });
    fields.push(Choice {
        key: "errorStyle",
        label: "Error style",
        options: vec![("stop", "Stop"), ("warning", "Warning"), ("information", "Information")],
    });
    fields.push(Text { key: "errorMessage", label: "Error message:" });
    fields
}

/// Builds the command params from form values for dialogs that need translation.
pub fn build_params(name: &str, values: &Map<String, Json>) -> Json {
    let mut v = Json::Object(values.clone());
    match name {
        "dataValidation" => {
            let kind = values.get("type").and_then(Json::as_str).unwrap_or("any");
            let operator = values.get("operator").and_then(Json::as_str).unwrap_or("between");
            if let Some(m) = v.as_object_mut() {
                if !matches!(operator, "between" | "notBetween") || matches!(kind, "any" | "list" | "custom") {
                    m.remove("formula2");
                }
                if matches!(kind, "any" | "list" | "custom") {
                    m.remove("operator");
                }
                if kind == "any" {
                    m.remove("formula1");
                }
                if kind != "list" {
                    m.remove("dropdown");
                }
            }
        }
        "textToColumns" => {
            let mut delims = Vec::new();
            for (k, c) in [("tab", "\t"), ("semicolon", ";"), ("comma", ","), ("space", " ")] {
                if values.get(k).and_then(Json::as_bool) == Some(true) {
                    delims.push(json!(c));
                }
            }
            v["delimiters"] = Json::Array(delims);
        }
        "removeDuplicates" | "subtotal" => {
            if let Some(t) = values.get("columnsText").and_then(Json::as_str).filter(|t| !t.trim().is_empty()) {
                v["columns"] = Json::Array(t.split(',').map(|c| json!(c.trim().to_ascii_uppercase())).collect());
            }
        }
        "allowEditRanges" => {
            if let Some(m) = v.as_object_mut() {
                // Blank fields mean "unchanged"; the check box clears a password.
                m.retain(|_, x| x.as_str().is_none_or(|s| !s.trim().is_empty()));
                if m.remove("removePassword").and_then(|x| x.as_bool()) == Some(true) {
                    m.insert("password".into(), json!(""));
                }
            }
        }
        "zoom" => {
            if let Some(p) = values.get("preset").and_then(Json::as_str)
                && values.get("percent").is_none()
            {
                v["percent"] = json!(p.parse::<f64>().unwrap_or(100.0));
            }
        }
        "cfQuick" => {
            let mut rule = Map::new();
            for (k, val) in values {
                rule.insert(k.clone(), val.clone());
            }
            v = json!({"rule": Json::Object(rule)});
        }
        "series" => {
            if let Some(s) = values.get("stop").and_then(Json::as_str).and_then(|s| s.parse::<f64>().ok()) {
                v["stop"] = json!(s);
            } else if let Some(m) = v.as_object_mut() {
                m.remove("stop");
            }
        }
        "defineName" => {
            if let Some(m) = v.as_object_mut() {
                for key in ["scope", "oldScope"] {
                    if m.get(key).and_then(Json::as_str) == Some("Workbook") {
                        m.remove(key);
                    }
                }
            }
        }
        _ => {}
    }
    v
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Every form the engine knows.
    pub(crate) const NAMES: &[&str] = &[
        "insertCells",
        "insertPaste",
        "deleteCells",
        "rowHeight",
        "columnWidth",
        "defaultWidth",
        "renameSheet",
        "moveSheet",
        "protectSheet",
        "allowEditRanges",
        "protectWorkbook",
        "unprotectWorkbook",
        "openPassword",
        "unprotectSheet",
        "unhideSheet",
        "pasteSpecial",
        "series",
        "comment",
        "openLink",
        "insertLink",
        "sparkline",
        "headerFooter",
        "pageSetup",
        "textToColumns",
        "removeDuplicates",
        "forecastSheet",
        "subtotal",
        "zoom",
        "defineName",
        "dataValidation",
        "cfQuick",
        "pivotCalculatedField",
        "goToSpecial",
        "chartTitle",
    ];

    fn session() -> Session {
        let mut s = Session::new();
        s.new_workbook();
        s
    }

    #[test]
    fn every_form_opens_with_and_without_params() {
        let mut s = session();
        for name in NAMES {
            for params in [json!({}), json!({"sheet": 7, "threaded": false, "type": "timePeriod", "url": 3, "oldName": "X"})] {
                let form = open(&mut s, name, &params);
                assert!(form.as_ref().is_some_and(|f| !f.title.is_empty() && !f.command.is_empty()), "{name}");
            }
        }
        assert!(open(&mut s, "find", &json!({})).is_none(), "not a form");
        // Without a workbook they still open (the engine's command then says what's missing).
        assert!(open(&mut Session::new(), "rowHeight", &json!({})).is_some());
    }

    #[test]
    fn data_validation_fields_follow_allow_and_data() {
        let mut s = session();
        let mut dv = open(&mut s, "dataValidation", &json!({})).unwrap();
        set_value(&mut dv, "type", json!("whole"));
        set_value(&mut dv, "operator", json!("between"));
        assert!(dv.fields.iter().any(|f| matches!(f, Field::Text { key: "formula2", .. })));
        set_value(&mut dv, "operator", json!("greater"));
        assert!(!dv.fields.iter().any(|f| matches!(f, Field::Text { key: "formula2", .. })));
        let params = build_params(&dv.name, &dv.values);
        assert!(params.get("formula2").is_none());
        let mut zoom = open(&mut s, "zoom", &json!({})).unwrap();
        set_value(&mut zoom, "preset", json!("75"));
        assert_eq!(build_params(&zoom.name, &zoom.values)["percent"], json!(75.0));
    }

    #[test]
    fn row_height_starts_from_the_active_row() {
        let mut s = session();
        s.execute("home.rowHeight", json!({"rows": "1:1", "height": 33})).ok();
        let f = open(&mut s, "rowHeight", &json!({})).unwrap();
        assert_eq!(f.values["height"], json!(33.0));
        let applied = s.execute(f.command, build_params(&f.name, &f.values));
        assert!(applied.is_ok(), "{applied:?}");
    }
}
