use dashboard_configurator::build_flash::BuildSelection;
use dashboard_configurator::editor::*;
use dashboard_configurator::helper_client::*;
#[path = "support/display_fixture.rs"]
mod fixture;

fn oracle(
    cache: &mut ArtifactDisplayCache,
    e: &Editor,
    s: &BuildSelection,
) -> Option<ArtifactDisplayIdentity> {
    let fresh = PreparedBuild::capture(e, s, 0).map(|p| p.display_identity());
    let cached = cache.inspect(e, s);
    assert_eq!(cached, fresh);
    cached.ok()
}
#[test]
fn display_identity_tracks_complete_content_and_live_readiness() {
    let mut e = Editor::default();
    let mut s = BuildSelection::default();
    let mut c = ArtifactDisplayCache::default();
    let initial = oracle(&mut c, &e, &s).unwrap();
    let widget = e.preview().config().widgets[0].id;
    e.select(Selection {
        view: e.selection().view,
        widget: Some(widget),
    })
    .unwrap();
    assert_eq!(oracle(&mut c, &e, &s), Some(initial.clone()));
    let stamp = e.stamp();
    let json = e.open_json().unwrap();
    e.set_json_text(json, "{".into()).unwrap();
    assert_eq!(e.stamp(), stamp);
    assert!(oracle(&mut c, &e, &s).is_none());
    e.discard_json().unwrap();
    assert_eq!(oracle(&mut c, &e, &s), Some(initial.clone()));
    e.set_startup_number(e.stamp(), StartupNumber::SplashMilliseconds, "bad")
        .unwrap();
    assert!(oracle(&mut c, &e, &s).is_none());
    e.discard_startup_timing(e.stamp()).unwrap();
    e.set_raw(e.field_token(), RawField::ViewName(1), "".into()).unwrap();
    assert!(oracle(&mut c, &e, &s).is_none());
    e.discard_raw().unwrap();
    let pointer = e.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    assert!(oracle(&mut c, &e, &s).is_none());
    e.cancel_pointer(pointer).unwrap();
    let import = e.begin_import().unwrap();
    assert!(oracle(&mut c, &e, &s).is_none());
    e.cancel_import(import).unwrap();
    let mut profile = e.startup_profile();
    profile.duration_ms += 1;
    e.set_startup_profile(profile).unwrap();
    assert_ne!(oracle(&mut c, &e, &s), Some(initial.clone()));
    e.undo().unwrap();
    assert_eq!(oracle(&mut c, &e, &s), Some(initial.clone()));
    e.redo().unwrap();
    oracle(&mut c, &e, &s);
    e.apply(Edit::SetReducedMotion(true)).unwrap();
    oracle(&mut c, &e, &s);
    s.options.full_frame = true;
    oracle(&mut c, &e, &s);
    s.select("custom").unwrap();
    assert!(oracle(&mut c, &e, &s).is_none());
    s.options.expert = true;
    oracle(&mut c, &e, &s);
    s.cpu.set("oops");
    assert!(oracle(&mut c, &e, &s).is_none());
    s.cpu.set("150000000");
    oracle(&mut c, &e, &s);
    let bytes = Editor::default().export_binary().unwrap();
    let import = e.begin_import().unwrap();
    e.complete_import(import, ImportFormat::Binary, &bytes).unwrap();
    oracle(&mut c, &e, &s);
    oracle(&mut c, &Editor::default(), &s);
}
#[test]
fn artwork_recipe_original_identity_and_undo_never_reuse_old_hashes() {
    let mut e = Editor::default();
    let s = BuildSelection::default();
    let mut c = ArtifactDisplayCache::default();
    let asset = fixture::artwork("one", 1);
    e.set_artwork_if_current(e.stamp(), asset.clone()).unwrap();
    let first = oracle(&mut c, &e, &s).unwrap();
    let mut recipe = asset.recipe().clone();
    recipe.label = "two".into();
    recipe.original_included = false;
    let changed = std::sync::Arc::new(
        dashboard_config_tools::project::Artwork::new(asset.converted_arc(), recipe, None).unwrap(),
    );
    e.set_artwork_if_current(e.stamp(), changed).unwrap();
    assert_ne!(oracle(&mut c, &e, &s), Some(first.clone()));
    e.undo().unwrap();
    assert_eq!(oracle(&mut c, &e, &s), Some(first.clone()));
    e.set_artwork_if_current(e.stamp(), fixture::artwork("one", 2)).unwrap();
    assert_ne!(oracle(&mut c, &e, &s), Some(first));
    assert!(e.startup_artwork().unwrap().original().is_none());
    let mut pixels = asset.converted().to_vec();
    pixels[112] ^= 1;
    let crc = dashboard_config::startup_asset::crc32(&pixels[112..112 + 153600]);
    pixels[44..48].copy_from_slice(&crc.to_le_bytes());
    let crc = dashboard_config::startup_asset::asset_crc32(&pixels).unwrap();
    pixels[28..32].copy_from_slice(&crc.to_le_bytes());
    let mut recipe = asset.recipe().clone();
    recipe.asset_sha256 = dashboard_config_tools::project::digest(&pixels);
    recipe.original_included = false;
    let changed =
        std::sync::Arc::new(dashboard_config_tools::project::Artwork::new(pixels.into(), recipe, None).unwrap());
    e.set_artwork_if_current(e.stamp(), changed).unwrap();
    oracle(&mut c, &e, &s);
    let saved = e
        .export_project(dashboard_config_tools::project::ExportMode::ConvertedOnly)
        .unwrap();
    let token = e.begin_import().unwrap();
    e.complete_project_import(token, &saved).unwrap();
    oracle(&mut c, &e, &s);
}
