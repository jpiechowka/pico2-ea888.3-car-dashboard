//! Common loopback fetch policy. Callers own deadlines, authentication and continuation epochs.
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, ReadableStreamDefaultReader, Request, RequestInit, Response};

pub(super) fn request_init(
    method: &str,
    abort: &AbortController,
) -> RequestInit {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_signal(Some(&abort.signal()));
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_cache(web_sys::RequestCache::NoStore);
    init.set_redirect(web_sys::RequestRedirect::Error);
    init
}

pub(super) async fn fetch_bounded(
    request: &Request,
    max_bytes: usize,
    accept_storage_conflict: bool,
) -> Result<Vec<u8>, &'static str> {
    let response = JsFuture::from(
        web_sys::window()
            .ok_or("Browser window unavailable")?
            .fetch_with_request(request),
    )
    .await
    .map_err(|_| "Helper unreachable or request timed out. Check the local terminal, then reconnect.")?
    .dyn_into::<Response>()
    .map_err(|_| "Invalid helper response")?;
    if !response.ok() && !(accept_storage_conflict && response.status() == 409) {
        return Err(match response.status() {
            403 => "Helper session expired or origin rejected. Reconnect from Studio on port 8080.",
            409 => "Another helper job is active. Reconnect to inspect its status.",
            404 => "This helper does not support the requested operation.",
            _ => "Helper request failed. Check its terminal for details.",
        });
    }
    let stream = response.body().ok_or("Helper response body unavailable")?;
    let reader = stream
        .get_reader()
        .dyn_into::<ReadableStreamDefaultReader>()
        .map_err(|_| "Cannot read helper response")?;
    let mut bytes = Vec::new();
    let result = async {
        loop {
            let chunk = JsFuture::from(reader.read())
                .await
                .map_err(|_| "Helper response was interrupted")?;
            if js_sys::Reflect::get(&chunk, &"done".into())
                .ok()
                .and_then(|v| v.as_bool())
                == Some(true)
            {
                break;
            }
            let value = js_sys::Reflect::get(&chunk, &"value".into()).map_err(|_| "Invalid helper stream")?;
            let data = value
                .dyn_into::<js_sys::Uint8Array>()
                .map_err(|_| "Invalid helper byte stream")?;
            let count = data.length() as usize;
            if count > max_bytes.saturating_sub(bytes.len()) {
                return Err("Helper response exceeds the allowed size");
            }
            let at = bytes.len();
            bytes.resize(at + count, 0);
            data.copy_to(&mut bytes[at..]);
        }
        Ok(bytes)
    }
    .await;
    reader.release_lock();
    result
}

