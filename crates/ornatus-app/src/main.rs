#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        depth_buffer: 32,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ornatus")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ornatus",
        options,
        Box::new(|cc| Ok(Box::new(ornatus_app::OrnatusApp::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = ornatus_app::web::start().await {
            log::error!("start failed: {error}");
            ornatus_app::web::show_unsupported();
        }
    });
}
