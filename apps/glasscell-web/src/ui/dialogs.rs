//! Dialogs: the engine's forms drawn generically, and Find and Replace and Go To. Like the Mac's
//! sheets they act only through commands, and the engine's refusal stays in the dialog.

use std::collections::HashMap;

use gridcraft_engine::core::RangeRef;
use gridcraft_engine::forms::{self, Field, Form};

use super::*;
use crate::doc::{FindOptions, Found, FunctionRow, NameRow, RESULT_DIALOGS, RuleRow, sort_columns};

/// An open dialog.
#[derive(Clone)]
pub(super) enum Dialog {
    /// One of the engine's form dialogs.
    Form(Form),
    Find {
        replace: bool,
    },
    GoTo,
    /// Sort `range` (the engine guessed whether it has a header row).
    Sort {
        range: RangeRef,
        header: bool,
    },
    NameManager,
    InsertFunction,
    GoalSeek,
    /// The conditional-format rules of the sheet.
    Rules,
    /// A query's result as text: (title, command).
    Report {
        title: &'static str,
        command: &'static str,
    },
}

/// The engine's form dialogs, by name (`forms::open`).
const FORMS: &[&str] = &[
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

impl App {
    /// The dialog a menu-style invocation of `id` (no parameters) opens: Find, Replace and Go To,
    /// and the engine form whose command it is (Row Height…, Rename Sheet…). Programmatic calls
    /// never open dialogs; the page does what a menu does.
    pub(super) fn dialog_for(self, id: &str) -> Option<(String, Json)> {
        match id {
            "edit.find" => Some(("find".into(), json!({}))),
            "edit.replace" => Some(("find".into(), json!({"replace": true}))),
            "edit.goTo" => Some(("goTo".into(), json!({}))),
            "formulas.nameManager" => Some(("nameManager".into(), json!({}))),
            "data.goalSeek" => Some(("goalSeek".into(), json!({}))),
            "home.manageRules" => Some(("manageRules".into(), json!({}))),
            id if RESULT_DIALOGS.iter().any(|(_, _, command)| *command == id) => {
                RESULT_DIALOGS.iter().find(|(_, _, command)| *command == id).map(|(name, _, _)| (name.to_string(), json!({})))
            }
            _ => self
                .doc
                .try_update_value(|d| {
                    FORMS.iter().find(|name| d.open_form(name, &json!({})).is_some_and(|f| f.command == id)).map(|n| (n.to_string(), json!({})))
                })
                .flatten(),
        }
    }

    /// Opens the dialog a command asked for (`UiRequest::Dialog`).
    pub(super) fn open_dialog(self, name: &str, params: &Json) {
        let dialog = match name {
            "find" => Some(Dialog::Find { replace: params.get("replace").and_then(Json::as_bool).unwrap_or(false) }),
            "goTo" => Some(Dialog::GoTo),
            "nameManager" => Some(Dialog::NameManager),
            "goalSeek" => Some(Dialog::GoalSeek),
            "manageRules" => Some(Dialog::Rules),
            n if RESULT_DIALOGS.iter().any(|(name, _, _)| *name == n) => {
                RESULT_DIALOGS.iter().find(|(name, _, _)| *name == n).map(|(_, title, command)| Dialog::Report { title, command })
            }
            "insertFunction" => Some(Dialog::InsertFunction),
            "sort" => {
                let range = params
                    .get("range")
                    .and_then(Json::as_str)
                    .and_then(RangeRef::parse)
                    .or_else(|| self.doc.with_value(|d| d.selection().map(|s| s.current())));
                range.map(|range| Dialog::Sort { range, header: params.get("header").and_then(Json::as_bool).unwrap_or(true) })
            }
            _ => self.doc.try_update_value(|d| d.open_form(name, params)).flatten().map(Dialog::Form),
        };
        match dialog {
            Some(d) => {
                self.filter.set(None);
                self.dialog.set(Some(d));
            }
            None => self.show(Some(format!("“{name}” needs a dialog, which isn't on the web yet."))),
        }
    }

    fn close_dialog(self) {
        self.dialog.set(None);
        self.focus_grid();
    }
}

#[component]
pub(super) fn DialogHost(app: App) -> impl IntoView {
    move || {
        app.dialog.get().map(|d| match d {
            Dialog::Form(form) => view! { <FormDialog app=app form=form/> }.into_any(),
            Dialog::Find { replace } => view! { <FindDialog app=app replace=replace/> }.into_any(),
            Dialog::GoTo => view! { <GoToDialog app=app/> }.into_any(),
            Dialog::Sort { range, header } => view! { <SortDialog app=app range=range header=header/> }.into_any(),
            Dialog::NameManager => view! { <NameManagerDialog app=app/> }.into_any(),
            Dialog::InsertFunction => view! { <InsertFunctionDialog app=app/> }.into_any(),
            Dialog::GoalSeek => view! { <GoalSeekDialog app=app/> }.into_any(),
            Dialog::Rules => view! { <RulesDialog app=app/> }.into_any(),
            Dialog::Report { title, command } => view! { <ReportDialog app=app title=title command=command/> }.into_any(),
        })
    }
}

/// A dialog's frame: a title, the content, Esc to close, the first field focused. A floating one
/// leaves the grid usable behind it (Find); the others dim it.
#[component]
fn Modal(app: App, title: String, #[prop(optional)] floating: bool, #[prop(optional)] wide: bool, children: Children) -> impl IntoView {
    let panel = NodeRef::<html::Div>::new();
    Effect::new(move |_| {
        if let Some(el) = panel.get()
            && let Ok(Some(first)) = el.query_selector("input:not([type=checkbox]), select")
            && let Ok(first) = first.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = first.focus();
        }
    });
    let label = title.clone();
    view! {
        <div class="dialog-host" class:floating=floating>
            <Show when=move || !floating>
                <div class="dialog-scrim" on:pointerdown=move |_| app.close_dialog()></div>
            </Show>
            <div
                class="glass dialog"
                class:wide=wide
                role="dialog"
                aria-label=label
                node_ref=panel
                on:keydown=move |ev: KeyboardEvent| {
                    if ev.key() == "Escape" {
                        app.close_dialog();
                    }
                }
            >
                <h2>{title}</h2>
                {children()}
            </div>
        </div>
    }
}

