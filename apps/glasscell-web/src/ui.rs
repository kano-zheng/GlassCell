//! The page: toolbar, formula bar, grid, status bar and inspector, in Leptos. Like the Mac app
//! it acts only through the engine's commands (`Doc::run`) and redraws from the engine's read
//! models after each one.

use gridcraft_engine::cmd::data::FilterMenu;
use gridcraft_engine::core::{CellRef, RangeRef};
use gridcraft_engine::geometry::{Hit, Rect};
use gridcraft_engine::selection_format::SelectionFormat;
use gridcraft_engine::{CommandInfo, UiRequest};
use leptos::html;
use leptos::prelude::*;
use serde_json::{Value as Json, json};
use wasm_bindgen::JsCast as _;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlInputElement, KeyboardEvent, PointerEvent, WheelEvent};

mod dialogs;
mod files;
mod menus;

use crate::doc::{Doc, ObjectRef, ResizeEdge, kind_name};
use crate::icons;
use crate::measure::CanvasMeasure;
use crate::render::{self, Frame, Preview, Scene};
use crate::shortcuts::{self, Chord, Press};
use dialogs::{Dialog, DialogHost};
use files::{open_file, save_file};
use menus::{ContextMenuHost, OpenMenu, Target};

/// An entry in progress (the cell's editor and the formula bar share it).
#[derive(Clone, Debug, PartialEq)]
struct Edit {
    text: String,
    /// Typed in the formula bar, which keeps the keyboard.
    from_bar: bool,
}

/// What the pointer is doing between a press and its release.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    None,
    /// Selecting cells.
    Select,
    /// Resizing a column or row: where the pointer started (view points along the axis), and the
    /// size then (sheet units).
    Resize {
        edge: ResizeEdge,
        start: f32,
        original: f32,
    },
    /// Moving the selected object: where the press was.
    MoveObject {
        object: ObjectRef,
        start: (f32, f32),
    },
    /// Resizing the selected object from its corner: where the press was.
    ResizeObject {
        object: ObjectRef,
        start: (f32, f32),
    },
    /// Dragging the fill handle of `source`.
    Fill {
        source: RangeRef,
    },
    /// Dragging the selection's border: the cells of `source` move by how far the pointer moves
    /// from the cell it grabbed.
    MoveCells {
        source: RangeRef,
        grab: CellRef,
    },
    /// Dragging a new area added with ⌘-click: it starts at `anchor`, after `keep` areas.
    AddArea {
        anchor: CellRef,
        keep: usize,
    },
}

/// A filter dropdown that is open: its header cell, the filter button it hangs from (view
/// points) and what it lists.
#[derive(Clone, Debug, PartialEq)]
struct OpenFilter {
    cell: CellRef,
    anchor: Rect,
    menu: FilterMenu,
}

#[derive(Clone, Copy)]
struct App {
    doc: StoredValue<Doc, LocalStorage>,
    /// Bumped after every change to the document: what redraws everything.
    rev: RwSignal<u64>,
    fmt: RwSignal<Option<SelectionFormat>, LocalStorage>,
    size: RwSignal<(f32, f32)>,
    edit: RwSignal<Option<Edit>>,
    message: RwSignal<Option<String>>,
    inspector: RwSignal<bool>,
    palette: RwSignal<bool>,
    canvas: NodeRef<html::Canvas>,
    editor: NodeRef<html::Input>,
    /// A header edge being dragged: (columns, position along the axis), drawn as a guide line.
    guide: RwSignal<Option<(bool, f32)>>,
    /// The selected floating object.
    selected: RwSignal<Option<ObjectRef>>,
    /// What a drag in progress would do.
    preview: RwSignal<Option<Preview>>,
    /// The open filter dropdown.
    filter: RwSignal<Option<OpenFilter>, LocalStorage>,
    /// The labels of what Undo and Redo would undo and redo (`None`: nothing, so disabled).
    history: RwSignal<(Option<String>, Option<String>)>,
    /// The catalog's keyboard shortcuts, and whether this is a Mac (⌘ is the command key).
    shortcuts: StoredValue<Vec<(Chord, &'static str)>, LocalStorage>,
    mac: bool,
    /// The open dialog.
    dialog: RwSignal<Option<Dialog>, LocalStorage>,
    /// The open context menu.
    menu: RwSignal<Option<OpenMenu>, LocalStorage>,
    /// The file the workbook was opened from or last saved to (a File System Access handle).
    file: StoredValue<Option<wasm_bindgen::JsValue>, LocalStorage>,
}

impl App {
    fn new(doc: Doc) -> App {
        let fmt = doc.selection_format();
        let history = doc.history();
        let table = shortcuts::table(&doc.session.commands());
        let mac = web_sys::window().and_then(|w| w.navigator().platform().ok()).is_some_and(|p| p.contains("Mac"));
        App {
            doc: StoredValue::new_local(doc),
            rev: RwSignal::new(0),
            fmt: RwSignal::new_local(fmt),
            size: RwSignal::new((800.0, 500.0)),
            edit: RwSignal::new(None),
            message: RwSignal::new(None),
            inspector: RwSignal::new(false),
            palette: RwSignal::new(false),
            canvas: NodeRef::new(),
            editor: NodeRef::new(),
            guide: RwSignal::new(None),
            selected: RwSignal::new(None),
            preview: RwSignal::new(None),
            filter: RwSignal::new_local(None),
            history: RwSignal::new(history),
            shortcuts: StoredValue::new_local(table),
            mac,
            dialog: RwSignal::new_local(None),
            menu: RwSignal::new_local(None),
            file: StoredValue::new_local(None),
        }
    }

    /// Reads the document, redrawing the caller after every change.
    fn with<R>(self, f: impl FnOnce(&Doc) -> R) -> R {
        let _ = self.rev.get();
        self.doc.with_value(f)
    }

    /// The document changed: refresh what depends on it.
    fn bump(self) {
        self.fmt.set(self.doc.with_value(Doc::selection_format));
        self.history.set(self.doc.with_value(Doc::history));
        self.rev.update(|r| *r += 1);
    }

    fn show(self, message: Option<String>) {
        self.message.set(message);
    }

    /// Changes the document, then refreshes. An error from the engine is shown to the user.
    fn change(self, f: impl FnOnce(&mut Doc) -> Result<(), String>) {
        let r = self.doc.try_update_value(f).unwrap_or(Ok(()));
        self.show(r.err());
        self.bump();
    }

    /// Runs a command and handles what it asks of the interface.
    fn act(self, id: &str, params: Json) {
        // A command from the toolbar, a menu or a key confirms an entry in progress first.
        if self.edit.get_untracked().is_some() {
            self.commit(None);
        }
        // Without parameters it is a menu-style call: what needs a dialog opens it.
        if params.as_object().is_none_or(|p| p.is_empty())
            && let Some((name, dialog_params)) = self.dialog_for(id)
        {
            self.open_dialog(&name, &dialog_params);
            return;
        }
        let (r, requests) = self
            .doc
            .try_update_value(|d| {
                let r = d.run(id, params);
                (r, d.session.take_ui_requests())
            })
            .unwrap_or((Ok(Json::Null), Vec::new()));
        self.show(r.err());
        for request in requests {
            match request {
                UiRequest::Message(m) => self.show(Some(m)),
                UiRequest::EditCell(text) => self.begin_edit(text, false),
                UiRequest::Dialog(name, params) => self.open_dialog(&name, &params),
                UiRequest::OpenUrl(url) => {
                    if (url.starts_with("https://") || url.starts_with("http://"))
                        && let Some(w) = web_sys::window()
                    {
                        let _ = w.open_with_url_and_target_and_features(&url, "_blank", "noopener,noreferrer");
                    }
                }
            }
        }
        self.bump();
    }

