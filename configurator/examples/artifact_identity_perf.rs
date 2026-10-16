//! Run alone in release mode; prints raw same-workload native timings, not browser/Pico FPS.
use std::hint::black_box;
use std::time::Instant;

use dashboard_configurator::build_flash::BuildSelection;
use dashboard_configurator::editor::*;
use dashboard_configurator::helper_client::*;
#[path = "../tests/support/display_fixture.rs"]
mod fixture;

fn run(
    large: bool,
    cached: bool,
    workload: &str,
    count: usize,
) -> u128 {
    let mut editor = Editor::default();
    if large {
        editor
            .set_artwork_if_current(editor.stamp(), fixture::artwork("performance", 1))
            .unwrap();
    }
    let mut selection = BuildSelection::default();
    let mut cache = ArtifactDisplayCache::default();
    // Warm both paths outside timing; every measured action uses the same real reducer.
    black_box(cache.inspect(&editor, &selection).unwrap());
    black_box(PreparedBuild::capture(&editor, &selection, 0).unwrap());
    let widget = editor.preview().config().widgets[0].id;
    let started = Instant::now();
    for index in 0..count {
        match workload {
            "selection" => {
                editor
                    .select(Selection {
                        view: 1,
                        widget: if index % 2 == 0 { Some(widget) } else { None },
                    })
                    .unwrap();
            }
            "options" => selection.options.full_frame = index % 2 == 0,
            "miss" => {
                let mut profile = editor.startup_profile();
                profile.duration_ms = 2501 + (index % 2) as u16;
                editor.set_startup_profile(profile).unwrap();
            }
            "blocked" => {
                if index % 2 == 0 {
                    editor
                        .set_startup_number(editor.stamp(), StartupNumber::SplashMilliseconds, "bad")
                        .unwrap();
                } else {
                    editor.discard_startup_timing(editor.stamp()).unwrap();
                }
            }
            _ => unreachable!(),
        }
        let result = if cached {
            cache.inspect(&editor, &selection)
        } else {
            PreparedBuild::capture(&editor, &selection, 0).map(|p| p.display_identity())
        };
        black_box(result.ok());
    }
    started.elapsed().as_nanos()
}
fn main() {
    let count = std::env::args()
        .nth(1)
        .map(|s| s.parse::<usize>().expect("iteration count"))
        .unwrap_or(100);
    assert!((1..=10000).contains(&count));
    for large in [false, true] {
        let mut editor = Editor::default();
        if large {
            editor
                .set_artwork_if_current(editor.stamp(), fixture::artwork("performance", 1))
                .unwrap();
        }
        let prepared = PreparedBuild::capture(&editor, &BuildSelection::default(), 0).unwrap();
        println!(
            "fixture,large={large},asset_bytes={},project_bytes={},project_sha256={}",
            editor.startup_artwork().map_or(0, |a| a.converted().len()),
            prepared.bytes().len(),
            dashboard_config_tools::sha256_hex(prepared.bytes())
        );
        for workload in ["selection", "options", "miss", "blocked"] {
            for batch in 0..5 {
                // Alternate order to expose drift; retain every raw sample.
                for cached in if batch % 2 == 0 { [false, true] } else { [true, false] } {
                    println!(
                        "sample,large={large},workload={workload},cached={cached},batch={batch},iterations={count},\
                         ns={}",
                        run(large, cached, workload, count)
                    );
                }
            }
        }
    }
}