// MARK: - Forms

#[component]
fn FormDialog(app: App, form: Form) -> impl IntoView {
    let title = form.title.clone();
    // The field list changes only when a value makes it (Data Validation): typing must not
    // rebuild the inputs.
    let fields = RwSignal::new_local(form.fields.clone());
    let form = RwSignal::new_local(form);
    let error = RwSignal::new(None::<String>);
    // What was typed in number fields, so "4." stays "4." while it is a 4.
    let raw = RwSignal::new_local(HashMap::<&'static str, String>::new());

    let set = move |key: &'static str, value: Json| {
        form.update(|f| forms::set_value(f, key, value));
        let now = form.with(|f| f.fields.clone());
        if fields.with(|f| *f != now) {
            fields.set(now);
        }
    };
    let text_of = move |key: &'static str| {
        form.with(|f| match f.values.get(key) {
            Some(Json::String(s)) => s.clone(),
            Some(Json::Number(n)) => n.to_string(),
            _ => String::new(),
        })
    };
    let flag_of = move |key: &'static str| form.with(|f| f.values.get(key).and_then(Json::as_bool).unwrap_or(false));
    let submit = move || {
        let f = form.get_untracked();
        match app.doc.try_update_value(|d| d.submit_form(&f)) {
            Some(Err(e)) => error.set(Some(e)),
            _ => {
                app.dialog.set(None);
                app.show(None);
                app.bump();
                app.focus_grid();
            }
        }
    };

    view! {
        <Modal app=app title=title>
            <form on:submit=move |ev| {
                ev.prevent_default();
                submit();
            }>
                <div class="fields">
                    {move || {
                        fields
                            .get()
                            .into_iter()
                            .map(|field| match field {
                                Field::Text { key, label } | Field::Password { key, label } => {
                                    let secret = matches!(field, Field::Password { .. });
                                    view! {
                                        <label class="frow">
                                            <span>{label}</span>
                                            <input
                                                class="field"
                                                type=if secret { "password" } else { "text" }
                                                prop:value=move || text_of(key)
                                                on:input=move |ev| set(key, json!(event_target_value(&ev)))
                                            />
                                        </label>
                                    }
                                        .into_any()
                                }
                                Field::Number { key, label } => {
                                    view! {
                                        <label class="frow">
                                            <span>{label}</span>
                                            <input
                                                class="field"
                                                type="text"
                                                inputmode="decimal"
                                                prop:value=move || {
                                                    let n = text_of(key);
                                                    raw.with(|r| match r.get(key) {
                                                        Some(s) if s.parse::<f64>().ok().map(|x| x.to_string()).as_deref() == Some(n.as_str()) => s.clone(),
                                                        _ => n,
                                                    })
                                                }
                                                on:input=move |ev| {
                                                    let typed = event_target_value(&ev);
                                                    raw.update(|r| {
                                                        r.insert(key, typed.clone());
                                                    });
                                                    match typed.trim().parse::<f64>() {
                                                        Ok(n) => set(key, json!(n)),
                                                        Err(_) => set(key, json!(typed)),
                                                    }
                                                }
                                            />
                                        </label>
                                    }
                                        .into_any()
                                }
                                Field::Choice { key, label, options } => {
                                    view! {
                                        <label class="frow">
                                            <span>{label}</span>
                                            <select class="field" on:change=move |ev| set(key, json!(event_target_value(&ev)))>
                                                {options
                                                    .into_iter()
                                                    .map(|(value, text)| view! { <option value=value selected=move || text_of(key) == value>{text}</option> })
                                                    .collect_view()}
                                            </select>
                                        </label>
                                    }
                                        .into_any()
                                }
                                Field::Check { key, label } => {
                                    view! {
                                        <label class="check">
                                            <input type="checkbox" prop:checked=move || flag_of(key) on:change=move |ev| set(key, json!(event_target_checked(&ev)))/>
                                            {label}
                                        </label>
                                    }
                                        .into_any()
                                }
                                Field::Note(text) => view! { <p class="note">{text}</p> }.into_any(),
                            })
                            .collect_view()
                    }}
                </div>
                <Show when=move || error.with(Option::is_some)>
                    <p class="failure" role="alert">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="row">
                    <div class="spacer"></div>
                    <button type="button" class="btn" on:click=move |_| app.close_dialog()>"Cancel"</button>
                    <button type="submit" class="btn primary">"OK"</button>
                </div>
            </form>
        </Modal>
    }
}