    /// Runs the command a keyboard shortcut names (files and the clipboard are the page's own).
    fn shortcut(self, id: &str) {
        match id {
            "file.open" => open_file(self),
            "file.save" => save_file(self, false),
            "file.saveAs" => save_file(self, true),
            id if id.starts_with("file.") => {}
            id => self.act(id, json!({})),
        }
    }

    /// The window's size in points, for placing menus.
    fn size_window(self) -> (f32, f32) {
        let w = web_sys::window();
        let dim =
            |v: Option<Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>>| v.and_then(Result::ok).and_then(|x| x.as_f64()).unwrap_or(800.0) as f32;
        (dim(w.as_ref().map(|w| w.inner_width())), dim(w.as_ref().map(|w| w.inner_height())))
    }

    fn focus_grid(self) {
        if let Some(c) = self.canvas.get_untracked() {
            let _ = c.focus();
        }
    }

    /// Starts an entry in the active cell: with `text`, replacing it; without, from its content.
    fn begin_edit(self, text: Option<String>, from_bar: bool) {
        let text = text.unwrap_or_else(|| self.doc.with_value(Doc::entry_text));
        self.edit.set(Some(Edit { text, from_bar }));
        // The editor takes the keyboard now, not after the next render: what is typed next, and
        // the character that started the entry, land in it.
        if !from_bar && let Some(input) = self.editor.get_untracked() {
            let _ = input.focus();
        }
    }

    /// Enters the entry in the cell and moves on (`(by_row, backwards)`: Enter, Tab…).
    fn commit(self, advance: Option<(bool, bool)>) {
        let Some(edit) = self.edit.get_untracked() else { return };
        self.edit.set(None);
        let size = self.size.get_untracked();
        self.change(|d| {
            d.commit(&edit.text)?;
            if let Some((by_row, back)) = advance {
                d.advance(by_row, back)?;
                d.reveal_active(size);
            }
            Ok(())
        });
        self.focus_grid();
    }

    fn cancel(self) {
        self.edit.set(None);
        self.focus_grid();
    }

    fn select_at(self, x: f32, y: f32, extend: bool) {
        let size = self.size.get_untracked();
        self.change(|d| {
            let Some(layout) = d.layout(size) else { return Ok(()) };
            if let Some(cell) = d.cell_at(&layout, x, y) {
                return d.select(cell, extend);
            }
            // The headers select whole columns and rows, the corner selects everything.
            match (x < layout.header_w, y < layout.header_h) {
                (true, true) => d.run("edit.selectAll", json!({})).map(|_| ()),
                (false, true) => {
                    let c = layout.geo.col_at(x);
                    d.run("selection.set", json!({"range": gridcraft_engine::core::RangeRef::cols(c, c).a1()})).map(|_| ())
                }
                (true, false) => {
                    let r = layout.geo.row_at(y);
                    d.run("selection.set", json!({"range": gridcraft_engine::core::RangeRef::rows(r, r).a1()})).map(|_| ())
                }
                _ => Ok(()),
            }
        });
    }

    fn key(self, ev: &KeyboardEvent) {
        let key = ev.key();
        let cmd = ev.meta_key() || ev.ctrl_key();
        let shift = ev.shift_key();
        let size = self.size.get_untracked();
        let moved = |dr: i64, dc: i64| {
            self.change(|d| {
                d.run("selection.move", json!({"dr": dr, "dc": dc, "extend": shift, "jump": cmd}))?;
                d.reveal_active(size);
                Ok(())
            });
        };
        // The catalog's shortcuts (⌘B, ⌘⇧T, F9…): chords and function keys, not typing.
        let press = Press { meta: ev.meta_key(), ctrl: ev.ctrl_key(), alt: ev.alt_key(), shift, key: key.clone(), code: ev.code() };
        if cmd && shift && key.eq_ignore_ascii_case("z") {
            ev.prevent_default();
            self.act("edit.redo", json!({}));
            return;
        }
        if cmd && shift && key.eq_ignore_ascii_case("p") {
            ev.prevent_default();
            self.palette.set(true);
            return;
        }
        if let Some(id) = self.shortcuts.with_value(|t| shortcuts::lookup(t, &press, self.mac)) {
            ev.prevent_default();
            self.shortcut(id);
            return;
        }
        // Copy, cut and paste come through the clipboard events; other chords are the browser's.
        if cmd && !key.starts_with("Arrow") && !matches!(key.as_str(), "Home" | "End" | "PageUp" | "PageDown") {
            return;
        }
        // Space toggles a checkbox cell.
        if key == " " && !cmd && !shift && self.doc.with_value(Doc::space_toggles) {
            ev.prevent_default();
            self.act("cell.toggleCheckbox", json!({}));
            return;
        }
        // A character (or an input method's composition) starts an entry: the editor takes the
        // keyboard during this event, so the browser types it there.
        if (key.chars().count() == 1 && !cmd && !ev.alt_key()) || key == "Process" || ev.is_composing() {
            self.begin_edit(Some(String::new()), false);
            return;
        }
        ev.prevent_default();
        match key.as_str() {
            "ArrowUp" => moved(-1, 0),
            "ArrowDown" => moved(1, 0),
            "ArrowLeft" => moved(0, -1),
            "ArrowRight" => moved(0, 1),
            "PageUp" => moved(-self.doc.with_value(|d| d.page_rows(size)), 0),
            "PageDown" => moved(self.doc.with_value(|d| d.page_rows(size)), 0),
            "Home" => self.change(|d| {
                d.home(cmd, shift)?;
                d.reveal_active(size);
                Ok(())
            }),
            "End" if cmd => self.change(|d| {
                d.end()?;
                d.reveal_active(size);
                Ok(())
            }),
            "Enter" => self.change(|d| {
                d.advance(false, shift)?;
                d.reveal_active(size);
                Ok(())
            }),
            "Tab" => self.change(|d| {
                d.advance(true, shift)?;
                d.reveal_active(size);
                Ok(())
            }),
            "F2" => self.begin_edit(None, false),
            "Delete" | "Backspace" => match self.selected.get_untracked() {
                Some(object) => {
                    self.selected.set(None);
                    self.change(|d| d.delete_object(object));
                }
                None => self.act("edit.clearContents", json!({})),
            },
            "Escape" => {
                self.selected.set(None);
                self.filter.set(None);
                self.act("edit.clearClipboard", json!({}));
            }
            _ => {}
        }
    }
}

/// The page.
pub fn mount() {
    let params = web_sys::window().and_then(|w| w.location().search().ok()).and_then(|s| web_sys::UrlSearchParams::new_with_str(&s).ok());
    let sample = params.and_then(|p| p.get("sample")).unwrap_or_else(|| "sales".into());
    let app = App::new(Doc::sample(&sample));
    leptos::mount::mount_to_body(move || view! { <Window app=app/> });
}

#[component]
fn Window(app: App) -> impl IntoView {
    // Closing the page with unsaved changes asks first.
    window_event_listener(leptos::ev::beforeunload, move |ev: web_sys::BeforeUnloadEvent| {
        if app.doc.with_value(Doc::is_dirty) {
            ev.prevent_default();
            ev.set_return_value("This workbook has unsaved changes.");
        }
    });
    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if (ev.meta_key() || ev.ctrl_key()) && ev.shift_key() && ev.key().eq_ignore_ascii_case("p") {
            ev.prevent_default();
            app.palette.set(true);
        }
    });
    view! {
        <div class="window">
            <div class="chrome">
                <Toolbar app=app/>
                <FormulaBar app=app/>
            </div>
            <GridView app=app/>
            <Palette app=app/>
            <DialogHost app=app/>
            <ContextMenuHost app=app/>
        </div>
    }
}

