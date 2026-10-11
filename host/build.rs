use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let project = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let driver = project.join("../firmware/pico2/src/drivers/st7789.rs");
    println!("cargo:rerun-if-changed={}", driver.display());

    // The old driver mixes the pure DrawTarget with the MCU SPI owner. Keep the
    // original renderer in the host harness until those parts are separated.
    let source = fs::read_to_string(&driver).expect("read production display driver");
    let marker = "pub struct St7789Renderer<'a>";
    let offset = source
        .find(marker)
        .expect("renderer boundary changed; update host harness");
    let generated = format!(
        "use embedded_graphics::pixelcolor::{{Rgb565, raw::RawU16}};\nuse embedded_graphics::prelude::*;\npub const \
         WIDTH: usize = 320;\npub const HEIGHT: usize = 240;\n{}",
        &source[offset..]
    );
    let output = PathBuf::from(env::var("OUT_DIR").expect("output directory"));
    fs::write(output.join("renderer.rs"), generated).expect("write host renderer module");
}
