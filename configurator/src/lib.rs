//! Framework-independent editor state; browser callbacks use this same reducer.
//!
//! `Editor::preview` always exposes a shared-validator-approved document. When
//! `has_invalid_draft` is true, inspectors display the retained `raw_fields` and
//! `draft_error`. The canvas shows `diagnostic()` with an INVALID PREVIEW banner;
//! it cannot serialize or enter firmware. Correct or discard invalid raw fields;
//! move, resize or remove rejected widgets to repair a layout, or discard it.
//! `editing_config` is draft data and must never be used as a validation proof.
//!
//! Bind inspector callbacks to `field_token` and use `set_raw` or
//! `apply_if_current`. Refresh the binding after an accepted dispatch; a callback
//! from an earlier revision must not obtain a fresh token to bypass that check.
//! Pointer capture, hit testing, and grab offsets belong to the browser adapter;
//! the reducer verifies pointer identity, revision, cancellation, and placement.
//!
//! Check a browser file's size before reading it, then finish the corresponding
//! import request with its original token. Persistence adapters write the exact
//! `SaveRequest::json` bytes, check `save_is_current` just before writing, serialize
//! writes, and acknowledge only their actual result. Project restore captures the
//! original stamp and joins only if editing has not started; original backing is
//! pinned under the origin lock before the prepared editor is accepted.
//!
//! Inline JSON is an Editor-owned bounded transaction: `open_json` is clean,
//! the first `set_json_text` is pending, and delayed work finishes its original
//! parse token. Apply reparses and validates current text; Discard adds no undo.
//! Snapshot actions use `action_readiness`; ordinary mutations preserve their
//! pointer/import cancellation behavior but reject pending JSON or startup timing.
//! Startup timing owns bounded raw text until Apply/Discard and never publishes
//! invalid values to the accepted preview. Inspection is
//! pure and index-based, with guarded repair for retained raw entries only.
//! `stamp` tracks action provenance, `accepted_revision` tracks accepted state,
//! and `accepted_config_key` tracks canonical configuration transitions for the
//! persistent acquisition owner. They are distinct identities.

/// Bounded reusable appearance snapshots, preview sessions and explicit preset persistence.
pub mod appearance_presets;
/// Raw conversion-number drafts and bounded option admission.
pub mod artwork_draft;
/// Prototype adapter/profile configuration with ECU requests disabled.
pub mod ble;
/// Synthetic BLE fixture state distinct from real adapter/vehicle data.
pub mod ble_simulator;
/// Normalized firmware options and command/artifact presentation.
pub mod build_flash;
/// Once-per-cause cache identity exhaustion reporting.
pub mod cache_reporting;
/// Inspectable rejected configuration projection and affected-field context.
pub mod diagnostic;
/// Offline documentary evidence with current source-catalogue bindings.
pub mod ecu_reference;
/// Accepted projects, revisioned drafts, undo/redo and mutation authority.
pub mod editor;
/// Transition-based contextual diagnostics for editor and runtime adapters.
pub mod event_adapters;
/// Bounded severity/category event storage and filtering.
pub mod events;
/// Stable field descriptors, candidate-position targets and guarded repair lookup.
pub mod field_registry;
/// Checked callback leases and bounded scheduler ownership.
pub mod frame_callbacks;
/// Shared-grid hit testing, placement and drag thresholds.
pub mod geometry;
/// Validated helper sessions, frozen build inputs and explicit flash authorization.
pub mod helper_client;
/// Bounded structural comparison of serialized configuration projections.
pub mod json_diff;
/// Retained raw JSON, parse identities and read-only candidate inspection.
pub mod json_draft;
/// Shared orientation repacking and display-selection helpers.
pub mod orientation;
/// Shared Rust rendering with admitted/diagnostic/synthetic pixel ownership.
pub mod preview;
/// Cached accepted-project resource estimates and build artifact identity.
pub mod resources;
/// Private producer bridge feeding shared acquisition state.
mod simulation;
/// Persistent synthetic controls, acquisition and completed-display state.
pub mod simulator;
/// Monotonic playback and bounded interaction measurement ownership.
pub mod timing;
/// Revision-bound overlap recovery and widget context actions.
pub mod widget_actions;
/// Workspace/group preferences and checked command availability.
pub mod workspace;

pub use dashboard_config as config;

/// Publication identity gating canvas interaction after successful drawing.
pub mod canvas_frame;
/// Bounded browser-to-helper diagnostic batches and acknowledgment.
pub mod diagnostic_relay;