/// Whether a media query holds now.
fn media(query: &str) -> bool {
    web_sys::window().and_then(|w| w.match_media(query).ok().flatten()).is_some_and(|m| m.matches())
}

/// The display scale this browser and window call for: its default text size, a touch screen,
/// the window's width.
fn environment_scale() -> f32 {
    let Some(window) = web_sys::window() else { return crate::doc::DISPLAY_SCALE };
    let root_font = window
        .document()
        .and_then(|d| d.document_element())
        .and_then(|e| window.get_computed_style(&e).ok().flatten())
        .and_then(|s| s.get_property_value("font-size").ok())
        .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
        .unwrap_or(16.0);
    let width = window.inner_width().ok().and_then(|w| w.as_f64()).unwrap_or(1280.0) as f32;
    crate::doc::adaptive_scale(root_font, media("(pointer: coarse)"), width)
}

// MARK: - Toolbar

#[component]
fn Icon(path: &'static str) -> impl IntoView {
    view! { <svg viewBox="0 0 16 16" aria-hidden="true"><path d=path/></svg> }
}

#[component]
fn Tool<F>(label: &'static str, path: &'static str, press: F, #[prop(optional, into)] on: Signal<bool>) -> impl IntoView
where
    F: Fn() + 'static,
{
    view! {
        <button class="tool" class:on=move || on.get() title=label aria-label=label aria-pressed=move || on.get().to_string() on:click=move |_| press()>
            <Icon path=path/>
        </button>
    }
}

fn flag(app: App, f: fn(&SelectionFormat) -> Option<bool>) -> Signal<bool> {
    Signal::derive(move || app.fmt.with(|s| s.as_ref().and_then(f)).unwrap_or(false))
}

fn hex(value: &str) -> String {
    value.to_string()
}

#[component]
fn Toolbar(app: App) -> impl IntoView {
    let bold = flag(app, |f| f.font.bold);
    let italic = flag(app, |f| f.font.italic);
    let underline = flag(app, |f| f.font.underline.map(|u| u != gridcraft_engine::model::Underline::None));
    let wrap = flag(app, |f| f.alignment.wrap);
    let left = flag(app, |f| f.alignment.h.map(|h| h == gridcraft_engine::model::HAlign::Left));
    let center = flag(app, |f| f.alignment.h.map(|h| h == gridcraft_engine::model::HAlign::Center));
    let right = flag(app, |f| f.alignment.h.map(|h| h == gridcraft_engine::model::HAlign::Right));
    let title = move || app.with(|d| format!("{}{}", d.title(), if d.is_dirty() { " •" } else { "" }));
    let number_code = move || app.fmt.with(|f| f.as_ref().and_then(|f| f.number.code.clone())).unwrap_or_default();
    let zoom = move || app.with(|d| d.sheet().map(|s| s.zoom).unwrap_or(100)).to_string();
    view! {
        <div class="row wrap">
            <div class="glass title">{title} <small>"GlassCell"</small></div>
            <div class="glass capsule">
                <button
                    class="tool"
                    disabled=move || app.history.with(|h| h.0.is_none())
                    title=move || app.history.with(|h| h.0.as_ref().map_or("Undo".to_string(), |l| format!("Undo {l}")))
                    aria-label="Undo"
                    on:click=move |_| app.act("edit.undo", json!({}))
                >
                    <Icon path=icons::UNDO/>
                </button>
                <button
                    class="tool"
                    disabled=move || app.history.with(|h| h.1.is_none())
                    title=move || app.history.with(|h| h.1.as_ref().map_or("Redo".to_string(), |l| format!("Redo {l}")))
                    aria-label="Redo"
                    on:click=move |_| app.act("edit.redo", json!({}))
                >
                    <Icon path=icons::REDO/>
                </button>
                <span class="vsep"></span>
                <button class="tool" class:on=move || bold.get() title="Bold" on:click=move |_| app.act("home.bold", json!({}))><b>"B"</b></button>
                <button class="tool" class:on=move || italic.get() title="Italic" on:click=move |_| app.act("home.italic", json!({}))><i>"I"</i></button>
                <button class="tool" class:on=move || underline.get() title="Underline" on:click=move |_| app.act("home.underline", json!({}))><u>"U"</u></button>
                <label class="tool swatch" title="Text Color">
                    "A"
                    <span class="bar" style="color:#e5322d"></span>
                    <input type="color" value="#e5322d" on:change=move |ev| app.act("home.fontColor", json!({"color": hex(&event_target_value(&ev))}))/>
                </label>
                <label class="tool swatch" title="Fill Color">
                    <svg viewBox="0 0 16 16"><path d="M3 4.5h10v7H3z"/></svg>
                    <span class="bar" style="color:#ffd60a"></span>
                    <input type="color" value="#ffd60a" on:change=move |ev| app.act("home.fillColor", json!({"color": hex(&event_target_value(&ev))}))/>
                </label>
                <span class="vsep"></span>
                <Tool label="Align Left" path=icons::ALIGN_LEFT on=left press=move || app.act("home.alignLeft", json!({}))/>
                <Tool label="Center" path=icons::ALIGN_CENTER on=center press=move || app.act("home.alignCenter", json!({}))/>
                <Tool label="Align Right" path=icons::ALIGN_RIGHT on=right press=move || app.act("home.alignRight", json!({}))/>
                <Tool label="Wrap Text" path=icons::WRAP on=wrap press=move || app.act("home.wrapText", json!({}))/>
                <Tool label="Merge & Center" path=icons::MERGE press=move || app.act("home.mergeCenter", json!({}))/>
                <span class="vsep"></span>
                <select class="pick" title="Number Format" prop:value=number_code on:change=move |ev| app.act("home.numberFormat", json!({"format": event_target_value(&ev)}))>
                    <option value="" disabled>"Mixed"</option>
                    <option value="General">"General"</option>
                    <option value="0.00">"Number"</option>
                    <option value="$#,##0.00">"Currency"</option>
                    <option value="0.00%">"Percent"</option>
                    <option value="m/d/yyyy">"Date"</option>
                    <option value="h:mm:ss AM/PM">"Time"</option>
                    <option value="@">"Text"</option>
                </select>
                <Tool label="Decrease Decimal" path=icons::DEC_LESS press=move || app.act("home.decreaseDecimal", json!({}))/>
                <Tool label="Increase Decimal" path=icons::DEC_MORE press=move || app.act("home.increaseDecimal", json!({}))/>
            </div>
            <div class="glass capsule">
                <Tool label="AutoSum" path=icons::SUM press=move || app.act("formulas.autoSum", json!({}))/>
                <Tool label="Sort A to Z" path=icons::SORT_UP press=move || app.act("data.sortAscending", json!({}))/>
                <Tool label="Sort Z to A" path=icons::SORT_DOWN press=move || app.act("data.sortDescending", json!({}))/>
            </div>
            <div class="spacer"></div>
            <div class="glass capsule">
                <Tool label="Open…" path=icons::OPEN press=move || open_file(app)/>
                <Tool label="Save as xlsx" path=icons::SAVE press=move || save_file(app, false)/>
                <Tool label="Search Commands" path=icons::SEARCH press=move || app.palette.set(true)/>
                <select class="pick" title="Zoom" prop:value=zoom on:change=move |ev| app.act("view.zoom", json!({"percent": event_target_value(&ev).parse::<u32>().unwrap_or(100)}))>
                    {[50, 75, 100, 125, 150, 200].map(|z| view! { <option value=z.to_string()>{format!("{z}%")}</option> })}
                </select>
            </div>
            <div class="glass capsule">
                <Tool label="Inspector" path=icons::INSPECTOR on=app.inspector press=move || app.inspector.update(|i| *i = !*i)/>
            </div>
        </div>
    }
}

// MARK: - Files

// MARK: - Formula bar

#[component]
fn FormulaBar(app: App) -> impl IntoView {
    let name = move || app.with(|d| d.name_box_text());
    let formula = move || match app.edit.get() {
        Some(e) => e.text,
        None => app.with(|d| d.entry_text()),
    };
    view! {
        <div class="row">
            <input
                class="field namebox"
                aria-label="Name Box"
                prop:value=name
                on:keydown=move |ev: KeyboardEvent| {
                    if ev.key() == "Enter" {
                        let text = event_target_value(&ev);
                        let size = app.size.get_untracked();
                        app.change(|d| {
                            d.run("edit.nameBox", json!({"text": text}))?;
                            d.reveal_active(size);
                            Ok(())
                        });
                        app.focus_grid();
                    }
                }
            />
            <span class="fx">"fx"</span>
            <input
                class="field formula"
                aria-label="Formula Bar"
                spellcheck="false"
                prop:value=formula
                on:input=move |ev| app.edit.set(Some(Edit { text: event_target_value(&ev), from_bar: true }))
                on:keydown=move |ev: KeyboardEvent| match ev.key().as_str() {
                    "Enter" => {
                        ev.prevent_default();
                        if app.edit.get_untracked().is_some() {
                            app.commit(Some((false, false)));
                        } else {
                            app.focus_grid();
                        }
                    }
                    "Escape" => app.cancel(),
                    _ => {}
                }
            />
        </div>
    }
}

// MARK: - Grid

fn paint(
    canvas: &HtmlCanvasElement,
    app: App,
    size: (f32, f32),
    editing: bool,
    guide: Option<(bool, f32)>,
    selected: Option<ObjectRef>,
    preview: Option<Preview>,
) {
    let dpr = web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0).max(1.0);
    let (w, h) = (f64::from(size.0.max(1.0)), f64::from(size.1.max(1.0)));
    let (pw, ph) = ((w * dpr) as u32, (h * dpr) as u32);
    if canvas.width() != pw || canvas.height() != ph {
        canvas.set_width(pw);
        canvas.set_height(ph);
    }
    let Some(ctx) = canvas.get_context("2d").ok().flatten().and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok()) else { return };
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    app.doc.with_value(|doc| {
        let Some(layout) = doc.layout(size) else { return };
        let panes = doc.viewports(&layout, &CanvasMeasure);
        let scene = Scene::of(doc);
        let repaint: std::rc::Rc<dyn Fn()> = std::rc::Rc::new(move || app.bump());
        render::paint(
            &ctx,
            &Frame {
                size: (w, h),
                doc,
                layout: &layout,
                panes: &panes,
                editing,
                dark: media("(prefers-color-scheme: dark)"),
                scene: &scene,
                guide,
                repaint: &repaint,
                selected,
                preview,
            },
        );
    });
}

