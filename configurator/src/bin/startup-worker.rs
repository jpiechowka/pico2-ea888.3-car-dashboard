#[cfg(not(target_arch = "wasm32"))]
fn main() {}

#[cfg(target_arch = "wasm32")]
fn main() {
    use dashboard_startup_tools::{MAX_SOURCE_BYTES, Options, convert};
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    let scope: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let sender = scope.clone();
    let listener = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        let result = (|| -> Result<_, String> {
            let data = event.data();
            let source = js_sys::Reflect::get(&data, &"source".into())
                .map_err(|_| "Missing source")?
                .dyn_into::<js_sys::ArrayBuffer>()
                .map_err(|_| "Invalid source buffer")?;
            if source.byte_length() as usize > MAX_SOURCE_BYTES {
                return Err("Source exceeds 8 MiB".into());
            }
            let options = js_sys::Reflect::get(&data, &"options".into()).map_err(|_| "Missing options")?;
            if !options.is_string() {
                return Err("Invalid options".into());
            }
            let options: js_sys::JsString = options.unchecked_into();
            if options.length() > 1024 {
                return Err("Options too large".into());
            }
            let options: Options = serde_json::from_str(&String::from(options)).map_err(|_| "Invalid options")?;
            convert(&js_sys::Uint8Array::new(&source).to_vec(), &options).map_err(|e| e.to_string())
        })();
        let reply = js_sys::Object::new();
        match result {
            Ok(converted) => {
                let bytes = js_sys::Uint8Array::from(converted.asset.as_slice());
                let buffer = bytes.buffer();
                let summary = serde_json::to_string(&(
                    converted.source_width,
                    converted.source_height,
                    converted.source_frames,
                    converted.source_duration_ms,
                    converted.zero_delay_frames,
                    converted.timing_changed,
                    converted.trim_end_ms,
                ))
                .expect("finite summary");
                let _ = js_sys::Reflect::set(&reply, &"asset".into(), &buffer);
                let _ = js_sys::Reflect::set(&reply, &"summary".into(), &summary.into());
                let transfer = js_sys::Array::new();
                transfer.push(&buffer);
                let _ = sender.post_message_with_transfer(&reply, &transfer);
            }
            Err(error) => {
                let _ = js_sys::Reflect::set(&reply, &"error".into(), &error.into());
                let _ = sender.post_message(&reply);
            }
        }
    });
    scope.set_onmessage(Some(listener.as_ref().unchecked_ref()));
    listener.forget();
    let _ = scope.post_message(&JsValue::from_str("ready"));
}
