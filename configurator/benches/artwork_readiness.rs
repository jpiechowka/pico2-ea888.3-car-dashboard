//! Readiness cost with admitted immutable artwork. Host time, not device FPS.
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use dashboard_config_tools::project as p;
use dashboard_configurator::editor::Editor;
use dashboard_startup_tools::{Fit, Options, Sheet, SourceKind};

fn artwork(
    source: &[u8],
    options: Options,
) -> Arc<p::Artwork> {
    let converted = dashboard_startup_tools::convert(source, &options).unwrap();
    let recipe = p::Recipe {
        version: 2,
        converter: p::CONVERTER.into(),
        label: "Readiness benchmark".into(),
        source_kind: match options.kind {
            SourceKind::Gif => p::SourceKind::Gif,
            _ => p::SourceKind::SpriteSheet,
        },
        source_sha256: p::digest(source),
        source_length: source.len() as u32,
        original_included: true,
        asset_sha256: p::digest(&converted.asset),
        geometry: options.geometry,
        fit: match options.fit {
            Fit::Stretch => p::Fit::Stretch,
            _ => p::Fit::Contain,
        },
        matte_rgb: options.matte_rgb,
        sample_fps: options.sample_fps,
        trim_start_ms: options.trim_start_ms,
        trim_end_ms: options.trim_end_ms,
        sheet: options.sheet.map(|s| {
            p::Sheet {
                columns: s.columns,
                rows: s.rows,
                frame_ms: s.frame_ms,
            }
        }),
        gif_background: p::GifBackground::Logical,
    };
    Arc::new(p::Artwork::new(converted.asset.into(), recipe, Some(Arc::from(source))).unwrap())
}
fn run() {
    let tiny = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";
    let small = artwork(
        tiny,
        Options {
            kind: SourceKind::Gif,
            trim_end_ms: 100,
            ..Default::default()
        },
    );
    let maximum = artwork(
        include_bytes!("fixtures/capacity-sprite.png"),
        Options {
            kind: SourceKind::SpriteSheet,
            fit: Fit::Stretch,
            sample_fps: 10,
            trim_end_ms: 400,
            sheet: Some(Sheet {
                columns: 2,
                rows: 2,
                frame_ms: 100,
            }),
            ..Default::default()
        },
    );
    assert_eq!(maximum.converted().len(), 524288);
    println!("case,asset_bytes,asset_sha256,batch,iterations,ns_per_readiness_and_save_status");
    for (name, art) in [("none", None), ("small", Some(small)), ("maximum", Some(maximum))] {
        let mut editor = Editor::default();
        if let Some(art) = art {
            editor.set_artwork_if_current(editor.stamp(), art).unwrap();
        }
        let (bytes, hash) = editor.startup_artwork().map_or((0, String::new()), |a| {
            (a.converted().len(), dashboard_config_tools::sha256_hex(a.converted()))
        });
        let query = || {
            black_box(black_box(&editor).action_readiness());
            black_box(black_box(&editor).save_status());
        };
        for _ in 0..16 {
            query();
        }
        for batch in 0..7 {
            let iterations = if bytes > 100000 { 128 } else { 16384 };
            let start = Instant::now();
            for _ in 0..iterations {
                query();
            }
            println!(
                "{name},{bytes},{hash},{batch},{iterations},{:.3}",
                start.elapsed().as_nanos() as f64 / f64::from(iterations)
            );
        }
    }
}
fn main() {
    std::thread::Builder::new()
        .name("readiness-benchmark".into())
        .stack_size(2 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