#[component]
fn GridView(app: App) -> impl IntoView {
    let area = NodeRef::<html::Div>::new();
    let editor = app.editor;
    let drag = StoredValue::new_local(Drag::None);
    // When and on which header edge the last press was (pointer events carry no click count).
    let last_press = StoredValue::new_local((0.0_f64, None::<(bool, u32)>));
    // Presses right after an AutoFit are the rest of the double-click, not new selections.
    let quiet_until = StoredValue::new_local(0.0_f64);

    let measure = move || {
        if let Some(el) = area.get_untracked() {
            app.size.set((el.client_width() as f32, el.client_height() as f32));
        }
    };
    Effect::new(move |_| {
        if area.get().is_some() {
            measure();
        }
    });
    // The scale follows the environment: window width, text size, touch.
    let adapt = move || {
        let scale = environment_scale();
        if app.doc.with_value(|d| d.display_scale) != scale {
            app.change(|d| {
                d.display_scale = scale;
                Ok(())
            });
        }
    };
    Effect::new(move |_| {
        if area.get().is_some() {
            adapt();
        }
    });
    window_event_listener(leptos::ev::resize, move |_| {
        adapt();
        measure();
    });
    // Light and dark follow the system while the page is open.
    if let Some(query) = web_sys::window().and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten()) {
        let on_change = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || app.bump());
        let _ = query.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref());
        on_change.forget();
    }

    Effect::new(move |_| {
        let _ = app.rev.get();
        let size = app.size.get();
        let editing = app.edit.with(Option::is_some);
        let guide = app.guide.get();
        let selected = app.selected.get();
        let preview = app.preview.get();
        if let Some(canvas) = app.canvas.get() {
            paint(&canvas, app, size, editing, guide, selected, preview);
        }
    });

    // The editor sits over the active cell.
    let editor_style = move || {
        if app.edit.with(Option::is_none) {
            return "left:0;top:0;width:1px;height:1px;opacity:0;pointer-events:none".to_string();
        }
        let size = app.size.get();
        app.with(|d| {
            let (layout, sheet, sel) = (d.layout(size)?, d.sheet()?, d.selection()?);
            let range = sheet.merge_at(sel.active).unwrap_or_else(|| gridcraft_engine::core::RangeRef::cell(sel.active));
            let r = layout.geo.range_rect(sheet, range);
            let font = (11.0 * layout.scale / d.display_scale).max(9.0);
            Some(format!("left:{}px;top:{}px;min-width:{}px;height:{}px;font-size:{font}px", r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0))
        })
        .unwrap_or_default()
    };
    // An entry started from the grid (F2, double-click) puts the caret after its text.
    Effect::new(move |started: Option<bool>| {
        let now = app.edit.with(|e| e.as_ref().is_some_and(|e| !e.from_bar));
        if now
            && started != Some(true)
            && let Some(input) = editor.get()
        {
            let _ = input.focus();
            let n = input.value().chars().count() as u32;
            let _ = input.set_selection_range(n, n);
        }
        now
    });

    view! {
        <div class="grid-area" node_ref=area>
            <canvas
                class="grid"
                node_ref=app.canvas
                tabindex="0"
                aria-label=move || format!("Spreadsheet grid. Selected: {}", app.with(|d| d.selection().map(|s| s.a1()).unwrap_or_default()))
                on:pointerdown=move |ev: PointerEvent| {
                    if ev.button() != 0 || js_sys::Date::now() < quiet_until.get_value() {
                        return;
                    }
                    app.filter.set(None);
                    if app.edit.get_untracked().is_some() {
                        app.commit(None);
                    }
                    app.focus_grid();
                    if let Some(c) = app.canvas.get_untracked() {
                        let _ = c.set_pointer_capture(ev.pointer_id());
                    }
                    let (x, y) = (ev.offset_x() as f32, ev.offset_y() as f32);
                    let size = app.size.get_untracked();
                    // A header edge: drag to resize, double-click to AutoFit.
                    let edge = app.doc.with_value(|d| d.layout(size).and_then(|l| d.resize_edge(&l, x, y)));
                    if let Some(edge) = edge {
                        let now = js_sys::Date::now();
                        let (then, was) = last_press.get_value();
                        let twice = was == Some((edge.columns, edge.index)) && now - then < 500.0;
                        last_press.set_value((now, Some((edge.columns, edge.index))));
                        if twice {
                            last_press.set_value((0.0, None));
                            quiet_until.set_value(now + 400.0);
                            app.change(|d| d.autofit(edge));
                        } else {
                            drag.set_value(Drag::Resize { edge, start: if edge.columns { x } else { y }, original: edge.size });
                        }
                        return;
                    }
                    // ⌘-click (Ctrl elsewhere) adds an area to the selection.
                    let additive = if app.mac { ev.meta_key() } else { ev.ctrl_key() };
                    if additive && let Some(cell) = app.doc.with_value(|d| d.layout(size).and_then(|l| d.cell_at(&l, x, y))) {
                        app.selected.set(None);
                        let keep = app.doc.with_value(|d| d.selection().map_or(0, |s| s.ranges.len()));
                        drag.set_value(Drag::AddArea { anchor: cell, keep });
                        app.change(|d| d.select_area(cell, cell, keep));
                        return;
                    }
                    // Over the cells: an object, the fill handle, or a cell.
                    let hit = app.doc.with_value(|d| d.layout(size).and_then(|l| d.hit(&l, x, y)));
                    match hit {
                        Some(Hit::Object { id, rect, .. }) => {
                            let object = app.doc.with_value(|d| d.objects().into_iter().find(|o| o.id == id).map(|o| ObjectRef { kind: kind_name(&o.kind), id }));
                            if let Some(object) = object {
                                // From the corner of the selected object, a drag resizes it.
                                let corner = ((x - rect.right()).powi(2) + (y - rect.bottom()).powi(2)).sqrt() < 10.0;
                                let resizing = corner && app.selected.get_untracked() == Some(object);
                                drag.set_value(if resizing { Drag::ResizeObject { object, start: (x, y) } } else { Drag::MoveObject { object, start: (x, y) } });
                                app.selected.set(Some(object));
                            }
                            return;
                        }
                        Some(Hit::FilterButton { cell }) => {
                            app.selected.set(None);
                            app.select_at(x, y, false);
                            app.change(|d| d.select(cell, false));
                            let open = app.doc.with_value(|d| {
                                let layout = d.layout(size)?;
                                Some(OpenFilter { cell, anchor: d.filter_button_rect(&layout, cell)?, menu: d.filter_menu(cell)? })
                            });
                            app.filter.set(open);
                            return;
                        }
                        Some(Hit::SelectionBorder) => {
                            app.selected.set(None);
                            let grab = app.doc.with_value(|d| d.layout(size).map(|l| l.geo.cell_at(x, y)));
                            let source = app.doc.with_value(|d| d.selection().map(|s| s.current()));
                            if let (Some(grab), Some(source)) = (grab, source) {
                                drag.set_value(Drag::MoveCells { source, grab });
                            }
                            return;
                        }
                        Some(Hit::FillHandle) => {
                            app.selected.set(None);
                            let now = js_sys::Date::now();
                            let (then, was) = last_press.get_value();
                            last_press.set_value((now, Some((false, u32::MAX))));
                            if was == Some((false, u32::MAX)) && now - then < 500.0 {
                                last_press.set_value((0.0, None));
                                quiet_until.set_value(now + 400.0);
                                app.change(|d| d.fill_down());
                            } else if let Some(source) = app.doc.with_value(|d| d.selection().map(|s| s.current())) {
                                drag.set_value(Drag::Fill { source });
                            }
                            return;
                        }
                        _ => {}
                    }
                    app.selected.set(None);
                    drag.set_value(Drag::Select);
                    app.select_at(x, y, ev.shift_key());
                }
                on:pointermove=move |ev: PointerEvent| {
                    let (x, y) = (ev.offset_x() as f32, ev.offset_y() as f32);
                    let size = app.size.get_untracked();
                    // A drag past the edge of the cells scrolls the sheet along.
                    let autoscroll = |x: f32, y: f32| {
                        let header = app.doc.with_value(|d| d.layout(size).map(|l| (l.header_w, l.header_h))).unwrap_or((0.0, 0.0));
                        let step = |at: f32, low: f32, high: f32| if at > high - 2.0 { 20.0 } else if at < low { -20.0 } else { 0.0 };
                        let (dx, dy) = (step(x, header.0, size.0), step(y, header.1, size.1));
                        if dx != 0.0 || dy != 0.0 {
                            app.change(|d| {
                                d.scroll_by(size, dx, dy);
                                Ok(())
                            });
                        }
                    };
                    match drag.get_value() {
                        Drag::Select => {
                            autoscroll(x, y);
                            app.select_at(x, y, true);
                        }
                        Drag::AddArea { anchor, keep } => {
                            autoscroll(x, y);
                            let cell = app.doc.with_value(|d| d.layout(size).map(|l| l.geo.cell_at(x.max(l.header_w + 1.0), y.max(l.header_h + 1.0))));
                            if let Some(cell) = cell {
                                app.change(|d| d.select_area(anchor, cell, keep));
                            }
                        }
                        Drag::Resize { edge, start, original } => {
                            let scale = app.doc.with_value(|d| d.layout(size).map(|l| l.scale)).unwrap_or(1.0);
                            let at = if edge.columns { x } else { y };
                            let new_size = (original + (at - start) / scale).max(0.0);
                            app.guide.set(Some((edge.columns, at)));
                            app.show(Some(gridcraft_engine::geometry::resize_label(edge.columns, new_size)));
                        }
                        Drag::MoveObject { object, start } => {
                            let rect = app.doc.with_value(|d| d.layout(size).and_then(|l| d.object(object).map(|o| l.geo.anchor_rect(&o.anchor))));
                            if let Some(mut r) = rect {
                                (r.x, r.y) = (r.x + x - start.0, r.y + y - start.1);
                                app.preview.set(Some(Preview::Rect(r)));
                            }
                        }
                        Drag::ResizeObject { object, start } => {
                            let rect = app.doc.with_value(|d| d.layout(size).and_then(|l| d.object(object).map(|o| l.geo.anchor_rect(&o.anchor))));
                            if let Some(mut r) = rect {
                                (r.w, r.h) = ((r.w + x - start.0).max(20.0), (r.h + y - start.1).max(20.0));
                                app.preview.set(Some(Preview::Rect(r)));
                            }
                        }
                        Drag::MoveCells { source, grab } => {
                            let target = app.doc.with_value(|d| {
                                let l = d.layout(size)?;
                                let cell = l.geo.cell_at(x.max(l.header_w + 1.0), y.max(l.header_h + 1.0));
                                Some(crate::doc::moved_range(source, i64::from(cell.row) - i64::from(grab.row), i64::from(cell.col) - i64::from(grab.col)))
                            });
                            app.preview.set(target.map(Preview::Range));
                        }
                        Drag::Fill { source } => {
                            let target = app.doc.with_value(|d| {
                                let l = d.layout(size)?;
                                let cell = l.geo.cell_at(x.max(l.header_w + 1.0), y.max(l.header_h + 1.0));
                                Some(gridcraft_engine::geometry::fill_target(source, cell))
                            });
                            app.preview.set(target.map(Preview::Range));
                        }
                        Drag::None => {
                            // The pointer says what a press here would do.
                            let edge = app.doc.with_value(|d| d.layout(size).and_then(|l| d.resize_edge(&l, x, y)));
                            if let Some(c) = app.canvas.get_untracked() {
                                let over = if edge.is_none() { app.doc.with_value(|d| d.layout(size).and_then(|l| d.hit(&l, x, y))) } else { None };
                                let cursor = match (edge, over) {
                                    (Some(e), _) if e.columns => "col-resize",
                                    (Some(_), _) => "row-resize",
                                    (None, Some(Hit::FillHandle)) => "crosshair",
                                    (None, Some(Hit::SelectionBorder)) => "grab",
                                    (None, Some(Hit::FilterButton { .. })) => "pointer",
                                    (None, Some(Hit::Object { .. })) => "move",
                                    _ => "default",
                                };
                                let el: &web_sys::HtmlElement = c.as_ref();
                                let _ = el.style().set_property("cursor", cursor);
                            }
                        }
                    }
                }
                on:pointerup=move |ev: PointerEvent| {
                    let (x, y) = (ev.offset_x() as f32, ev.offset_y() as f32);
                    let size = app.size.get_untracked();
                    let scale = app.doc.with_value(|d| d.layout(size).map(|l| l.scale)).unwrap_or(1.0);
                    match drag.get_value() {
                        Drag::MoveObject { object, start } => {
                            app.preview.set(None);
                            if (x - start.0).abs() >= 1.0 || (y - start.1).abs() >= 1.0 {
                                app.change(|d| d.move_object(object, x - start.0, y - start.1, scale));
                            }
                        }
                        Drag::ResizeObject { object, start } => {
                            app.preview.set(None);
                            let current = app.doc.with_value(|d| d.object(object).map(|o| (o.anchor.width, o.anchor.height)));
                            if let Some((w, h)) = current
                                && ((x - start.0).abs() >= 1.0 || (y - start.1).abs() >= 1.0)
                            {
                                app.change(|d| d.resize_object(object, w + (x - start.0) / scale, h + (y - start.1) / scale));
                            }
                        }
                        Drag::MoveCells { source, .. } => {
                            let target = app.preview.get_untracked();
                            app.preview.set(None);
                            if let Some(Preview::Range(target)) = target {
                                app.change(|d| d.move_cells(source, target.start));
                            }
                        }
                        Drag::Fill { source } => {
                            let target = app.preview.get_untracked();
                            app.preview.set(None);
                            if let Some(Preview::Range(target)) = target {
                                // ⌘ swaps copying and extending a series, as in Excel.
                                let alternate = ev.meta_key() || ev.ctrl_key();
                                app.change(|d| d.fill(source, target, alternate));
                            }
                        }
                        _ => {}
                    }
                    if let Drag::Resize { edge, start, original } = drag.get_value() {
                        let size = app.size.get_untracked();
                        let scale = app.doc.with_value(|d| d.layout(size).map(|l| l.scale)).unwrap_or(1.0);
                        let at = if edge.columns { ev.offset_x() as f32 } else { ev.offset_y() as f32 };
                        let new_size = (original + (at - start) / scale).max(0.0);
                        app.guide.set(None);
                        // A click on the edge without moving changes nothing.
                        if (at - start).abs() >= 1.0 {
                            app.change(|d| d.resize(edge, new_size));
                        } else {
                            app.show(None);
                        }
                    }
                    drag.set_value(Drag::None);
                    if let Some(c) = app.canvas.get_untracked() {
                        let _ = c.release_pointer_capture(ev.pointer_id());
                    }
                }
                on:dblclick=move |ev: web_sys::MouseEvent| {
                    // Only a double-click on a cell edits it (on a header edge it AutoFits).
                    let size = app.size.get_untracked();
                    let on_cell = app.doc.with_value(|d| d.layout(size).and_then(|l| d.cell_at(&l, ev.offset_x() as f32, ev.offset_y() as f32)).is_some());
                    if on_cell {
                        app.begin_edit(None, false);
                    }
                }
                on:contextmenu=move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    let (x, y) = (ev.offset_x() as f32, ev.offset_y() as f32);
                    let size = app.size.get_untracked();
                    // What it is on, and whether the press selects it first (outside the selection).
                    let target = app.doc.with_value(|d| {
                        let (l, sel) = (d.layout(size)?, d.selection()?);
                        if y < l.header_h && x >= l.header_w {
                            let c = l.geo.col_at(x);
                            Some((Target::Columns, !sel.ranges.iter().any(|r| r.is_full_cols() && c >= r.start.col && c <= r.end.col)))
                        } else if x < l.header_w && y >= l.header_h {
                            let r = l.geo.row_at(y);
                            Some((Target::Rows, !sel.ranges.iter().any(|a| a.is_full_rows() && r >= a.start.row && r <= a.end.row)))
                        } else if x >= l.header_w && y >= l.header_h {
                            Some((Target::Cells, !sel.contains(l.geo.cell_at(x, y))))
                        } else {
                            None
                        }
                    });
                    let Some((target, select)) = target else { return };
                    if select {
                        app.select_at(x, y, false);
                    }
                    app.open_menu(target, ev.client_x() as f32, ev.client_y() as f32);
                }
                on:wheel=move |ev: WheelEvent| {
                    ev.prevent_default();
                    let (mut dx, mut dy) = (ev.delta_x() as f32, ev.delta_y() as f32);
                    if ev.shift_key() && dx == 0.0 {
                        (dx, dy) = (dy, 0.0);
                    }
                    let size = app.size.get_untracked();
                    app.change(|d| {
                        d.scroll_by(size, dx, dy);
                        Ok(())
                    });
                }
                on:keydown=move |ev: KeyboardEvent| app.key(&ev)
                on:copy=move |ev: web_sys::ClipboardEvent| clipboard(app, &ev, false)
                on:cut=move |ev: web_sys::ClipboardEvent| clipboard(app, &ev, true)
                on:paste=move |ev: web_sys::ClipboardEvent| {
                    ev.prevent_default();
                    let text = ev.clipboard_data().and_then(|d| d.get_data("text/plain").ok());
                    app.change(|d| d.paste(text.as_deref()));
                }
            ></canvas>
            <input
                class="cell-editor"
                node_ref=editor
                style=editor_style
                spellcheck="false"
                aria-label="Cell editor"
                prop:value=move || app.edit.with(|e| e.as_ref().map(|e| e.text.clone()).unwrap_or_default())
                on:input=move |ev| app.edit.set(Some(Edit { text: event_target_value(&ev), from_bar: false }))
                    on:keydown=move |ev: KeyboardEvent| match ev.key().as_str() {
                        "Enter" => {
                            ev.prevent_default();
                            app.commit(Some((false, ev.shift_key())));
                        }
                        "Tab" => {
                            ev.prevent_default();
                            app.commit(Some((true, ev.shift_key())));
                        }
                        "Escape" => app.cancel(),
                        _ => {}
                    }
                    on:blur=move |_| {
                        if app.edit.get_untracked().is_some_and(|e| !e.from_bar) {
                            app.commit(None);
                        }
                    }
                />
            <StatusBar app=app/>
            <Inspector app=app/>
            {move || app.filter.get().map(|open| view! { <FilterPanel app=app open=open/> })}
        </div>
    }
}

