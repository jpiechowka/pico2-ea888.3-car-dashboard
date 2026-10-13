# Transactional editor state

`configurator/src/editor.rs` is a native-testable Rust reducer shared by every
browser interaction. Every structural edit first builds and validates a
candidate. Failed collision, capacity, text, route, numeric or selection checks
leave the accepted document and its history unchanged.

The document, selected view/widget, raw invalid inputs, rejected typed candidate,
validated rendering state,
32-entry undo/redo history and saved revision are distinct. Invalid partial
numeric text remains editable; it blocks export, saving and selection changes
until corrected or explicitly discarded. Related raw fields commit together
when the complete candidate becomes valid. JSON, device binary and saved-draft
round trips use the same bounded configuration codec as firmware.

Rejected typed changes now retire callbacks and block exports/saves too, while
leaving the accepted document/history intact. The browser automatically renders
an explicitly marked diagnostic draft. Raw fields can be corrected together.
Rejected typed candidates retain their own selection and expose unvalidated
`editing_config()` to the browser hit targets, pointer anchors, menu and inspector.
Move, resize, delete, control mapping and widget/global field commands are allowed while repairing that candidate;
each command runs the full shared validator. Partial repairs preserve accepted
bytes/history, and the first valid result becomes one undo transaction. Discard
restores the accepted selection. Control mappings are read from the rejected
candidate, and hold-duration text remains editable while mappings are rejected.
Retained unrelated raw inputs remain editable against the current candidate too;
they must still identify an existing, applicable field in that candidate.
Field and mapping repairs replay retained raw inputs and identify any parse blocker;
they never clear an unrelated invalid setting. New fields must identify existing
applicable targets. Reference mode/value, bands, warning endpoints, shared
history and LED settings stay repairable. Removing a band deliberately removes
its raw fields and reindexes later band drafts, preserving unrelated text.
Parsed raw fields awaiting only placement repair permit layout repair; incomplete
or otherwise invalid raw fields keep layout locked. Each partial repair retires
stale callbacks without changing accepted bytes/history. See [preview hardening](preview-hardening.md)
for the inspection-only rendering and safe fallback contract.

A setting switch that would remove the input for a retained raw edit is refused
before changing its current option. For example, Fixed graph limits remain
visible until repaired before Automatic can be selected; fuel inputs retain
their ratio/blend/reference variant while their raw edits await repair. The
message names the field to resolve. That refused switch preserves revision,
tokens and history; correcting the complete draft retires earlier callbacks and
creates the one accepted repair transaction.

Style changes retain repair controls for graph/gauge preferences while raw
edits or a precise shared field error still require them. Disabling history or
warnings cannot hide their retained failing numeric inputs. The pure
`widget_fields_need_repair` query supplies the inspector reveal seam for
validation errors that have no raw text, including inactive stored fields.
A future JSON candidate adapter must select the error's widget/view and keep
the reported field's control or applicable option control mounted before focus;
the accepted configuration proof and export gates must remain strict.

Pointer, field, import and save operations carry private owner/revision/request
tokens. Old callbacks cannot change a newer selection, undo result, imported
document or replacement editor. A pointer also retains its original pointer ID.
The browser checks a prepared save immediately before writing, then acknowledges
the actual result. A late successful old write records the older stored revision
and cannot leave a newer document falsely marked Saved. Quota/access failures
do not prevent a valid file export.

Deleting the selected view selects the next surviving ordered view; deleting
another view preserves the current selection. Removing the boot view chooses
the first survivor. The final view cannot be deleted. Duplicates receive fresh
IDs; capacity failures are atomic. Signed-zero payloads are preserved during
otherwise-equal imports rather than lost through floating-point equality.

Native contracts cover these transitions, all spans/display/fuel
options, every catalogue definition and route, strict malformed/oversized
imports, storage failures and binary equality after round trips. Strict Clippy,
formatting and the Wasm target also pass. Browser event binding and actual
storage behavior are verified separately in the browser stage.

JSON parsing enables Serde's direct f32 rounding. Raw decimal input, imported
JSON and restored drafts therefore produce the same device bytes even around
floating-point midpoints. Four precision regressions also check that rounding
cannot incorrectly turn an equal graph range into an accepted interval.

The configurator is an independent Cargo workspace with a committed lockfile,
like `host` and `firmware`. It consumes the same shared path crates and inherits
the repository's pinned nightly. This isolates browser framework features and
release settings from firmware target/linker settings.
