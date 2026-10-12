# Transactional editor state

`configurator/src/editor.rs` is a native-testable Rust reducer shared by every
browser interaction. Every structural edit first builds and validates a
candidate. Failed collision, capacity, text, route, numeric or selection checks
leave the accepted document and its history unchanged.

The document, selected view/widget, raw invalid inputs, last valid preview,
32-entry undo/redo history and saved revision are distinct. Invalid partial
numeric text remains editable; it blocks export, saving and selection changes
until corrected or explicitly discarded. Related raw fields commit together
when the complete candidate becomes valid. JSON, device binary and saved-draft
round trips use the same bounded configuration codec as firmware.

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

Twenty-eight native contracts cover these transitions, all spans/display/fuel
options, every catalogue definition and route, strict malformed/oversized
imports, storage failures and binary equality after round trips. Strict Clippy,
formatting and the Wasm target also pass. Browser event binding and actual
storage behavior are verified separately in the browser stage.

The configurator is an independent Cargo workspace with a committed lockfile,
like `host` and `firmware`. It consumes the same shared path crates and inherits
the repository's pinned nightly. This isolates browser framework features and
release settings from firmware target/linker settings.