fn clipboard(app: App, ev: &web_sys::ClipboardEvent, cut: bool) {
    ev.prevent_default();
    let text = app.doc.try_update_value(|d| d.copy(cut));
    match text {
        Some(Ok(text)) => {
            if let Some(data) = ev.clipboard_data() {
                let _ = data.set_data("text/plain", &text);
            }
            app.show(None);
        }
        Some(Err(e)) => app.show(Some(e)),
        None => {}
    }
    app.bump();
}

// MARK: - Status bar

#[component]
fn StatusBar(app: App) -> impl IntoView {
    let tabs = move || app.with(|d| d.sheet_tabs());
    let stats = move || app.with(|d| d.status_items());
    view! {
        <div class="status">
            <div class="glass tabs">
                {move || {
                    tabs()
                        .into_iter()
                        .enumerate()
                        .map(|(i, (name, on))| view! {
                            <button
                                class:on=on
                                on:click=move |_| app.act("sheet.activate", json!({"sheet": i}))
                                on:dblclick=move |_| app.run_action(menus::Action::RenameSheet(i))
                                on:contextmenu=move |ev: web_sys::MouseEvent| {
                                    ev.prevent_default();
                                    app.open_menu(Target::Tab(i), ev.client_x() as f32, ev.client_y() as f32);
                                }
                            >
                                {name}
                            </button>
                        })
                        .collect_view()
                }}
                <button title="Insert Sheet" aria-label="Insert Sheet" on:click=move |_| app.act("home.insertSheet", json!({}))>"+"</button>
            </div>
            <div class="spacer"></div>
            <Show when=move || app.message.with(Option::is_some)>
                <div class="glass toast" role="alert">{move || app.message.get().unwrap_or_default()}</div>
            </Show>
            <Show when=move || !stats().is_empty()>
                <div class="glass stats">
                    {move || stats().into_iter().map(|(k, v)| view! { <span>{k}" "<b>{v}</b></span> }).collect_view()}
                </div>
            </Show>
        </div>
    }
}

