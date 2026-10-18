//! Text measurement for the engine's viewport, through a scratch canvas.

use std::cell::RefCell;
use std::collections::HashMap;

use gridcraft_engine::model::Font;
use gridcraft_engine::viewport::Measure;
use wasm_bindgen::JsCast as _;
use web_sys::CanvasRenderingContext2d as Ctx;

/// Fonts to fall back on when the file's face (Calibri…) isn't installed: Carlito is
/// metric-compatible with Calibri where it exists, then the system fonts.
const FALLBACK: &str = "Carlito, -apple-system, \"Helvetica Neue\", Arial, sans-serif";

/// A CSS font for `font` at `size` screen units.
pub fn font_css(font: &Font, size: f32) -> String {
    let name: String = font.name.chars().filter(|c| !matches!(c, '"' | '\\' | '\n')).collect();
    format!("{}{}{:.2}px \"{}\", {}", if font.italic { "italic " } else { "" }, if font.bold { "bold " } else { "" }, size.max(1.0), name, FALLBACK)
}

type Key = (String, String, bool, bool, u32);

thread_local! {
    static SCRATCH: Option<Ctx> = scratch();
    static WIDTHS: RefCell<HashMap<Key, f32>> = RefCell::new(HashMap::new());
    static CHART_WIDTHS: RefCell<HashMap<(String, u32, bool), f32>> = RefCell::new(HashMap::new());
}

fn scratch() -> Option<Ctx> {
    let canvas = web_sys::window()?.document()?.create_element("canvas").ok()?.dyn_into::<web_sys::HtmlCanvasElement>().ok()?;
    canvas.get_context("2d").ok()??.dyn_into::<Ctx>().ok()
}

/// Widths from the browser's own text engine, remembered across frames.
pub struct CanvasMeasure;

impl Measure for CanvasMeasure {
    fn text_width(&self, text: &str, font: &Font, size: f32) -> f32 {
        let key = (text.to_string(), font.name.clone(), font.bold, font.italic, size.to_bits());
        if let Some(w) = WIDTHS.with(|c| c.borrow().get(&key).copied()) {
            return w;
        }
        let w = SCRATCH
            .with(|ctx| {
                let ctx = ctx.as_ref()?;
                ctx.set_font(&font_css(font, size));
                ctx.measure_text(text).ok().map(|m| m.width() as f32)
            })
            .filter(|w| w.is_finite())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0e7);
        WIDTHS.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() > 50_000 {
                c.clear();
            }
            c.insert(key, w);
        });
        w
    }
}

/// Text widths for chart primitives (size and weight only: charts use the system font).
pub struct ChartMeasure;

impl gridcraft_chart::Measure for ChartMeasure {
    fn text_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let key = (text.to_string(), size.to_bits(), bold);
        if let Some(w) = CHART_WIDTHS.with(|c| c.borrow().get(&key).copied()) {
            return w;
        }
        let w = SCRATCH
            .with(|ctx| {
                let ctx = ctx.as_ref()?;
                ctx.set_font(&format!("{}{:.2}px {FALLBACK}", if bold { "bold " } else { "" }, size.max(1.0)));
                ctx.measure_text(text).ok().map(|m| m.width() as f32)
            })
            .filter(|w| w.is_finite())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0e7);
        CHART_WIDTHS.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() > 20_000 {
                c.clear();
            }
            c.insert(key, w);
        });
        w
    }
}
