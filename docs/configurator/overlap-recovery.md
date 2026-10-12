# Overlap recovery and widget actions

## Design and implementation order

The visible rejected layout must remain repairable. The last validated document
continues to govern exports, persistence and undo; invalid preview pixels never
become firmware configuration.

1. Reproduce the rejected-resize drag lock in native reducer tests. Store draft
   selection with the rejected configuration, expose its geometry for editing,
   and permit move, resize and delete operations on that draft. Validate every
   operation. Partial repairs retain INVALID PREVIEW and export/save gates; the
   first valid result becomes one undoable transaction. Cancellation preserves
   the draft; discard restores accepted selection and configuration. Raw invalid
   text must still be corrected before layout operations.
2. Use the same draft geometry for canvas hit targets, pointer anchors, drop
   guides and inspector placement controls. Preserve pointer/field/save/import
   revision guards. Include widgets that exist only in the rejected draft.
3. Add a Rust context-menu component: right click or Shift+F10 opens actions for
   the selected widget. Offer Delete and Move left/right/up/down; prevent moves
   beyond the grid. List widgets under the clicked point so a fully covered
   widget can be selected and raised for dragging. Clamp the menu to the viewport;
   support keyboard navigation, Escape/outside dismissal and stale-menu retirement.
4. Test resize/move/add collisions, incremental recovery, cancellation, deletion,
   undo/redo, stale callbacks, bounds and overlapping hit selection. Run native
   tests and strict native/Wasm lints, build release assets, reproduce and repair
   the overlap through the real browser, and inspect narrow-screen menu behavior.
5. Independently review state transitions, commit in stages, push master and
   verify the exact CI run. Record evidence and keep TODO/audit files uncommitted.

This work changes the browser editor only. It retains the shared configuration
validator and does not alter firmware rendering, hardware control or ECU access.