// MARK: - Filter dropdown

/// A header's filter dropdown: sort the column, or choose the values it shows. It floats over
/// the grid on glass, under its button (above it when there is no room below).
#[component]
fn FilterPanel(app: App, open: OpenFilter) -> impl IntoView {
    const WIDTH: f32 = 260.0;
    const HEIGHT: f32 = 380.0;
    let column = open.cell.col;
    let anchor = open.anchor;
    let menu = StoredValue::new_local(open.menu.clone());
    let shown = RwSignal::new_local(open.menu.values.clone());
    let search = RwSignal::new(String::new());
    let input = NodeRef::<html::Input>::new();
    Effect::new(move |_| {
        if let Some(i) = input.get() {
            let _ = i.focus();
        }
    });

    let matches = move || {
        let q = search.get().trim().to_lowercase();
        shown.with(|v| v.iter().filter(|(t, _)| q.is_empty() || t.to_lowercase().contains(&q)).cloned().collect::<Vec<_>>())
    };
    // What Apply shows: the checked values the search matches.
    let applied = move || matches().into_iter().filter(|(_, on)| *on).map(|(t, _)| t).collect::<Vec<String>>();
    let all_on = move || {
        let m = matches();
        !m.is_empty() && m.iter().all(|(_, on)| *on)
    };
    let close = move || {
        app.filter.set(None);
        app.focus_grid();
    };
    let apply = move |showing: Vec<String>| {
        if showing.is_empty() {
            return;
        }
        close();
        menu.with_value(|m| app.change(|d| d.apply_filter(m, column, &showing)));
    };
    let style = move || {
        let size = app.size.get();
        let left = anchor.x.min(size.0 - WIDTH - 8.0).max(8.0);
        let top = if anchor.bottom() + 4.0 + HEIGHT > size.1 { (anchor.y - HEIGHT - 4.0).max(8.0) } else { anchor.bottom() + 4.0 };
        format!("left:{left}px;top:{top}px;width:{WIDTH}px")
    };
    view! {
        <div class="filter-scrim" on:pointerdown=move |_| close()></div>
        <div
            class="glass filter"
            style=style
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" {
                    close();
                }
            }
        >
            <button class="menu-item" on:click=move |_| {
                close();
                app.change(|d| d.sort_column(column, true));
            }>"Sort Ascending"</button>
            <button class="menu-item" on:click=move |_| {
                close();
                app.change(|d| d.sort_column(column, false));
            }>"Sort Descending"</button>
            <div class="vline"></div>
            <input
                class="field"
                node_ref=input
                placeholder="Search"
                prop:value=move || search.get()
                on:input=move |ev| search.set(event_target_value(&ev))
                on:keydown=move |ev: KeyboardEvent| {
                    if ev.key() == "Enter" && !ev.is_composing() {
                        apply(applied());
                    }
                }
            />
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=all_on
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        let names: Vec<String> = matches().into_iter().map(|(t, _)| t).collect();
                        shown.update(|v| v.iter_mut().filter(|e| names.contains(&e.0)).for_each(|e| e.1 = on));
                    }
                />
                {move || if search.with(|s| s.trim().is_empty()) { "(Select All)" } else { "(Select All Search Results)" }}
            </label>
            <div class="filter-list">
                {move || {
                    matches()
                        .into_iter()
                        .map(|(text, on)| {
                            let name = text.clone();
                            view! {
                                <label class="check">
                                    <input
                                        type="checkbox"
                                        prop:checked=on
                                        on:change=move |ev| {
                                            let on = event_target_checked(&ev);
                                            shown.update(|v| v.iter_mut().filter(|e| e.0 == name).for_each(|e| e.1 = on));
                                        }
                                    />
                                    <span>{text}</span>
                                </label>
                            }
                        })
                        .collect_view()
                }}
            </div>
            <div class="row">
                <button class="btn" on:click=move |_| apply(menu.with_value(|m| m.values.iter().map(|(t, _)| t.clone()).collect()))>"Clear Filter"</button>
                <div class="spacer"></div>
                <button class="btn primary" disabled=move || applied().is_empty() on:click=move |_| apply(applied())>"Apply"</button>
            </div>
        </div>
    }
}

