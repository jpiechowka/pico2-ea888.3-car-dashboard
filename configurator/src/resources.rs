//! Accepted Studio serialization and borrowed build evidence. Target resource
//! measurements come only from an admitted helper artifact, never host layout.
use dashboard_config::*;

use crate::editor::{ActionBlock, Editor, EditorError, EditorStamp};
use crate::events::{Category, EventError, EventInput, EventSink, RecordOutcome, Severity};

/// Only transition summaries cross into the 512-record common event sink.
/// Helper job stdout/stderr and its 1,024-record per-job log stay separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostSummary {
    Connected,
    Disconnected,
    StageStarted,
    StageCompleted,
    StageFailed,
    ArtifactAvailable,
}

/// `redacted_subject` comes from the helper's redaction boundary and is plain
/// text (for example a job reference or artifact link), never raw tool output.
pub fn record_host_summary(
    sink: &mut impl EventSink,
    summary: HostSummary,
    redacted_subject: Option<&str>,
    elapsed_ms: u64,
    wall_clock_ms: Option<i64>,
) -> Result<RecordOutcome, EventError> {
    let (severity, code, message) = match summary {
        HostSummary::Connected => (Severity::Info, "helper.connected", "Build helper connected"),
        HostSummary::Disconnected => (Severity::Warn, "helper.disconnected", "Build helper disconnected"),
        HostSummary::StageStarted => (Severity::Info, "helper.stage_started", "Build helper stage started"),
        HostSummary::StageCompleted => (Severity::Info, "helper.stage_completed", "Build helper stage completed"),
        HostSummary::StageFailed => {
            (
                Severity::Error,
                "helper.stage_failed",
                "Build helper stage failed; inspect job log",
            )
        }
        HostSummary::ArtifactAvailable => (Severity::Info, "helper.artifact", "Build artifact available"),
    };
    sink.record(EventInput {
        category: Category::HostTools,
        severity,
        code,
        subject: redacted_subject,
        message,
        elapsed_ms,
        wall_clock_ms,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    pub eav_bytes: usize,
    pub json_bytes: usize,
    pub saved_bytes: usize,
    pub views: usize,
    pub widgets: usize,
    pub cells_per_view: u8,
    pub view_name_bytes: usize,
    pub widget_label_bytes: usize,
    pub warning_message_bytes: usize,
}
impl ResourceLimits {
    pub const CURRENT: Self = Self {
        eav_bytes: MAX_FILE_BYTES,
        json_bytes: MAX_JSON_BYTES,
        saved_bytes: crate::editor::MAX_SAVED_BYTES,
        views: MAX_VIEWS,
        widgets: MAX_WIDGETS,
        cells_per_view: 8,
        view_name_bytes: 14,
        widget_label_bytes: 24,
        warning_message_bytes: 20,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewUsage {
    pub id: u8,
    pub name: String,
    pub widget_count: usize,
    pub occupied_mask: u8,
    pub occupied_cells: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSelectionStatus {
    Disabled,
    Unavailable(UnavailableReason),
    PollOff,
    RouteUnknown,
    /// The configured decoder can be selected; live ECU/transport/capability
    /// and sample qualification remain runtime evidence, not config evidence.
    ExecutableSelectionUnqualified,
}

#[derive(Clone, Copy, Debug)]
pub struct SourceUsage {
    pub channel: Channel,
    pub configured: ChannelSource,
    pub catalogue_name: &'static str,
    pub catalogue_interpretation: &'static str,
    pub catalogue_references: &'static [&'static str],
    pub catalogue_unavailable: Option<UnavailableReason>,
    pub status: SourceSelectionStatus,
}

#[derive(Clone, Debug)]
pub struct WidgetUsage {
    pub id: u16,
    pub view: u8,
    pub label: String,
    pub channel: Channel,
    pub readout: Readout,
    pub secondary: Secondary,
    pub capabilities: WidgetCapabilities,
}

#[derive(Clone, Debug)]
pub struct ConfigResources {
    pub stamp: EditorStamp,
    pub accepted_revision: u64,
    pub eav_bytes: usize,
    pub json_bytes: usize,
    pub saved_bytes: usize,
    pub saved_overhead: usize,
    pub limits: ResourceLimits,
    pub views_used: usize,
    pub widgets_used: usize,
    pub views: Vec<ViewUsage>,
    pub widgets: Vec<WidgetUsage>,
    pub sources: Vec<SourceUsage>,
    pub stoich_ratio: f32,
    pub fuel: Fuel,
}
impl ConfigResources {
    /// Presentation relation only. A configured stoichiometric ratio is not
    /// evidence of the fuel actually in a vehicle.
    pub fn afr_for_lambda(
        &self,
        lambda: f32,
    ) -> f32 {
        lambda * self.stoich_ratio
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceAvailability {
    Current,
    LastAccepted(ActionBlock),
    NotExportable(ActionBlock),
}
#[derive(Clone, Debug)]
pub struct ResourceReport {
    pub availability: ResourceAvailability,
    /// Present with LastAccepted only as a labelled former accepted snapshot.
    pub accepted: Option<ConfigResources>,
}

#[derive(Default)]
pub struct ResourceCache {
    accepted: Option<ConfigResources>,
}
impl ResourceCache {
    pub fn inspect(
        &mut self,
        editor: &Editor,
    ) -> Result<ResourceReport, EditorError> {
        // Always read live readiness. JSON text/parse state can change without
        // changing EditorStamp, so a stamp-only cache hit is unsafe.
        let readiness = editor.action_readiness();
        if let Some(block) = readiness.block {
            return Ok(ResourceReport {
                availability: if self.accepted.is_some() {
                    ResourceAvailability::LastAccepted(block)
                } else {
                    ResourceAvailability::NotExportable(block)
                },
                accepted: self.accepted.clone(),
            });
        }
        if self.accepted.as_ref().is_none_or(|old| old.stamp != readiness.stamp) {
            self.accepted = Some(measure_ready(editor)?);
        }
        Ok(ResourceReport {
            availability: ResourceAvailability::Current,
            accepted: self.accepted.clone(),
        })
    }
}

fn measure_ready(editor: &Editor) -> Result<ConfigResources, EditorError> {
    let readiness = editor.action_readiness();
    // Recheck even if the caller inspected before this function. Export methods
    // and the saved-envelope method also perform their own admission check.
    if let Some(block) = readiness.block {
        return Err(match block {
            ActionBlock::InvalidDraft => EditorError::InvalidDraft,
            ActionBlock::JsonPending => EditorError::JsonPending,
            ActionBlock::StartupPending => EditorError::StartupPending,
            ActionBlock::PointerActive => EditorError::PointerActive,
            ActionBlock::ImportPending => EditorError::ImportPending,
            ActionBlock::IdentityExhausted => EditorError::CounterExhausted,
        });
    }
    let eav_bytes = editor.export_binary()?.len();
    let json_bytes = editor.export_json()?.len();
    let saved_bytes = editor.saved_envelope_json()?.len();
    let saved_overhead = saved_bytes
        .checked_sub(json_bytes)
        .ok_or(EditorError::Codec(ConfigError::Length))?;
    let accepted = editor.preview();
    let config = accepted.config();
    let views = config
        .views
        .iter()
        .map(|view| {
            let mut occupied_mask = 0_u8;
            let mut widget_count = 0;
            for widget in config.widgets.iter().filter(|widget| widget.view == view.id) {
                widget_count += 1;
                occupied_mask |= widget.placement.occupied().unwrap_or(0);
            }
            ViewUsage {
                id: view.id,
                name: view.name.to_string(),
                widget_count,
                occupied_mask,
                occupied_cells: occupied_mask.count_ones() as u8,
            }
        })
        .collect();
    let widgets = config
        .widgets
        .iter()
        .map(|widget| {
            WidgetUsage {
                id: widget.id,
                view: widget.view,
                label: widget.label.to_string(),
                channel: widget.channel,
                readout: widget.advanced.readout,
                secondary: widget.advanced.secondary,
                capabilities: widget.capabilities(),
            }
        })
        .collect();
    let sources = Channel::ALL
        .into_iter()
        .map(|channel| {
            let configured = config.sources[channel.index()];
            let catalogue = definition(configured.definition);
            let status = if configured.definition == DefinitionId::Disabled {
                SourceSelectionStatus::Disabled
            } else if let Some(reason) = catalogue.unavailable {
                SourceSelectionStatus::Unavailable(reason)
            } else if configured.poll == Poll::Off {
                SourceSelectionStatus::PollOff
            } else if configured.route == Route::Unknown {
                SourceSelectionStatus::RouteUnknown
            } else {
                SourceSelectionStatus::ExecutableSelectionUnqualified
            };
            SourceUsage {
                channel,
                configured,
                catalogue_name: catalogue.name,
                catalogue_interpretation: catalogue.interpretation,
                catalogue_references: catalogue.references(),
                catalogue_unavailable: catalogue.unavailable,
                status,
            }
        })
        .collect();
    Ok(ConfigResources {
        stamp: readiness.stamp,
        accepted_revision: editor.accepted_revision().get(),
        eav_bytes,
        json_bytes,
        saved_bytes,
        saved_overhead,
        limits: ResourceLimits::CURRENT,
        views_used: config.views.len(),
        widgets_used: config.widgets.len(),
        views,
        widgets,
        sources,
        stoich_ratio: accepted.stoich(),
        fuel: config.fuel,
    })
}

pub const fn canonical_unit(channel: Channel) -> &'static str {
    match channel {
        Channel::Boost => "bar",
        Channel::Oil | Channel::Water | Channel::Dsg | Channel::Iat | Channel::Egt => "°C",
        Channel::Battery => "V",
        Channel::Lambda => "λ",
    }
}
pub const fn boost_display_unit(psi: bool) -> &'static str { if psi { "PSI" } else { "bar" } }

/// Pure borrowed projection over helper-owned identities; never a protocol
/// DTO. None or empty is missing authority, not a wildcard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentityView<'a> {
    pub source_inventory: Option<&'a str>,
    pub compiler: Option<&'a str>,
    pub toolchain: Option<&'a str>,
    pub tools: Option<&'a str>,
}
impl IdentityView<'_> {
    fn complete(self) -> bool {
        [self.source_inventory, self.compiler, self.toolchain, self.tools]
            .into_iter()
            .all(|value| value.is_some_and(|value| !value.is_empty()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetMeasurements {
    pub static_ram_bytes: Option<u64>,
    pub flash_bytes: Option<u64>,
    pub core0_stack_bytes: Option<u64>,
    pub core1_stack_bytes: Option<u64>,
}

/// Presence and outcome of each required artifact admission family. The
/// helper adapter must set other_required only after checking every further
/// gate that applies to the selected build, never as a default success.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateEvidence {
    pub elf: Option<bool>,
    pub flash_layout: Option<bool>,
    pub static_ram: Option<bool>,
    pub stack: Option<bool>,
    pub other_required: Option<bool>,
}
impl GateEvidence {
    fn all_passed(self) -> bool {
        [
            self.elf,
            self.flash_layout,
            self.static_ram,
            self.stack,
            self.other_required,
        ]
        .into_iter()
        .all(|gate| gate == Some(true))
    }
}
impl TargetMeasurements {
    fn complete(self) -> bool {
        self.static_ram_bytes.is_some()
            && self.flash_bytes.is_some()
            && self.core0_stack_bytes.is_some()
            && self.core1_stack_bytes.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactEvidenceState {
    NotMeasured,
    Matching,
    Stale,
}

#[derive(Clone, Copy, Debug)]
pub struct CurrentEvidence<'a, K, O> {
    pub selection: Option<&'a K>,
    pub digest: Option<&'a str>,
    pub identities: IdentityView<'a>,
    pub options: Option<&'a O>,
}
#[derive(Clone, Copy, Debug)]
pub struct CapturedAttempt<'a, K, O> {
    pub stamp: EditorStamp,
    pub selection: &'a K,
    pub digest: &'a str,
    pub revision: u64,
    pub identities: IdentityView<'a>,
    pub options: &'a O,
}
#[derive(Clone, Copy, Debug)]
pub struct ArtifactEvidence<'a, O> {
    pub digest: Option<&'a str>,
    pub revision: Option<u64>,
    pub identities: IdentityView<'a>,
    pub options: Option<&'a O>,
    pub measurements: TargetMeasurements,
    pub gates: GateEvidence,
}

/// One currentness decision for Resources and future Build & Flash readiness.
/// Device/connection/operation gates remain separate and cannot turn Stale
/// into Matching. Historical artifacts and jobs are never changed here.
pub fn match_artifact<K: PartialEq, O: PartialEq>(
    editor: &Editor,
    current: &CurrentEvidence<'_, K, O>,
    captured: &CapturedAttempt<'_, K, O>,
    artifact: Option<&ArtifactEvidence<'_, O>>,
) -> ArtifactEvidenceState {
    let Some(artifact) = artifact else {
        return ArtifactEvidenceState::NotMeasured;
    };
    let readiness = editor.action_readiness();
    let identities_match = current.identities.complete()
        && captured.identities.complete()
        && artifact.identities.complete()
        && current.identities == captured.identities
        && current.identities == artifact.identities;
    let digest_match = current
        .digest
        .is_some_and(|digest| !digest.is_empty() && digest == captured.digest && artifact.digest == Some(digest));
    let selection_match = current.selection.is_some_and(|key| key == captured.selection);
    let options_match = current
        .options
        .is_some_and(|options| options == captured.options && artifact.options == Some(options));
    if readiness.block.is_some()
        || readiness.stamp != captured.stamp
        || !selection_match
        || !digest_match
        || !identities_match
        || !options_match
        || editor.accepted_revision().get() != captured.revision
        || artifact.revision != Some(captured.revision)
        || !artifact.measurements.complete()
        || !artifact.gates.all_passed()
    {
        ArtifactEvidenceState::Stale
    } else {
        ArtifactEvidenceState::Matching
    }
}
