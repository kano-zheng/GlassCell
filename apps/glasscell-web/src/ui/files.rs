//! Opening and saving files. Where the browser has the File System Access API (Chrome, Edge) a
//! workbook is opened and saved in place, remembering its file; elsewhere Open uses the file
//! picker and Save downloads an xlsx.

use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use super::*;

const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// `window.<name>`, when it is a function.
fn window_fn(name: &str) -> Option<Function> {
    let w = web_sys::window()?;
    Reflect::get(&w, &name.into()).ok()?.dyn_into::<Function>().ok()
}

/// Calls `obj.method(...args)` and waits for what it gives back.
pub(super) async fn invoke(obj: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f = Reflect::get(obj, &method.into())?.dyn_into::<Function>().map_err(|_| JsValue::from_str("not a function"))?;
    let list = Array::new();
    args.iter().for_each(|a| {
        list.push(a);
    });
    let result = Reflect::apply(&f, obj, &list)?;
    match result.dyn_into::<Promise>() {
        Ok(promise) => JsFuture::from(promise).await,
        Err(value) => Ok(value),
    }
}

/// Calls a picker (`showOpenFilePicker`, `showSaveFilePicker`) with `options`.
async fn picker(function: &Function, options: &JsValue) -> Result<JsValue, JsValue> {
    let w = web_sys::window().map(JsValue::from).unwrap_or(JsValue::NULL);
    let result = function.call1(&w, options)?;
    JsFuture::from(result.dyn_into::<Promise>()?).await
}

fn set(obj: &Object, key: &str, value: &JsValue) {
    let _ = Reflect::set(obj, &key.into(), value);
}

/// The file types the pickers offer.
fn types(save: bool) -> JsValue {
    let accept = Object::new();
    if save {
        set(&accept, XLSX, &Array::of1(&".xlsx".into()));
    } else {
        set(&accept, XLSX, &Array::of2(&".xlsx".into(), &".xlsm".into()));
        set(&accept, "text/csv", &Array::of1(&".csv".into()));
        set(&accept, "text/tab-separated-values", &Array::of1(&".tsv".into()));
        set(&accept, "application/vnd.oasis.opendocument.spreadsheet", &Array::of1(&".ods".into()));
        set(&accept, "application/vnd.ms-excel", &Array::of1(&".xls".into()));
    }
    let entry = Object::new();
    set(&entry, "description", &(if save { "Excel workbook" } else { "Spreadsheets" }).into());
    set(&entry, "accept", &accept);
    Array::of1(&entry).into()
}

/// The user closed the picker.
fn cancelled(e: &JsValue) -> bool {
    Reflect::get(e, &"name".into()).ok().and_then(|n| n.as_string()).as_deref() == Some("AbortError")
}

fn describe(e: &JsValue) -> String {
    Reflect::get(e, &"message".into()).ok().and_then(|m| m.as_string()).or_else(|| e.as_string()).unwrap_or_else(|| "The browser refused.".into())
}

pub(super) fn open_file(app: App) {
    let Some(pick) = window_fn("showOpenFilePicker") else { return open_with_input(app) };
    wasm_bindgen_futures::spawn_local(async move {
        let options = Object::new();
        set(&options, "types", &types(false));
        set(&options, "multiple", &false.into());
        let handle = match picker(&pick, &options).await {
            Ok(handles) => handles.dyn_into::<Array>().ok().map(|a| a.get(0)),
            Err(e) if cancelled(&e) => return,
            Err(e) => return app.show(Some(describe(&e))),
        };
        let Some(handle) = handle else { return };
        let read = async {
            let file = invoke(&handle, "getFile", &[]).await?;
            let name = Reflect::get(&file, &"name".into())?.as_string().unwrap_or_else(|| "Book.xlsx".into());
            let buffer = invoke(&file, "arrayBuffer", &[]).await?;
            Ok::<_, JsValue>((name, Uint8Array::new(&buffer).to_vec()))
        };
        match read.await {
            Ok((name, bytes)) => {
                app.change(|d| d.open_bytes(&name, &bytes).map(|_| ()));
                // Saving goes back to this file, when it is one Save can write (xlsx).
                app.file.set_value(name.to_lowercase().ends_with(".xlsx").then_some(handle));
            }
            Err(e) => app.show(Some(describe(&e))),
        }
    });
}

