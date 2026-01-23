//! Build script for dashboard-simulator
//!
//! Sets up SDL2 library paths and copies SDL2.dll to the target directory.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    // Only run SDL2 setup on Windows
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor_sdl2 = manifest_dir.parent().unwrap().join("vendor").join("sdl2");

    // Add the vendor/sdl2 directory to the library search path
    if vendor_sdl2.exists() {
        println!("cargo:rustc-link-search=native={}", vendor_sdl2.display());

        // Copy SDL2.dll to the target directory
        // The target directory is tricky to find reliably, so we use OUT_DIR's parent
        if let Ok(out_dir) = env::var("OUT_DIR") {
            let out_path = PathBuf::from(&out_dir);
            // OUT_DIR is like target/release/build/dashboard-simulator-xxx/out
            // We need to go up to target/release
            if let Some(target_dir) = out_path
                .ancestors()
                .find(|p| p.file_name().is_some_and(|n| n == "release" || n == "debug"))
            {
                let dll_src = vendor_sdl2.join("SDL2.dll");
                let dll_dst = target_dir.join("SDL2.dll");

                if dll_src.exists() && !dll_dst.exists() {
                    if let Err(e) = fs::copy(&dll_src, &dll_dst) {
                        println!("cargo:warning=Failed to copy SDL2.dll: {}", e);
                    } else {
                        println!("cargo:warning=Copied SDL2.dll to {}", dll_dst.display());
                    }
                }
            }
        }
    } else {
        println!(
            "cargo:warning=SDL2 vendor directory not found at {}",
            vendor_sdl2.display()
        );
        println!("cargo:warning=Please ensure SDL2.lib and SDL2.dll are in firmware/vendor/sdl2/");
    }

    // Rerun if the vendor directory changes
    println!("cargo:rerun-if-changed={}", vendor_sdl2.display());
}
