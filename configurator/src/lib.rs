//! Framework-independent editor state; browser callbacks use this same reducer.
//!
//! `Editor::preview` always exposes a shared-validator-approved document. When
//! `has_invalid_draft` is true, inspectors display the retained `raw_fields` and
//! `draft_error`, and the canvas must identify its content as the last valid
//! preview. Correct or explicitly discard those fields before other edits.
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

pub mod editor;
pub mod geometry;
pub mod preview;
pub mod timing;

pub use dashboard_config as config;
