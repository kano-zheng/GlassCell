//! Context menus (cells, row and column headers, sheet tabs) and the clipboard they use. Items
//! run commands, enabled as the engine says, like the Mac's menus.

use std::collections::HashMap;

use super::*;

/// What a menu item does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Action {
    /// A menu-style call of a command (a dialog opens where it needs one).
    Command(&'static str),
    Cut,
    Copy,
    Paste,
    Inspector,
    FilterByValue,
    InsertSheetBefore(usize),
    DeleteSheet(usize),
    RenameSheet(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Entry {
    Item { title: &'static str, action: Action, enabled: bool },
    Separator,
}

/// What a right-click was on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Target {
    Cells,
    Rows,
    Columns,
    Tab(usize),
}

/// An open menu: where (window points) and what it lists.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OpenMenu {
    pub x: f32,
    pub y: f32,
    pub entries: Vec<Entry>,
}

/// The items of a menu for `target`; `enabled` says which commands can run now, `sheets` is the
/// number of sheets (the last one can't be deleted).
pub(super) fn entries(target: Target, enabled: &HashMap<&'static str, bool>, sheets: usize) -> Vec<Entry> {
    let on = |id: &str| enabled.get(id).copied().unwrap_or(true);
    let item = |title: &'static str, id: &'static str| Entry::Item { title, action: Action::Command(id), enabled: on(id) };
    let plain = |title: &'static str, action: Action, enabled: bool| Entry::Item { title, action, enabled };
    match target {
        Target::Tab(i) => vec![
            plain("Insert Sheet", Action::InsertSheetBefore(i), on("home.insertSheet")),
            plain("Rename…", Action::RenameSheet(i), true),
            plain("Delete Sheet", Action::DeleteSheet(i), sheets > 1),
        ],
        _ => {
            let mut out = vec![
                plain("Cut", Action::Cut, on("edit.cut")),
                plain("Copy", Action::Copy, on("edit.copy")),
                plain("Paste", Action::Paste, true),
                item("Paste Special…", "edit.pasteSpecial"),
                Entry::Separator,
            ];
            match target {
                Target::Cells => out.extend([
                    item("Insert…", "home.insertCells"),
                    item("Delete…", "home.deleteCells"),
                    item("Clear Contents", "edit.clearContents"),
                    Entry::Separator,
                    plain("Filter by Selected Cell's Value", Action::FilterByValue, on("data.filterBy")),
                    item("Sort A to Z", "data.sortAscending"),
                    item("Sort Z to A", "data.sortDescending"),
                    Entry::Separator,
                    item("New Comment", "review.newComment"),
                    item("New Note", "review.newNote"),
                    Entry::Separator,
                    plain("Format Cells…", Action::Inspector, true),
                    item("Define Name…", "formulas.defineName"),
                    item("Link…", "insert.link"),
                ]),
                Target::Rows => out.extend([
                    item("Insert", "home.insertRows"),
                    item("Delete", "home.deleteRows"),
                    item("Clear Contents", "edit.clearContents"),
                    Entry::Separator,
                    item("Row Height…", "home.rowHeight"),
                    item("AutoFit Row Height", "home.autofitRowHeight"),
                    item("Hide", "home.hideRows"),
                    item("Unhide", "home.unhideRows"),
                    item("Group", "data.group"),
                ]),
                _ => out.extend([
                    item("Insert", "home.insertColumns"),
                    item("Delete", "home.deleteColumns"),
                    item("Clear Contents", "edit.clearContents"),
                    Entry::Separator,
                    item("Column Width…", "home.columnWidth"),
                    item("AutoFit Column Width", "home.autofitColumnWidth"),
                    item("Hide", "home.hideColumns"),
                    item("Unhide", "home.unhideColumns"),
                    item("Sort A to Z", "data.sortAscending"),
                    item("Group", "data.group"),
                ]),
            }
            out
        }
    }
}

impl App {
    /// Opens the menu for `target` at window point `(x, y)`.
    pub(super) fn open_menu(self, target: Target, x: f32, y: f32) {
        let (enabled, sheets) = self
            .doc
            .try_update_value(|d| {
                let enabled: HashMap<&'static str, bool> = d.session.commands().into_iter().map(|c| (c.id, c.enabled)).collect();
                (enabled, d.sheet_tabs().len())
            })
            .unwrap_or_default();
        self.filter.set(None);
        self.menu.set(Some(OpenMenu { x, y, entries: entries(target, &enabled, sheets) }));
    }

    pub(super) fn run_action(self, action: Action) {
        self.menu.set(None);
        match action {
            Action::Command(id) => self.act(id, json!({})),
            Action::Cut => self.copy_selection(true),
            Action::Copy => self.copy_selection(false),
            Action::Paste => self.paste_clipboard(),
            Action::Inspector => self.inspector.set(true),
            Action::FilterByValue => self.change(|d| d.filter_by_active_value()),
            Action::InsertSheetBefore(i) => self.act("home.insertSheet", json!({"before": i})),
            Action::DeleteSheet(i) => self.act("home.deleteSheet", json!({"sheet": i})),
            Action::RenameSheet(i) => {
                self.act("sheet.activate", json!({"sheet": i}));
                self.open_dialog("renameSheet", &json!({}));
            }
        }
    }

    /// Copies or cuts the selection to GridCraft's clipboard and the system's.
    pub(super) fn copy_selection(self, cut: bool) {
        match self.doc.try_update_value(|d| d.copy(cut)) {
            Some(Ok(text)) => {
                clipboard_write(&text);
                self.show(None);
            }
            Some(Err(e)) => self.show(Some(e)),
            None => {}
        }
        self.bump();
    }

    /// Pastes what the system clipboard holds (GridCraft's own copy when it can't be read).
    pub(super) fn paste_clipboard(self) {
        wasm_bindgen_futures::spawn_local(async move {
            let text = clipboard_read().await;
            self.change(|d| d.paste(text.as_deref()));
        });
    }
}

/// `navigator.clipboard`, if the page may use it.
fn clipboard() -> Option<wasm_bindgen::JsValue> {
    let navigator = web_sys::window()?.navigator();
    let cb = js_sys::Reflect::get(&navigator, &"clipboard".into()).ok()?;
    (!cb.is_undefined() && !cb.is_null()).then_some(cb)
}

fn clipboard_write(text: &str) {
    let Some(cb) = clipboard() else { return };
    if let Ok(f) = js_sys::Reflect::get(&cb, &"writeText".into())
        && let Ok(f) = f.dyn_into::<js_sys::Function>()
    {
        let _ = f.call1(&cb, &text.into());
    }
}

async fn clipboard_read() -> Option<String> {
    let cb = clipboard()?;
    let f = js_sys::Reflect::get(&cb, &"readText".into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    let promise = f.call0(&cb).ok()?.dyn_into::<js_sys::Promise>().ok()?;
    wasm_bindgen_futures::JsFuture::from(promise).await.ok()?.as_string()
}

#[component]
pub(super) fn ContextMenuHost(app: App) -> impl IntoView {
    move || {
        app.menu.get().map(|open| {
            let size = app.size_window();
            // Kept inside the window: about 28 points per item.
            let height = open.entries.iter().map(|e| if matches!(e, Entry::Item { .. }) { 28.0 } else { 9.0 }).sum::<f32>() + 12.0;
            let left = open.x.min(size.0 - 230.0).max(4.0);
            let top = if open.y + height > size.1 { (size.1 - height).max(4.0) } else { open.y };
            view! {
                <div class="ctx-scrim" on:pointerdown=move |_| app.menu.set(None) on:contextmenu=move |ev| {
                    ev.prevent_default();
                    app.menu.set(None);
                }></div>
                <div class="glass ctx" role="menu" style=format!("left:{left}px;top:{top}px")>
                    {open
                        .entries
                        .into_iter()
                        .map(|entry| match entry {
                            Entry::Separator => view! { <div class="vline"></div> }.into_any(),
                            Entry::Item { title, action, enabled } => {
                                view! { <button role="menuitem" disabled=!enabled on:click=move |_| app.run_action(action)>{title}</button> }.into_any()
                            }
                        })
                        .collect_view()}
                </div>
            }
        })
    }
}