// MARK: - Inspector

#[component]
fn Inspector(app: App) -> impl IntoView {
    let bold = flag(app, |f| f.font.bold);
    let italic = flag(app, |f| f.font.italic);
    let wrap = flag(app, |f| f.alignment.wrap);
    let font_name = move || app.fmt.with(|f| f.as_ref().and_then(|f| f.font.name.clone())).unwrap_or_default();
    let font_size = move || app.fmt.with(|f| f.as_ref().and_then(|f| f.font.size)).map(|s| s.to_string()).unwrap_or_default();
    let gridlines = move || app.with(|d| d.sheet().is_some_and(|s| s.show_gridlines));
    let headings = move || app.with(|d| d.sheet().is_some_and(|s| s.show_headings));
    let cells = move || app.fmt.with(|f| f.as_ref().map(|f| f.cells)).unwrap_or(0);
    view! {
        <Show when=move || app.inspector.get()>
            <aside class="glass inspector" aria-label="Inspector">
                <div class="group">
                    <h3>"Cell"</h3>
                    <div class="line"><label>"Selected"</label><span>{move || format!("{} cell(s)", cells())}</span></div>
                </div>
                <div class="group">
                    <h3>"Font"</h3>
                    <div class="line">
                        <label>"Family"</label>
                        <input class="field" prop:value=font_name on:change=move |ev| app.act("home.fontName", json!({"name": event_target_value(&ev)}))/>
                    </div>
                    <div class="line">
                        <label>"Size"</label>
                        <input class="field" type="number" min="1" max="409" prop:value=font_size on:change=move |ev| {
                            if let Ok(size) = event_target_value(&ev).parse::<f64>() {
                                app.act("home.fontSize", json!({"size": size}));
                            }
                        }/>
                    </div>
                    <div class="line">
                        <label>"Style"</label>
                        <div class="seg">
                            <button class="tool" class:on=move || bold.get() on:click=move |_| app.act("home.bold", json!({}))><b>"B"</b></button>
                            <button class="tool" class:on=move || italic.get() on:click=move |_| app.act("home.italic", json!({}))><i>"I"</i></button>
                            <button class="tool" on:click=move |_| app.act("home.underline", json!({}))><u>"U"</u></button>
                        </div>
                    </div>
                </div>
                <div class="group">
                    <h3>"Alignment"</h3>
                    <div class="line">
                        <label>"Horizontal"</label>
                        <div class="seg">
                            <Tool label="Align Left" path=icons::ALIGN_LEFT press=move || app.act("home.alignLeft", json!({}))/>
                            <Tool label="Center" path=icons::ALIGN_CENTER press=move || app.act("home.alignCenter", json!({}))/>
                            <Tool label="Align Right" path=icons::ALIGN_RIGHT press=move || app.act("home.alignRight", json!({}))/>
                        </div>
                    </div>
                    <div class="line">
                        <label>"Vertical"</label>
                        <div class="seg">
                            <button class="tool" on:click=move |_| app.act("home.alignTop", json!({}))>"Top"</button>
                            <button class="tool" on:click=move |_| app.act("home.alignMiddle", json!({}))>"Middle"</button>
                            <button class="tool" on:click=move |_| app.act("home.alignBottom", json!({}))>"Bottom"</button>
                        </div>
                    </div>
                    <label class="check"><input type="checkbox" prop:checked=move || wrap.get() on:change=move |_| app.act("home.wrapText", json!({}))/>"Wrap text"</label>
                </div>
                <div class="group">
                    <h3>"Sheet"</h3>
                    <label class="check"><input type="checkbox" prop:checked=gridlines on:change=move |_| app.act("view.gridlines", json!({}))/>"Gridlines"</label>
                    <label class="check"><input type="checkbox" prop:checked=headings on:change=move |_| app.act("view.headings", json!({}))/>"Headings"</label>
                    <div class="line">
                        <label>"Freeze"</label>
                        <div class="seg">
                            <button class="tool" on:click=move |_| app.act("view.freezeTopRow", json!({}))>"Top row"</button>
                            <button class="tool" on:click=move |_| app.act("view.freezeFirstColumn", json!({}))>"First col"</button>
                            <button class="tool" on:click=move |_| app.act("view.unfreezePanes", json!({}))>"None"</button>
                        </div>
                    </div>
                </div>
            </aside>
        </Show>
    }
}

