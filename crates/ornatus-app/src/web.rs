//! Browser entry point. All page setup is done from Rust, so the only HTML is
//! the tiny `index.html` that trunk uses to load the wasm (decision 0002).

use eframe::wasm_bindgen::JsCast as _;
use eframe::web_sys;

use crate::OrnatusApp;

const UNSUPPORTED: &str = "ornatus needs a browser with WebGPU or WebGL2. \
Please use a current version of Chrome, Edge, Firefox or Safari on a desktop.";

fn document() -> Result<web_sys::Document, String> {
    web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "no document".to_owned())
}

/// Create a full-window canvas and start the app on it.
pub async fn start() -> Result<(), String> {
    let document = document()?;
    let body = document.body().ok_or("no body")?;
    let js = |e: eframe::wasm_bindgen::JsValue| format!("{e:?}");
    body.style().set_property("margin", "0").map_err(js)?;
    body.style()
        .set_property("background", "#18181c")
        .map_err(js)?;

    let canvas = document
        .create_element("canvas")
        .map_err(js)?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "not a canvas".to_owned())?;
    canvas
        .style()
        .set_property("display", "block")
        .map_err(js)?;
    canvas.style().set_property("width", "100vw").map_err(js)?;
    canvas.style().set_property("height", "100vh").map_err(js)?;
    body.append_child(&canvas).map_err(js)?;

    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions {
                depth_buffer: 32,
                ..Default::default()
            },
            Box::new(|cc| Ok(Box::new(OrnatusApp::new(cc)))),
        )
        .await
        .map_err(|e| format!("{e:?}"))
}

/// Replace the page with a plain message when the app cannot start.
pub fn show_unsupported() {
    let Ok(document) = document() else { return };
    let Some(body) = document.body() else { return };
    body.set_inner_html("");
    let Ok(message) = document.create_element("p") else {
        return;
    };
    message.set_text_content(Some(UNSUPPORTED));
    let _ = message.set_attribute(
        "style",
        "color:#ddd;font-family:sans-serif;max-width:32em;margin:3em auto;padding:0 1em",
    );
    let _ = body.append_child(&message);
}
