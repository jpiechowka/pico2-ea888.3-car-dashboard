//! Accepted Studio serialization and borrowed build evidence. Target resource
//! measurements come only from an admitted helper artifact, never host layout.
use dashboard_config::*;

use crate::editor::{ActionBlock, Editor, EditorError, EditorStamp};
use crate::events::{Category, EventError, EventInput, EventSink, RecordOutcome, Severity};

/// Only transition summaries cross into the 512-record common event sink.
/// Helper job stdout/stderr and its 1,024-record per-job log stay separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostSummary {
    /// Helper connection was established.
    Connected,
    /// Helper connection was lost or explicitly closed.
    Disconnected,
    /// A helper operation entered a reported stage.
    StageStarted,
    /// A helper stage completed successfully.
    StageCompleted,
    /// A helper stage failed; detailed logs stay in the job store.
    StageFailed,
    /// A checked artifact became available for inspection.
    ArtifactAvailable,
}

/// `redacted_subject` comes from the helper's redaction boundary and is plain
/// text (for example a job reference or artifact link), never raw tool output.
/// Records typed `summary` in `sink` at monotonic `elapsed_ms` and optional Unix `wall_clock_ms`.
/// Returns the sink's retention/console decision or admission failure; this performs no console/helper I/O itself.
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
/// Current shared schema/storage capacities presented beside accepted serialization measurements.
pub struct ResourceLimits {
    /// Maximum encoded dashboard binary bytes.
    pub eav_bytes: usize,
    /// Maximum dashboard JSON input/output bytes.
    pub json_bytes: usize,
    /// Maximum editor selection/startup envelope bytes, excluding separately stored artwork.
    pub saved_bytes: usize,
    /// Maximum named views.
    pub views: usize,
    /// Maximum widgets across all views.
    pub widgets: usize,
    /// Grid cells in either orientation, always eight.
    pub cells_per_view: u8,
    /// Maximum stored view-name bytes.
    pub view_name_bytes: usize,
    /// Maximum stored widget-label bytes.
    pub widget_label_bytes: usize,
    /// Maximum stored warning-message bytes.
    pub warning_message_bytes: usize,
}
impl ResourceLimits {
    /// Capacities corresponding to the current bounded schema and editor envelope.
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
/// Accepted view geometry and grid occupancy, independent of host memory layout.
pub struct ViewUsage {
    /// Unique accepted view ID.
    pub id: u8,
    /// Effective orientation-derived grid/panel geometry.
    pub geometry: Geometry,
    /// Accepted view name copied for display.
    pub name: String,
    /// Number of widgets assigned to this view.
    pub widget_count: usize,
    /// Row-major occupied cells as an eight-bit mask in this view's geometry.
    pub occupied_mask: u8,
    /// Population count of occupied_mask, at most eight.
    pub occupied_cells: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Configuration-only source readiness; never proof of live ECU/adapter support or qualified data.
pub enum SourceSelectionStatus {
    /// Explicit Disabled catalogue entry.
    Disabled,
    /// Catalogue explicitly lacks an executable/qualified definition.
    Unavailable(
        /// Evidence-backed catalogue limitation.
        UnavailableReason,
    ),
    /// Polling is configured Off.
    PollOff,
    /// Addressing route is unknown.
    RouteUnknown,
    /// The configured decoder can be selected; live ECU/transport/capability
    /// and sample qualification remain runtime evidence, not config evidence.
    ExecutableSelectionUnqualified,
}

#[derive(Clone, Copy, Debug)]
/// Accepted source settings and catalogue evidence for one canonical channel.
pub struct SourceUsage {
    /// Dashboard canonical channel.
    pub channel: Channel,
    /// Accepted definition, addressing, polling and freshness settings.
    pub configured: ChannelSource,
    /// Static catalogue display name.
    pub catalogue_name: &'static str,
    /// Static evidence/interpretation boundary.
    pub catalogue_interpretation: &'static str,
    /// Primary/sanitized catalogue reference links, possibly empty.
    pub catalogue_references: &'static [&'static str],
    /// Catalogue limitation, or `None` without such a static blocker.
    pub catalogue_unavailable: Option<UnavailableReason>,
    /// Ordered configuration-only readiness classification.
    pub status: SourceSelectionStatus,
}

#[derive(Clone, Debug)]
/// Accepted widget readout/capability summary for Resources display.
pub struct WidgetUsage {
    /// Unique accepted widget ID.
    pub id: u16,
    /// Owning accepted view ID.
    pub view: u8,
    /// Accepted bounded user label, rendered as text.
    pub label: String,
    /// Canonical data channel.
    pub channel: Channel,
    /// Configured primary display-unit selection.
    pub readout: Readout,
    /// Retained optional boost companion-unit choice.
    pub secondary: Secondary,
    /// Actual renderer capabilities for current style/span, independent of retained preferences.
    pub capabilities: WidgetCapabilities,
}

#[derive(Clone, Debug)]
/// Exact accepted serialization sizes and bounded configuration usage at one editor action stamp.
pub struct ConfigResources {
    /// Editor owner/action identity at measurement.
    pub stamp: EditorStamp,
    /// Last accepted project/selection publication revision.
    pub accepted_revision: u64,
    /// Actual encoded dashboard binary length in bytes.
    pub eav_bytes: usize,
    /// Actual dashboard JSON length in UTF-8 bytes.
    pub json_bytes: usize,
    /// Actual saved selection/config/startup/artwork-identity envelope length in bytes.
    pub saved_bytes: usize,
    /// Saved envelope bytes minus standalone dashboard JSON bytes; checked against underflow.
    pub saved_overhead: usize,
    /// Current maximum schema/storage capacities.
    pub limits: ResourceLimits,
    /// Accepted view count.
    pub views_used: usize,
    /// Accepted total widget count.
    pub widgets_used: usize,
    /// Per-view grid occupancy in accepted order.
    pub views: Vec<ViewUsage>,
    /// Per-widget readout/capability summary in accepted order.
    pub widgets: Vec<WidgetUsage>,
    /// Source summaries in Channel order.
    pub sources: Vec<SourceUsage>,
    /// Validated fuel-assumption stoichiometric mass ratio used for AFR display.
    pub stoich_ratio: f32,
    /// Explicit accepted fuel assumption, not measured fuel composition.
    pub fuel: Fuel,
}
impl ConfigResources {
    /// Presentation relation only. A configured stoichiometric ratio is not
    /// evidence of the fuel actually in a vehicle.
    /// Returns equivalent AFR for dimensionless `lambda` by direct multiplication; nonfinite/overflow inputs propagate.
    pub fn afr_for_lambda(
        &self,
        lambda: f32,
    ) -> f32 {
        lambda * self.stoich_ratio
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Whether displayed resource measurements belong to currently admissible editor state.
pub enum ResourceAvailability {
    /// Measurements correspond to the current clean action stamp.
    Current,
    /// Historical accepted measurements remain available while current state is blocked.
    LastAccepted(
        /// Current action blocker requiring explicit historical labeling.
        ActionBlock,
    ),
    /// Current state is blocked and no historical accepted measurement exists.
    NotExportable(
        /// Current action blocker.
        ActionBlock,
    ),
}
#[derive(Clone, Debug)]
/// Labeled accepted-resource display result; blocked snapshots never become current by cache reuse.
pub struct ResourceReport {
    /// Current versus historical/unavailable admission status.
    pub availability: ResourceAvailability,
    /// Present with LastAccepted only as a labelled former accepted snapshot.
    pub accepted: Option<ConfigResources>,
}

#[derive(Default)]
/// One accepted-resource snapshot, reused only after live action readiness and stamp checks.
pub struct ResourceCache {
    /// Last successfully measured accepted state, retained during invalid/unapplied/busy state.
    accepted: Option<ConfigResources>,
}
impl ResourceCache {
    /// Returns current or explicitly historical measurements for `editor`, rechecking live readiness even at equal
    /// stamps. Serialization/admission measurement errors return `Err` and retain the preceding successful cache
    /// entry.
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

/// Returns actual serialized sizes and accepted configuration usage for currently admissible `editor`.
/// Admission/codec or envelope-size subtraction errors return no measurement. A panic would indicate a violated
/// ValidatedConfig view invariant; no target RAM/flash/stack measurements are inferred from host structures.
fn measure_ready(editor: &Editor) -> Result<ConfigResources, EditorError> {
    let readiness = editor.action_readiness();
    // Recheck even if the caller inspected before this function. Export methods
    // and the saved-envelope method also perform their own admission check.
    if let Some(block) = readiness.block {
        return Err(block.into());
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
            let geometry = config.view_orientation(view.id).expect("validated view").geometry();
            let mut occupied_mask = 0_u8;
            let mut widget_count = 0;
            for widget in config.widgets.iter().filter(|widget| widget.view == view.id) {
                widget_count += 1;
                occupied_mask |= widget.placement.occupied_in(geometry).unwrap_or(0);
            }
            ViewUsage {
                id: view.id,
                geometry,
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

/// Returns canonical internal unit text for `channel`, independent of display conversion choices.
pub const fn canonical_unit(channel: Channel) -> &'static str {
    match channel {
        Channel::Boost => "bar",
        Channel::Oil | Channel::Water | Channel::Dsg | Channel::Iat | Channel::Egt => "°C",
        Channel::Battery => "V",
        Channel::Lambda => "λ",
    }
}
/// Returns physical-toggle unit text: `psi` true selects PSI, false selects bar.
pub const fn boost_display_unit(psi: bool) -> &'static str { if psi { "PSI" } else { "bar" } }

/// Pure borrowed projection over helper-owned identities; never a protocol
/// DTO. None or empty is missing authority, not a wildcard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentityView<'a> {
    /// Source-inventory identity, or `None` if unavailable.
    pub source_inventory: Option<&'a str>,
    /// Compiler identity, or `None` if unavailable.
    pub compiler: Option<&'a str>,
    /// Toolchain identity, or `None` if unavailable.
    pub toolchain: Option<&'a str>,
    /// Relevant build/validation tool identity, or `None` if unavailable.
    pub tools: Option<&'a str>,
}
impl IdentityView<'_> {
    /// Returns true only when every identity is present and nonempty; does not independently verify hashes.
    fn complete(self) -> bool {
        [self.source_inventory, self.compiler, self.toolchain, self.tools]
            .into_iter()
            .all(|value| value.is_some_and(|value| !value.is_empty()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Optional artifact-derived linked-image resource measurements; unavailable never means zero.
pub struct TargetMeasurements {
    /// Linked static RAM bytes, if admitted evidence supplies them.
    pub static_ram_bytes: Option<u64>,
    /// Linked flash footprint bytes, if admitted evidence supplies them.
    pub flash_bytes: Option<u64>,
    /// Core0 software stack admission bound in bytes, not physical high-water.
    pub core0_stack_bytes: Option<u64>,
    /// Core1 software stack admission bound in bytes, not physical high-water.
    pub core1_stack_bytes: Option<u64>,
}

/// Presence and outcome of each required artifact admission family. The
/// helper adapter must set other_required only after checking every further
/// gate that applies to the selected build, never as a default success.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateEvidence {
    /// ELF structure/content gate outcome, or `None` without evidence.
    pub elf: Option<bool>,
    /// Flash layout/reserved-range gate outcome, or `None` without evidence.
    pub flash_layout: Option<bool>,
    /// Static RAM/headroom gate outcome, or `None` without evidence.
    pub static_ram: Option<bool>,
    /// Software stack-bound gate outcome, or `None` without evidence.
    pub stack: Option<bool>,
    /// All further applicable gates' aggregate outcome; never inferred from absence.
    pub other_required: Option<bool>,
}
impl GateEvidence {
    /// Returns true only when every required gate explicitly passed.
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
    /// Returns whether every resource measurement is present; zero remains a supplied value, not absence.
    fn complete(self) -> bool {
        self.static_ram_bytes.is_some()
            && self.flash_bytes.is_some()
            && self.core0_stack_bytes.is_some()
            && self.core1_stack_bytes.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Display relation between borrowed artifact evidence and the current accepted build selection.
pub enum ArtifactEvidenceState {
    /// No artifact evidence was supplied.
    NotMeasured,
    /// Complete identities/options/revision/measurements and all gates agree with current admission.
    Matching,
    /// Artifact exists but at least one required identity, outcome or currentness check failed/missing.
    Stale,
}

#[derive(Clone, Copy, Debug)]
/// Live selected build identity; missing fields fail matching rather than acting as wildcards.
pub struct CurrentEvidence<'a, K, O> {
    /// Current selected recipe/build key, or `None`.
    pub selection: Option<&'a K>,
    /// Current accepted content digest, or `None`.
    pub digest: Option<&'a str>,
    /// Current source/compiler/toolchain/tool identities.
    pub identities: IdentityView<'a>,
    /// Current normalized build options, or `None`.
    pub options: Option<&'a O>,
}
#[derive(Clone, Copy, Debug)]
/// Immutable build-attempt context captured before asynchronous helper work.
pub struct CapturedAttempt<'a, K, O> {
    /// Editor owner/action identity captured at dispatch.
    pub stamp: EditorStamp,
    /// Selected recipe/build key captured at dispatch.
    pub selection: &'a K,
    /// Accepted content digest captured at dispatch.
    pub digest: &'a str,
    /// Accepted publication revision captured at dispatch.
    pub revision: u64,
    /// Frozen source/compiler/toolchain/tool identities.
    pub identities: IdentityView<'a>,
    /// Normalized options captured at dispatch.
    pub options: &'a O,
}
#[derive(Clone, Copy, Debug)]
/// Borrowed artifact report; authority depends on complete matching and explicit gate evidence.
pub struct ArtifactEvidence<'a, O> {
    /// Reported content digest, or `None` when absent.
    pub digest: Option<&'a str>,
    /// Reported accepted publication revision, or `None`.
    pub revision: Option<u64>,
    /// Reported source/compiler/toolchain/tool identities.
    pub identities: IdentityView<'a>,
    /// Reported normalized build options, or `None`.
    pub options: Option<&'a O>,
    /// Linked target resource evidence with unavailable values preserved.
    pub measurements: TargetMeasurements,
    /// Explicit admission outcomes for every required gate family.
    pub gates: GateEvidence,
}

/// One currentness decision for Resources and future Build & Flash readiness.
/// Device/connection/operation gates remain separate and cannot turn Stale
/// into Matching. Historical artifacts and jobs are never changed here.
/// Returns NotMeasured for absent `artifact`, otherwise Matching only when `editor` readiness and `current`
/// selection agree exactly with `captured` and complete artifact identities/options/revision/gates/measurements.
/// Any missing/empty or differing authority returns Stale; no device, electrical or physical performance claim follows.
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
