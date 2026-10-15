//! Browser file access: File › Save downloads the project, File › Open reads one the user picks.
//! (The Mac app uses native dialogs in `dialogs.rs`.)

use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

thread_local! {
    /// (file name, contents) of a picked file, waiting for the app to take it.
    static OPENED: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Starts a download of `text` as `name`.
pub fn download(name: &str, text: &str) -> Result<(), String> {
    let err = |e: JsValue| format!("{e:?}");
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts).map_err(err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
    let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(err)?.dyn_into().map_err(|_| "no anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// Asks the browser for a file (`accept` like ".phosphor,.json"); it arrives later through `take_opened`.
pub fn pick_text(accept: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Ok(input) = doc.create_element("input").map(|e| e.unchecked_into::<web_sys::HtmlInputElement>()) else { return };
    input.set_type("file");
    input.set_accept(accept);
    let picked = input.clone();
    let on_change = Closure::once(move || {
        let Some(file) = picked.files().and_then(|f| f.get(0)) else { return };
        let name = file.name();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(text) = wasm_bindgen_futures::JsFuture::from(file.text()).await
                && let Some(text) = text.as_string()
            {
                OPENED.with(|o| *o.borrow_mut() = Some((name, text)));
            }
        });
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    on_change.forget();
    input.click();
}

/// The file the user picked, once it has been read.
pub fn take_opened() -> Option<(String, String)> {
    OPENED.with(|o| o.borrow_mut().take())
}
