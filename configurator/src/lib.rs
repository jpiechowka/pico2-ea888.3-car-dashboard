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
//! writes, and acknowledge only their actual result. Restoring browser storage
//! uses `from_saved_state` before mounting the interactive editor.
//!
//! Inline JSON is an Editor-owned bounded transaction: `open_json` is clean,
//! the first `set_json_text` is pending, and delayed work finishes its original
//! parse token. Apply reparses and validates current text; Discard adds no undo.
//! Snapshot actions use `action_readiness`; ordinary mutations preserve their
//! pointer/import cancellation behavior but reject JsonPending. Inspection is
//! pure and index-based, with guarded repair for retained raw entries only.
//! `stamp` tracks action provenance, `accepted_revision` tracks accepted state,
//! and `accepted_config_key` tracks canonical configuration transitions for the
//! persistent acquisition owner. They are distinct identities.

pub mod appearance_presets;
pub mod cache_reporting;
pub mod diagnostic;
pub mod editor;
pub mod event_adapters;
pub mod events;
pub mod field_registry;
pub mod frame_callbacks;
pub mod geometry;
pub mod json_diff;
pub mod json_draft;
pub mod preview;
pub mod resources;
mod simulation;
pub mod simulator;
pub mod timing;
pub mod widget_actions;
pub mod workspace;

pub use dashboard_config as config;
