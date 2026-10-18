//! GlassCell in the browser: the Mac frontend's design (toolbar, inspector, glass) over the
//! GridCraft engine, written in Rust and compiled to wasm. Build with `trunk build --release`
//! from this directory (output in `dist/glasscell-web`).
//!
//! Like the Mac app it acts only through the engine's commands (`Session::run`) and draws the
//! engine's viewport read model: the grid is a canvas, the chrome is DOM and CSS.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
// Natively only the tests use the document core; the page is wasm.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod doc;
#[cfg(target_arch = "wasm32")]
mod icons;
#[cfg(target_arch = "wasm32")]
mod measure;
#[cfg(target_arch = "wasm32")]
mod render;
mod shortcuts;
#[cfg(target_arch = "wasm32")]
mod ui;

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    ui::mount();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("glasscell-web only runs in the browser: build it with `trunk build --release` in apps/glasscell-web");
}
