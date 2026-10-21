//! Memoized display facts, never a captured build or device authorization.
use dashboard_config::startup::StartupProfile;
use dashboard_helper_protocol::ArtifactSummary;

use super::{BuildOptions, BuildSelection, Editor, PreparedBuild};
use crate::editor::AcceptedConfigKey;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Content/options comparison used to label an artifact current or stale in the UI.
pub struct ArtifactDisplayIdentity {
    /// Normalized requested build options; not a captured build request.
    pub(super) options: BuildOptions,
    /// Project, configuration, startup-profile and startup-asset SHA256 strings, in order.
    pub(super) hashes: [String; 4],
}
impl ArtifactDisplayIdentity {
    /// Returns whether `artifact` matches these options and four content digests.
    /// Does not check helper session, source revision, device or flash authorization.
    pub fn matches_artifact(
        &self,
        artifact: &ArtifactSummary,
    ) -> bool {
        matches(&self.options, &self.hashes, artifact)
    }
}

#[derive(PartialEq, Eq)]
/// Accepted-content cache key, independent of selection-only UI changes/build options.
struct ContentKey {
    /// Owning editor and accepted configuration lineage.
    config: AcceptedConfigKey,
    /// Accepted startup behavior, included even if configuration itself is unchanged.
    profile: StartupProfile,
    /// Accepted converted-artwork hash, or absent when no artwork is attached.
    artwork: Option<[u8; 32]>,
}

/// Retains only the last display digests: no payload, original pin, or session capability.
#[derive(Default)]
pub struct ArtifactDisplayCache {
    /// Most recent accepted-content identity and ordered digests; no payload is retained.
    content: Option<(ContentKey, [String; 4])>,
}
impl ArtifactDisplayCache {
    /// Returns display identity for `editor` and normalized `selection`, reusing
    /// digests only when accepted content matches. Readiness/options/capture failures
    /// return diagnostics without replacing cached content. Every call rechecks
    /// pending edits even when its content hash would otherwise be cached.
    pub fn inspect(
        &mut self,
        editor: &Editor,
        selection: &BuildSelection,
    ) -> Result<ArtifactDisplayIdentity, &'static str> {
        // JSON/pointer/import readiness can change without accepted content publication.
        if editor.action_readiness().block.is_some() {
            return Err("Finish or discard pending edits before building");
        }
        let options = selection
            .normalized()
            .map_err(|_| "Correct build options before building")?
            .requested;
        let key = ContentKey {
            config: editor.accepted_config_key(),
            profile: editor.startup_profile(),
            artwork: editor.startup_artwork().map(|art| art.identity()),
        };
        if self.content.as_ref().is_none_or(|(old, _)| *old != key) {
            let fresh = PreparedBuild::capture(editor, selection, 0)?;
            self.content = Some((key, fresh.hashes));
        }
        Ok(ArtifactDisplayIdentity {
            options,
            hashes: self.content.as_ref().expect("content populated").1.clone(),
        })
    }
}

/// Returns whether `artifact` has requested `options` and ordered `hashes`
/// (project/configuration/startup-profile/startup-asset); performs no authorization.
pub(super) fn matches(
    options: &BuildOptions,
    hashes: &[String; 4],
    artifact: &ArtifactSummary,
) -> bool {
    artifact.options.requested == *options
        && [
            artifact.project_sha256.as_str(),
            artifact.config_sha256.as_str(),
            artifact.startup_profile_sha256.as_str(),
            artifact.startup_asset_sha256.as_str(),
        ] == hashes.each_ref().map(String::as_str)
}

#[cfg(test)]
/// Cache-key ownership and allocation-retention regressions.
mod tests {
    use super::*;
    use crate::editor::Selection;
    #[test]
    /// Assert selection/options reuse digest allocations, while a new editor replaces lineage.
    fn hits_retain_digest_allocations_but_new_owner_changes_key() {
        let mut editor = Editor::default();
        let mut selection = BuildSelection::default();
        let mut cache = ArtifactDisplayCache::default();
        cache.inspect(&editor, &selection).unwrap();
        let ptr = cache.content.as_ref().unwrap().1[0].as_ptr();
        editor
            .select(Selection {
                view: 1,
                widget: Some(editor.preview().config().widgets[0].id),
            })
            .unwrap();
        selection.options.full_frame = true;
        cache.inspect(&editor, &selection).unwrap();
        assert_eq!(cache.content.as_ref().unwrap().1[0].as_ptr(), ptr);
        let old = cache.content.as_ref().unwrap().0.config;
        let replacement = Editor::default();
        cache.inspect(&replacement, &selection).unwrap();
        assert_ne!(cache.content.as_ref().unwrap().0.config, old);
    }
}
