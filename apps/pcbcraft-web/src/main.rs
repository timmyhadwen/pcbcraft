//! Browser entry point: starts the egui app on the page's canvas.
//!
//! Local rules: the page text that appears before the wasm loads lives in
//! `index.html` and `webgpu-check.js`; its wording must equal the catalog key
//! `web.needs_webgpu` (test `the_page_message_matches_the_catalog`). The wasm
//! build needs a browser with WebGPU; a failed start shows the same message.
//!
//! # Dependencies
//! - `../../crates/text/strings/en.txt` (message text)
//! - `index.html`, `webgpu-check.js`
//!
//! # Do not
//! - Add text literals here; read them from the catalog.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Build the web app with trunk in apps/pcbcraft-web.");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = start().await {
            web_sys::console::error_1(&error);
            show_start_failure();
        }
    });
}
#[cfg(target_arch = "wasm32")]
async fn start() -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser document unavailable"))?;
    let canvas = document
        .get_element_by_id("pcbcraft")
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("missing pcbcraft canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let session = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                Ok(Box::new(pcbcraft_ui_egui::App::new(
                    cc, session, None, None, false,
                )))
            }),
        )
        .await
}

/// Show the "needs WebGPU" notice that `index.html` carries, hide the canvas.
///
/// Called when the app fails to start after load, which is what a browser
/// without a usable graphics adapter produces. The text is the page's own
/// paragraph, kept equal to catalog key `web.needs_webgpu` by a test.
///
/// # Rules
/// - Best effort: a missing element is ignored, the console already has the error.
///
/// # Dependencies
/// - `index.html` (`#needs-webgpu`, `#pcbcraft`)
///
/// # Do not
/// - Panic or return an error; this runs on the failure path.
#[cfg(target_arch = "wasm32")]
fn show_start_failure() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(notice) = document.get_element_by_id("needs-webgpu") {
        let _ = notice.remove_attribute("hidden");
    }
    if let Some(canvas) = document.get_element_by_id("pcbcraft") {
        let _ = canvas.set_attribute("hidden", "");
    }
}

#[cfg(test)]
mod tests {
    /// The paragraph in index.html says what the catalog says.
    #[test]
    fn the_page_message_matches_the_catalog() {
        let html = include_str!("../index.html");
        let catalog = pcbcraft_text::Catalog::english();
        for key in ["web.needs_webgpu", "web.download_desktop", "web.download_desktop.tip"] {
            assert!(catalog.defines(key), "{key} missing from the catalog");
            assert!(
                html.contains(&catalog.get(key)),
                "index.html does not carry the catalog text of {key}"
            );
        }
        let script = include_str!("../webgpu-check.js");
        assert!(script.contains("needs-webgpu"));
    }
}
