//! Browser Studio entry point; native execution only explains the build workflow.
#[cfg(target_arch = "wasm32")]
/// Leptos browser adapters for the shared editor and synthetic renderer.
mod ui;

#[cfg(target_arch = "wasm32")]
/// Mount Studio into the browser document body under Leptos ownership.
fn main() { leptos::mount::mount_to_body(ui::Studio); }

#[cfg(not(target_arch = "wasm32"))]
/// Print the native workflow hint; no editor, helper or hardware operation is started.
fn main() {
    println!("Build the browser studio with Trunk; run cargo test for native contracts.");
}
