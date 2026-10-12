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
Only move, resize and delete commands are allowed while repairing that candidate;
each command runs the full shared validator. Partial repairs preserve accepted
bytes/history, and the first valid result becomes one undo transaction. Discard
restores the accepted selection. Rejected non-layout choices still require
discard before those settings can change. See [preview hardening](preview-hardening.md)
for the inspection-only rendering and safe fallback contract.

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