// MARK: - Command palette

#[component]
fn Palette(app: App) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let chosen = RwSignal::new(0usize);
    let input = NodeRef::<html::Input>::new();
    let results = move || -> Vec<CommandInfo> {
        let q = query.get().trim().to_lowercase();
        app.with(|d| {
            // Best first: the label starts with it, contains it, the menu path does, then the id.
            let mut scored: Vec<(u8, CommandInfo)> = d
                .session
                .commands()
                .into_iter()
                .filter(|c| c.enabled)
                .filter_map(|c| {
                    let label = c.label.to_lowercase();
                    let score = if q.is_empty() || label.starts_with(&q) {
                        0
                    } else if label.contains(&q) {
                        1
                    } else if c.menu.join(" ").to_lowercase().contains(&q) {
                        2
                    } else if c.id.to_lowercase().contains(&q) {
                        3
                    } else {
                        return None;
                    };
                    Some((score, c))
                })
                .collect();
            scored.sort_by_key(|(score, _)| *score);
            scored.into_iter().take(40).map(|(_, c)| c).collect()
        })
    };
    let close = move || {
        app.palette.set(false);
        query.set(String::new());
        chosen.set(0);
        app.focus_grid();
    };
    let run = move |id: &'static str| {
        close();
        app.act(id, json!({}));
    };
    Effect::new(move |_| {
        if app.palette.get()
            && let Some(i) = input.get()
        {
            let _ = i.focus();
        }
    });
    view! {
        <Show when=move || app.palette.get()>
            <div class="palette-scrim" on:click=move |_| close()>
                <div class="glass palette" on:click=move |ev| ev.stop_propagation()>
                    <input
                        class="field"
                        node_ref=input
                        placeholder="Search commands"
                        prop:value=move || query.get()
                        on:input=move |ev| {
                            query.set(event_target_value(&ev));
                            chosen.set(0);
                        }
                        on:keydown=move |ev: KeyboardEvent| match ev.key().as_str() {
                            "Escape" => close(),
                            "ArrowDown" => {
                                ev.prevent_default();
                                chosen.update(|c| *c = (*c + 1).min(results().len().saturating_sub(1)));
                            }
                            "ArrowUp" => {
                                ev.prevent_default();
                                chosen.update(|c| *c = c.saturating_sub(1));
                            }
                            "Enter" if !ev.is_composing() => {
                                // Enter must not go on to whatever takes the keyboard next: a
                                // dialog this opens would submit itself.
                                ev.prevent_default();
                                if let Some(c) = results().get(chosen.get_untracked()) {
                                    run(c.id);
                                }
                            }
                            _ => {}
                        }
                    />
                    <ul>
                        {move || {
                            results()
                                .into_iter()
                                .enumerate()
                                .map(|(i, c)| {
                                    let id = c.id;
                                    view! {
                                        <li class:on=move || chosen.get() == i on:click=move |_| run(id)>
                                            <span>{c.label}</span>
                                            <small>{c.menu.join(" ▸ ")}</small>
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ul>
                </div>
            </div>
        </Show>
    }
}