/// Open where there is no file picker API: the browser's file dialog.
fn open_with_input(app: App) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    let Some(input) = document.create_element("input").ok().and_then(|e| e.dyn_into::<HtmlInputElement>().ok()) else { return };
    input.set_type("file");
    input.set_accept(".xlsx,.xlsm,.xls,.ods,.csv,.tsv,.txt,.json");
    let picked = input.clone();
    let on_change = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
        let Some(file) = picked.files().and_then(|f| f.get(0)) else { return };
        let name = file.name();
        let reader = file.array_buffer();
        wasm_bindgen_futures::spawn_local(async move {
            match JsFuture::from(reader).await.ok().and_then(|v| v.dyn_into::<js_sys::ArrayBuffer>().ok()) {
                Some(buffer) => {
                    let bytes = Uint8Array::new(&buffer).to_vec();
                    app.change(|d| d.open_bytes(&name, &bytes).map(|_| ()));
                    app.file.set_value(None);
                }
                None => app.show(Some("Couldn't read that file.".into())),
            }
        });
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    on_change.forget();
    input.click();
}

/// Writes the workbook: to its file when it has one, to a file the user picks (Save As, or the
/// first save), or as a download where the browser can't write files.
pub(super) fn save_file(app: App, as_new: bool) {
    wasm_bindgen_futures::spawn_local(async move {
        let encoded = app.doc.try_update_value(Doc::encode_xlsx);
        let (name, bytes) = match encoded {
            Some(Ok(v)) => v,
            Some(Err(e)) => return app.show(Some(e)),
            None => return,
        };
        let existing = if as_new { None } else { app.file.get_value() };
        let handle = match (existing, window_fn("showSaveFilePicker")) {
            (Some(h), _) => Some(h),
            (None, Some(pick)) => {
                let options = Object::new();
                set(&options, "suggestedName", &name.as_str().into());
                set(&options, "types", &types(true));
                match picker(&pick, &options).await {
                    Ok(h) => Some(h),
                    Err(e) if cancelled(&e) => return,
                    Err(e) => return app.show(Some(describe(&e))),
                }
            }
            (None, None) => None,
        };
        let Some(handle) = handle else {
            download(&name, &bytes);
            app.change(|d| {
                d.mark_saved(None);
                Ok(())
            });
            return;
        };
        let write = async {
            let writable = invoke(&handle, "createWritable", &[]).await?;
            invoke(&writable, "write", &[Uint8Array::from(bytes.as_slice()).into()]).await?;
            invoke(&writable, "close", &[]).await
        };
        match write.await {
            Ok(_) => {
                let saved_as = Reflect::get(&handle, &"name".into()).ok().and_then(|n| n.as_string());
                app.file.set_value(Some(handle));
                app.change(|d| {
                    d.mark_saved(saved_as.as_deref());
                    Ok(())
                });
            }
            Err(e) => {
                // A handle that no longer works (moved, permission gone): the next save asks again.
                app.file.set_value(None);
                app.show(Some(format!("Couldn't save: {}", describe(&e))));
            }
        }
    });
}

/// Hands the bytes to the browser as a download.
fn download(name: &str, bytes: &[u8]) {
    let parts = Array::of1(&Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(XLSX);
    let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options) else { return };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
    if let Some(a) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.create_element("a").ok())
        .and_then(|e| e.dyn_into::<web_sys::HtmlAnchorElement>().ok())
    {
        a.set_href(&url);
        a.set_download(name);
        a.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// `?open=/book.xlsx`: a workbook on the page's own server, opened at start (demos, embedding).
/// Only paths on this origin are fetched.
pub(super) fn open_from_url(app: App, path: String) {
    if !path.starts_with('/') || path.starts_with("//") {
        return app.show(Some("Only a path on this server can be opened.".into()));
    }
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else { return };
        let fetched = async {
            let response = invoke(&window.into(), "fetch", &[path.as_str().into()]).await?;
            if Reflect::get(&response, &"ok".into())?.as_bool() != Some(true) {
                return Err(JsValue::from_str("The file wasn't found."));
            }
            let buffer = invoke(&response, "arrayBuffer", &[]).await?;
            Ok::<_, JsValue>(Uint8Array::new(&buffer).to_vec())
        };
        match fetched.await {
            Ok(bytes) => {
                let name = path.rsplit('/').next().unwrap_or("Book.xlsx").to_string();
                app.change(|d| d.open_bytes(&name, &bytes).map(|_| ()));
            }
            Err(e) => app.show(Some(describe(&e))),
        }
    });
}
