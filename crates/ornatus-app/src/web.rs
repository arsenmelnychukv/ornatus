//! Browser entry point. All page setup is done from Rust, so the only HTML is
//! the tiny `index.html` that trunk uses to load the wasm (decision 0002).

use eframe::wasm_bindgen::JsCast as _;
use eframe::web_sys;

use crate::OrnatusApp;
use crate::app::Inbox;

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

/// Open the browser's file picker and queue the chosen `.glb` for loading.
pub fn pick_file(inbox: Inbox, ctx: eframe::egui::Context) {
    if let Err(error) = try_pick_file(inbox, ctx) {
        log::error!("file picker failed: {error:?}");
    }
}

fn try_pick_file(inbox: Inbox, ctx: eframe::egui::Context) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::{JsCast as _, closure::Closure};

    let document = document().map_err(|e| wasm_bindgen::JsValue::from_str(&e))?;
    let input = document
        .create_element("input")?
        .dyn_into::<web_sys::HtmlInputElement>()?;
    input.set_type("file");
    input.set_accept(".glb,model/gltf-binary");
    let source = input.clone();
    let on_change = Closure::once_into_js(move || {
        let Some(file) = source.files().and_then(|files| files.get(0)) else {
            return;
        };
        let name = file.name();
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                .await
                .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec())
                .map_err(|e| format!("{e:?}"));
            inbox.lock().expect("inbox lock").push((name, result));
            ctx.request_repaint();
        });
    });
    input.set_onchange(Some(on_change.unchecked_ref()));
    input.click();
    Ok(())
}

type LeaveHandler = wasm_bindgen::closure::Closure<dyn FnMut(web_sys::BeforeUnloadEvent)>;

thread_local! {
    static LEAVE_WARNING: std::cell::RefCell<Option<LeaveHandler>> =
        const { std::cell::RefCell::new(None) };
}

/// Ask the browser to confirm before the page is closed or reloaded
/// (decision 0003: unsaved edits warn on leaving).
pub fn set_leave_warning(on: bool) {
    use wasm_bindgen::JsCast as _;
    let Some(window) = web_sys::window() else {
        return;
    };
    LEAVE_WARNING.with(|slot| {
        let mut slot = slot.borrow_mut();
        if on && slot.is_none() {
            let closure =
                wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
                    |event: web_sys::BeforeUnloadEvent| {
                        event.prevent_default();
                        event.set_return_value("");
                    },
                );
            let _ = window
                .add_event_listener_with_callback("beforeunload", closure.as_ref().unchecked_ref());
            *slot = Some(closure);
        } else if !on && let Some(closure) = slot.take() {
            let _ = window.remove_event_listener_with_callback(
                "beforeunload",
                closure.as_ref().unchecked_ref(),
            );
        }
    });
}