// MARK: - Find and Replace

#[component]
fn FindDialog(app: App, replace: bool) -> impl IntoView {
    let replacing = RwSignal::new(replace);
    let what = RwSignal::new(String::new());
    let with = RwSignal::new(String::new());
    let match_case = RwSignal::new(false);
    let whole_cell = RwSignal::new(false);
    let workbook = RwSignal::new(false);
    let found = RwSignal::new_local(Vec::<Found>::new());
    let status = RwSignal::new(String::new());
    let options = move || FindOptions {
        what: what.get_untracked(),
        with: with.get_untracked(),
        match_case: match_case.get_untracked(),
        whole_cell: whole_cell.get_untracked(),
        workbook: workbook.get_untracked(),
    };
    // Runs a search or replace; whatever it selects is brought into view.
    let run = move |f: fn(&mut Doc, &FindOptions, (f32, f32)) -> Result<String, String>| {
        let (o, size) = (options(), app.size.get_untracked());
        match app.doc.try_update_value(|d| f(d, &o, size)) {
            Some(Ok(message)) => status.set(message),
            Some(Err(e)) => status.set(e),
            None => {}
        }
        app.bump();
    };
    let find_next = move || {
        run(|d, o, size| {
            let cell = d.find_next(o)?;
            d.reveal_active(size);
            Ok(format!("Found {cell}"))
        })
    };
    let find_all = move || {
        let o = options();
        match app.doc.try_update_value(|d| d.find_all(&o)) {
            Some(Ok(list)) => {
                status.set(format!("{} cell(s) found", list.len()));
                found.set(list);
            }
            Some(Err(e)) => {
                found.set(Vec::new());
                status.set(e);
            }
            None => {}
        }
    };
    let replace_one = move || {
        run(|d, o, size| {
            d.replace_one(o)?;
            d.reveal_active(size);
            Ok("Replaced.".into())
        })
    };
    let replace_all = move || {
        run(|d, o, _| {
            let n = d.replace_all(o)?;
            Ok(format!("All done. {n} replacement(s)."))
        });
        found.set(Vec::new());
    };

    view! {
        <Modal app=app title="Find and Replace".to_string() floating=true>
            <div class="seg">
                <button class="tool" class:on=move || !replacing.get() on:click=move |_| replacing.set(false)>"Find"</button>
                <button class="tool" class:on=move || replacing.get() on:click=move |_| replacing.set(true)>"Replace"</button>
            </div>
            <form on:submit=move |ev| {
                ev.prevent_default();
                find_next();
            }>
                <div class="fields">
                    <label class="frow">
                        <span>"Find what:"</span>
                        <input class="field" prop:value=move || what.get() on:input=move |ev| what.set(event_target_value(&ev))/>
                    </label>
                    <Show when=move || replacing.get()>
                        <label class="frow">
                            <span>"Replace with:"</span>
                            <input class="field" prop:value=move || with.get() on:input=move |ev| with.set(event_target_value(&ev))/>
                        </label>
                    </Show>
                    <label class="check"><input type="checkbox" prop:checked=move || match_case.get() on:change=move |ev| match_case.set(event_target_checked(&ev))/>"Match case"</label>
                    <label class="check"><input type="checkbox" prop:checked=move || whole_cell.get() on:change=move |ev| whole_cell.set(event_target_checked(&ev))/>"Match entire cell contents"</label>
                    <label class="check"><input type="checkbox" prop:checked=move || workbook.get() on:change=move |ev| workbook.set(event_target_checked(&ev))/>"Within workbook"</label>
                </div>
                <Show when=move || found.with(|f| !f.is_empty())>
                    <ul class="found-list">
                        {move || {
                            found
                                .get()
                                .into_iter()
                                .map(|f| {
                                    let target = f.clone();
                                    view! {
                                        <li>
                                            <button
                                                type="button"
                                                on:click=move |_| {
                                                    let size = app.size.get_untracked();
                                                    let t = target.clone();
                                                    app.change(|d| {
                                                        d.show_found(&t)?;
                                                        d.reveal_active(size);
                                                        Ok(())
                                                    });
                                                }
                                            >
                                                <small>{f.sheet.clone()}</small>
                                                <code>{f.cell.clone()}</code>
                                                <span>{f.value.clone()}</span>
                                            </button>
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ul>
                </Show>
                <p class="status-text" role="status">{move || status.get()}</p>
                <div class="row">
                    <Show when=move || replacing.get()>
                        <button type="button" class="btn" disabled=move || what.with(String::is_empty) on:click=move |_| replace_all()>"Replace All"</button>
                        <button type="button" class="btn" disabled=move || what.with(String::is_empty) on:click=move |_| replace_one()>"Replace"</button>
                    </Show>
                    <div class="spacer"></div>
                    <button type="button" class="btn" disabled=move || what.with(String::is_empty) on:click=move |_| find_all()>"Find All"</button>
                    <button type="submit" class="btn primary" disabled=move || what.with(String::is_empty)>"Find Next"</button>
                    <button type="button" class="btn" on:click=move |_| app.close_dialog()>"Close"</button>
                </div>
            </form>
        </Modal>
    }
}

// MARK: - Go To

#[component]
fn GoToDialog(app: App) -> impl IntoView {
    let names = app.doc.try_update_value(Doc::names).unwrap_or_default();
    let reference = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let go = move || {
        let (target, size) = (reference.get_untracked(), app.size.get_untracked());
        let result = app.doc.try_update_value(|d| {
            d.go_to(&target)?;
            d.reveal_active(size);
            Ok::<(), String>(())
        });
        match result {
            Some(Err(e)) => error.set(Some(e)),
            _ => {
                app.dialog.set(None);
                app.bump();
                app.focus_grid();
            }
        }
    };
    view! {
        <Modal app=app title="Go To".to_string()>
            <form on:submit=move |ev| {
                ev.prevent_default();
                go();
            }>
                <Show when={
                    let any = !names.is_empty();
                    move || any
                }>
                    <ul class="found-list">
                        {names
                            .clone()
                            .into_iter()
                            .map(|n| {
                                let pick = n.clone();
                                view! {
                                    <li>
                                        <button type="button" class:on={
                                            let me = n.clone();
                                            move || reference.with(|r| *r == me)
                                        } on:click=move |_| reference.set(pick.clone())>
                                            {n.clone()}
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                </Show>
                <label class="frow">
                    <span>"Reference:"</span>
                    <input class="field" placeholder="B5, Sheet2!A1:C3 or a name" prop:value=move || reference.get() on:input=move |ev| reference.set(event_target_value(&ev))/>
                </label>
                <Show when=move || error.with(Option::is_some)>
                    <p class="failure" role="alert">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="row">
                    <div class="spacer"></div>
                    <button type="button" class="btn" on:click=move |_| app.close_dialog()>"Cancel"</button>
                    <button type="submit" class="btn primary" disabled=move || reference.with(String::is_empty)>"OK"</button>
                </div>
            </form>
        </Modal>
    }
}

// MARK: - Sort

#[component]
fn SortDialog(app: App, range: RangeRef, header: bool) -> impl IntoView {
    let columns = sort_columns(range);
    let header = RwSignal::new(header);
    // The sort levels: column letter and whether descending. The first is the main key.
    let levels = RwSignal::new(vec![(columns.first().cloned().unwrap_or_else(|| "A".into()), false)]);
    let error = RwSignal::new(None::<String>);
    let submit = {
        let _ = &columns;
        move || {
            let (header, keys) = (header.get_untracked(), levels.get_untracked());
            match app.doc.try_update_value(|d| d.sort(range, header, &keys)) {
                Some(Err(e)) => error.set(Some(e)),
                _ => {
                    app.dialog.set(None);
                    app.show(None);
                    app.bump();
                    app.focus_grid();
                }
            }
        }
    };
    let options_columns = columns.clone();
    let first = columns.first().cloned().unwrap_or_else(|| "A".into());
    view! {
        <Modal app=app title="Sort".to_string()>
            <form on:submit=move |ev| {
                ev.prevent_default();
                submit();
            }>
                <label class="check">
                    <input type="checkbox" prop:checked=move || header.get() on:change=move |ev| header.set(event_target_checked(&ev))/>
                    "My data has headers"
                </label>
                <div class="fields">
                    {move || {
                        let options = options_columns.clone();
                        let count = levels.with(Vec::len);
                        levels
                            .get()
                            .into_iter()
                            .enumerate()
                            .map(|(i, (column, desc))| {
                                let options = options.clone();
                                view! {
                                    <div class="frow">
                                        <span>{if i == 0 { "Sort by" } else { "Then by" }}</span>
                                        <select
                                            class="field"
                                            aria-label="Column"
                                            on:change=move |ev| {
                                                let value = event_target_value(&ev);
                                                levels.update(|l| {
                                                    if let Some(level) = l.get_mut(i) {
                                                        level.0 = value;
                                                    }
                                                });
                                            }
                                        >
                                            {options
                                                .into_iter()
                                                .map(|c| {
                                                    let (value, label) = (c.clone(), format!("Column {c}"));
                                                    view! { <option value=value selected=c == column>{label}</option> }
                                                })
                                                .collect_view()}
                                        </select>
                                        <select
                                            class="field"
                                            aria-label="Order"
                                            on:change=move |ev| {
                                                let descending = event_target_value(&ev) == "desc";
                                                levels.update(|l| {
                                                    if let Some(level) = l.get_mut(i) {
                                                        level.1 = descending;
                                                    }
                                                });
                                            }
                                        >
                                            <option value="asc" selected=!desc>"A to Z"</option>
                                            <option value="desc" selected=desc>"Z to A"</option>
                                        </select>
                                        <button
                                            type="button"
                                            class="btn"
                                            aria-label="Remove level"
                                            disabled=count == 1
                                            on:click=move |_| levels.update(|l| {
                                                if i < l.len() {
                                                    l.remove(i);
                                                }
                                            })
                                        >
                                            "−"
                                        </button>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                </div>
                <div class="row">
                    <button
                        type="button"
                        class="btn"
                        disabled=move || levels.with(|l| l.len() >= 8)
                        on:click={
                            let first = first.clone();
                            move |_| levels.update(|l| l.push((first.clone(), false)))
                        }
                    >
                        "Add Level"
                    </button>
                </div>
                <Show when=move || error.with(Option::is_some)>
                    <p class="failure" role="alert">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="row">
                    <div class="spacer"></div>
                    <button type="button" class="btn" on:click=move |_| app.close_dialog()>"Cancel"</button>
                    <button type="submit" class="btn primary">"OK"</button>
                </div>
            </form>
        </Modal>
    }
}

// MARK: - Name Manager

#[component]
fn NameManagerDialog(app: App) -> impl IntoView {
    let rows = RwSignal::new_local(app.doc.try_update_value(Doc::name_rows).unwrap_or_default());
    let selected = RwSignal::new(None::<usize>);
    let chosen = move || -> Option<NameRow> { selected.get().and_then(|i| rows.with(|r| r.get(i).cloned())) };
    view! {
        <Modal app=app title="Name Manager".to_string() wide=true>
            <div class="table-wrap">
                <table class="names">
                    <thead>
                        <tr><th>"Name"</th><th>"Refers To"</th><th>"Scope"</th><th>"Comment"</th></tr>
                    </thead>
                    <tbody>
                        {move || {
                            rows
                                .get()
                                .into_iter()
                                .enumerate()
                                .map(|(i, n)| {
                                    view! {
                                        <tr class:on=move || selected.get() == Some(i) on:click=move |_| selected.set(Some(i))>
                                            <td>{n.name}</td>
                                            <td><code>{n.refers_to}</code></td>
                                            <td>{n.scope}</td>
                                            <td>{n.comment}</td>
                                        </tr>
                                    }
                                })
                                .collect_view()
                        }}
                    </tbody>
                </table>
                <Show when=move || rows.with(Vec::is_empty)>
                    <p class="note">"No names are defined yet."</p>
                </Show>
            </div>
            <div class="row">
                <button class="btn" on:click=move |_| app.open_dialog("defineName", &json!({}))>"New…"</button>
                <button class="btn" disabled=move || chosen().is_none() on:click=move |_| {
                    if let Some(n) = chosen() {
                        app.open_dialog("defineName", &Doc::edit_name_params(&n));
                    }
                }>
                    "Edit…"
                </button>
                <button class="btn" disabled=move || chosen().is_none() on:click=move |_| {
                    if let Some(n) = chosen() {
                        app.change(|d| d.delete_name(&n));
                        rows.set(app.doc.try_update_value(Doc::name_rows).unwrap_or_default());
                        selected.set(None);
                    }
                }>
                    "Delete"
                </button>
                <div class="spacer"></div>
                <button class="btn" on:click=move |_| app.close_dialog()>"Close"</button>
            </div>
        </Modal>
    }
}

// MARK: - Insert Function

#[component]
fn InsertFunctionDialog(app: App) -> impl IntoView {
    let all = StoredValue::new_local(app.doc.try_update_value(Doc::functions).unwrap_or_default());
    let search = RwSignal::new(String::new());
    let selected = RwSignal::new(None::<String>);
    let shown = move || -> Vec<FunctionRow> {
        let q = search.get().trim().to_lowercase();
        all.with_value(|f| f.iter().filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q)).cloned().collect())
    };
    let detail = move || selected.get().and_then(|n| all.with_value(|f| f.iter().find(|r| r.name == n).cloned()));
    let insert = move |name: String| {
        app.close_dialog();
        app.act("formulas.insertFunction", json!({"name": name}));
    };
    view! {
        <Modal app=app title="Insert Function".to_string()>
            <form on:submit=move |ev| {
                ev.prevent_default();
                if let Some(n) = selected.get_untracked() {
                    insert(n);
                }
            }>
                <input class="field" placeholder="Search for a function" prop:value=move || search.get() on:input=move |ev| search.set(event_target_value(&ev))/>
                <ul class="found-list tall">
                    {move || {
                        shown()
                            .into_iter()
                            .map(|f| {
                                let (name, pick, open) = (f.name.clone(), f.name.clone(), f.name.clone());
                                view! {
                                    <li>
                                        <button
                                            type="button"
                                            class:on=move || selected.with(|s| s.as_deref() == Some(name.as_str()))
                                            on:click=move |_| selected.set(Some(pick.clone()))
                                            on:dblclick=move |_| insert(open.clone())
                                        >
                                            <code>{f.name}</code>
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view()
                    }}
                </ul>
                <Show when=move || detail().is_some()>
                    <div class="fn-detail">
                        <code>{move || detail().map(|f| f.signature).unwrap_or_default()}</code>
                        <p class="note">{move || detail().map(|f| f.description).unwrap_or_default()}</p>
                    </div>
                </Show>
                <div class="row">
                    <div class="spacer"></div>
                    <button type="button" class="btn" on:click=move |_| app.close_dialog()>"Cancel"</button>
                    <button type="submit" class="btn primary" disabled=move || selected.with(Option::is_none)>"OK"</button>
                </div>
            </form>
        </Modal>
    }
}

// MARK: - Goal Seek

#[component]
fn GoalSeekDialog(app: App) -> impl IntoView {
    let active = app.doc.with_value(|d| d.selection().map(|s| s.active.a1())).unwrap_or_default();
    let set = RwSignal::new(active);
    let to = RwSignal::new("0".to_string());
    let changing = RwSignal::new(String::new());
    let outcome = RwSignal::new(None::<String>);
    let valid = move || set.with(|s| !s.trim().is_empty()) && changing.with(|c| !c.trim().is_empty()) && to.with(|t| t.trim().parse::<f64>().is_ok());
    let submit = move || {
        let (s, t, c) = (set.get_untracked(), to.get_untracked(), changing.get_untracked());
        let Ok(target) = t.trim().parse::<f64>() else { return };
        let result = app.doc.try_update_value(|d| d.goal_seek(s.trim(), target, c.trim()));
        outcome.set(match result {
            Some(Ok(r)) if r.converged => Some(format!("Goal Seek found a solution: {}.", r.value)),
            Some(Ok(_)) => Some("Goal Seek may not have found a solution.".into()),
            Some(Err(e)) => Some(e),
            None => None,
        });
        app.bump();
    };
    view! {
        <Modal app=app title="Goal Seek".to_string()>
            <form on:submit=move |ev| {
                ev.prevent_default();
                if valid() {
                    submit();
                }
            }>
                <div class="fields">
                    <label class="frow">
                        <span>"Set cell:"</span>
                        <input class="field" prop:value=move || set.get() on:input=move |ev| set.set(event_target_value(&ev))/>
                    </label>
                    <label class="frow">
                        <span>"To value:"</span>
                        <input class="field" inputmode="decimal" prop:value=move || to.get() on:input=move |ev| to.set(event_target_value(&ev))/>
                    </label>
                    <label class="frow">
                        <span>"By changing cell:"</span>
                        <input class="field" prop:value=move || changing.get() on:input=move |ev| changing.set(event_target_value(&ev))/>
                    </label>
                </div>
                <Show when=move || outcome.with(Option::is_some)>
                    <p class="status-text" role="status">{move || outcome.get().unwrap_or_default()}</p>
                </Show>
                <div class="row">
                    <div class="spacer"></div>
                    <button type="button" class="btn" on:click=move |_| app.close_dialog()>"Close"</button>
                    <button type="submit" class="btn primary" disabled=move || !valid()>"OK"</button>
                </div>
            </form>
        </Modal>
    }
}

// MARK: - Conditional-format rules

#[component]
fn RulesDialog(app: App) -> impl IntoView {
    let load = move || app.doc.try_update_value(Doc::rules).unwrap_or_default();
    let rules = RwSignal::new_local(load());
    let selected = RwSignal::new(None::<usize>);
    let act = move |action: &'static str, next: fn(usize) -> usize| {
        let Some(i) = selected.get_untracked() else { return };
        app.change(|d| d.rule_action(action, i));
        rules.set(load());
        selected.set(if action == "delete" { None } else { Some(next(i)) });
    };
    let count = move || rules.with(Vec::len);
    view! {
        <Modal app=app title="Conditional Formatting Rules".to_string() wide=true>
            <ul class="found-list tall">
                {move || {
                    rules
                        .get()
                        .into_iter()
                        .enumerate()
                        .map(|(i, r): (usize, RuleRow)| {
                            view! {
                                <li>
                                    <button type="button" class:on=move || selected.get() == Some(i) on:click=move |_| selected.set(Some(i))>
                                        <span>{r.kind}</span>
                                        <code>{r.ranges}</code>
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>
            <Show when=move || count() == 0>
                <p class="note">"This sheet has no conditional formatting rules."</p>
            </Show>
            <div class="row">
                <button class="btn" disabled=move || selected.get().is_none() on:click=move |_| act("delete", |i| i)>"Delete Rule"</button>
                <button class="btn" disabled=move || selected.get().is_none_or(|i| i == 0) on:click=move |_| act("moveUp", |i| i.saturating_sub(1))>"Move Up"</button>
                <button class="btn" disabled=move || selected.get().is_none_or(|i| i + 1 >= count()) on:click=move |_| act("moveDown", |i| i + 1)>"Move Down"</button>
                <div class="spacer"></div>
                <button class="btn" on:click=move |_| app.close_dialog()>"Close"</button>
            </div>
        </Modal>
    }
}

// MARK: - Reports

/// A dialog that reports: the query's result as indented text.
#[component]
fn ReportDialog(app: App, title: &'static str, command: &'static str) -> impl IntoView {
    let result = app.doc.try_update_value(|d| d.report(command)).unwrap_or_else(|| Ok(Vec::new()));
    view! {
        <Modal app=app title=title.to_string() wide=true>
            {match result {
                Ok(lines) if lines.is_empty() => view! { <p class="note">"Nothing to report."</p> }.into_any(),
                Ok(lines) => view! { <pre class="report">{lines.join("\n")}</pre> }.into_any(),
                Err(e) => view! { <p class="failure" role="alert">{e}</p> }.into_any(),
            }}
            <div class="row">
                <div class="spacer"></div>
                <button class="btn primary" on:click=move |_| app.close_dialog()>"Close"</button>
            </div>
        </Modal>
    }
}
