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

## Browser and dependency checks

The [MDN contextmenu reference](https://developer.mozilla.org/en-US/docs/Web/API/Element/contextmenu_event)
describes pointer and keyboard invocation. Firefox deliberately keeps its native
menu for Shift+right-click. Shift+F10 and the context-menu key open Studio actions.
The [WAI menu pattern](https://www.w3.org/WAI/ARIA/apg/patterns/menubar/)
guides focus, arrow navigation, Home/End, Escape and Tab. Disabled directions use
`aria-disabled` and remain focusable; their commands cannot move beyond the grid.

Locked dependency source checks: Tachys 0.2.19 maps `context menu` to
`web_sys::MouseEvent`; the PointerEvent browser subtype retains those coordinates.
Leptos DOM 0.8.8 returns removable window-listener handles; resize, scroll and blur
handlers are removed during owner cleanup. Tachys reactive styles use
`RenderEffect` (reactive_graph 0.2.15), whose subsequent updates run asynchronously.
Menu focus therefore waits for the next animation boundary after visibility is
set, and checks that its element and menu still exist. Deferred focus after an
action also checks editor revision and that another menu has not opened.
Menu placement uses the root element's
[client dimensions](https://developer.mozilla.org/en-US/docs/Web/API/Element/clientWidth),
which exclude scrollbars; percentage CSS bounds cap the menu before measurement.

The independent review reproduced two issues during development: focus before
visibility was applied, and movement that left selection on the old widget.
Both were corrected; placement now selects its target and undo restores the
previous accepted selection. Native tests cover accepted and rejected movement.

## Verification, 2026-10-12

- 73 configurator native tests pass, including five overlap-recovery and six
  context-action contracts. Exhaustive cell/size nudging includes invalid u8
  coordinates; hit tests include view isolation, half-open edges and nonfinite
  coordinates. Partial repairs, rejected new widgets, cancellation, discard,
  single-step undo/redo, old saves and stale menu/pointer/field tokens are covered.
- Strict native and Wasm Clippy, formatting and the locked Trunk release build
  pass. No firmware/shared-renderer source or dependency version changed.
- Real browser: resize Boost to 2×2 over IAT, open the overlap menu, select the
  covered IAT, drag it to a free column. The larger Boost survives; INVALID and
  export gating clear only on a valid layout. Undo restores both old placements;
  redo restores both repaired placements. A second real 2×2 drag into IAT is
  repaired through the menu's Move left action.
- Right-click and Shift+F10, first-item focus, disabled boundary commands,
  Up/Down/Home/End navigation, Escape/Tab, all four movement commands, Delete
  and undo were exercised. Movement keeps inspector selection on its target.
  Invalid raw labels still lock layout controls and export until corrected.
- At 390×700, the menu remains inside the scrollbar-excluded 375-pixel width.
  At 320×220, its 204-pixel height fits with 8-pixel margins; End reaches Delete
  by scrolling the menu without dismissing it. Resizing dismisses the old menu.
  Browser error/warning logs were empty. Test view removed; default 466-byte
  Overview restored and viewport override reset.

These are host/browser checks. They make no hardware, ECU or physical FPS claim.
