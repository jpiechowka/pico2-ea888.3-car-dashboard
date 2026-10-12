#[cfg(target_arch = "wasm32")]
mod ui;

#[cfg(target_arch = "wasm32")]
fn main() { leptos::mount::mount_to_body(ui::Studio); }

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!("Build the browser studio with Trunk; run cargo test for native contracts.");
}
