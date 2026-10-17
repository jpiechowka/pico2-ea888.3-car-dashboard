# Studio workspace and diagnostics

This is the implementation contract for approved decisions D8, D9 and D10. It extends the Rust/Leptos editor after the shared controls and advanced widget runtime. Workspace, appearance, diagnostics, simulator and JSON editing paths have native and browser integration evidence. The later native Build & Flash and ECU Reference workspaces are implemented; their current workflows are documented in [building](building.md) and [ECU Reference](ecu-reference.md). The imperative language below retains the behavioral contract; dated verification records apply to their frozen inputs, not subsequent features.

## Navigation and visual direction

Keep the existing charcoal surfaces, restrained red accents, pixel-accurate Pico canvas and typography. The central visual remains the configured instrument display. Use the current CSS variables and local font stacks; no remote fonts or new JavaScript application library.

The primary navigation has six workspaces:

| Workspace | Contents |
| --- | --- |
| Views | Named views, widget library, drag/drop canvas, selected-widget inspector, appearance copy/presets |
| Device | Per-screen buttons and encoder, protected recovery, normal/warning RGB behavior, reduced motion, startup and artwork |
| Data & warnings | Channel sources, polling/freshness, channel history, fuel/AFR basis, combined warnings and a rule summary |
| Diagnostics | All-screen simulator, event log, performance trace, JSON editor/diff, configuration and source inspector |
| Build & Flash | Local prerequisites, profiles, accepted-project builds, admitted firmware, explicit device inspection and transfer/verify/reboot, logs and retained build storage |
| ECU Reference | Offline research, source/decoder details, evidence and qualification limits |

Workspace navigation never clears a draft, resets the simulator, changes the selected firmware view or accepts an edit. Store the active workspace and expanded groups as browser preferences separate from the exported firmware configuration. A missing or invalid preference falls back to Views. Moving between firmware screens inside the simulator preserves that simulator's current view and controls.

The Views layout keeps its library / canvas / inspector relationship. Device and Data use a wider settings area with a compact persistent preview; Diagnostics and Build & Flash use the width for tables and logs. At widths over 1100 CSS pixels the non-Views preview stays beside the controls with bounded scrolling. At narrower widths it follows the settings in one reading column; collapsed simulation channel groups and a sticky link in the controls provide a way to reach the live preview. Returning from that preview to the controls still requires page scrolling. Every pane has `min-width: 0`, explicit scrolling for long data, and controls that remain reachable at 320 CSS pixels. Keep one full-size canvas and one simulation owner.

Group widget settings by Layout, Readout, Gauge, History & statistics, Warnings and Appearance. Show commonly changed fields immediately; advanced groups expand on demand. Disabled controls explain their channel, size or style restriction. A hidden group must open when a validation link targets one of its fields. Retain the overlap context menu and keyboard movement.

Use semantic tab controls with an active tab stop, arrow/Home/End navigation and Enter/Space activation. Panels retain their state while inactive; inactive controls are not focusable. Follow the [WAI tabs pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/). Use text and icons alongside color for invalid, running and completed states.

## Search, commands and error navigation

A single registry describes dashboard configuration fields: stable key, workspace/group, label, aliases, applicability, DOM target and optional widget/channel identity. The inspector, search and error navigator consume this registry. Startup/artwork and build options currently use their dedicated panels, without field-search entries. Labels such as boost, AFR, lambda, PSI, smoothing, stale and warning find their related registered fields. Limit search input to 128 UTF-8 bytes and show at most 40 results with an explicit remaining count.

The command palette opens with Ctrl/Cmd+K or its visible button. It includes workspace navigation, find setting, duplicate selected widget/view, copy/apply appearance, undo/redo, open JSON, export and simulator recovery. Mutation commands use the same reducer and validation gates as their visible buttons. Availability is recalculated before dispatch; stale selections cannot target replacement widgets. A command cannot execute an arbitrary URL, shell command or expression. Flash remains an explicit action in its workspace with a selected device and current artifact.

Search and inspection present resolved human labels rather than internal owner/debug syntax. LED contexts read Normal operation and Warning; event categories read Import / export and Host tools. Inspection shows a JSON path or known view/widget/channel index when available, without inventing an absent location. Changed accepted edits clear stale inspection detail; invalid raw inspection remains available for repair.

Use a bounded native dialog with a visible close button, Escape, focus containment and focus restoration. [WAI dialog guidance](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/) supplies the keyboard/focus contract. Ignore global editing shortcuts while composing text or inside an input, textarea, select or editable region. Ctrl/Cmd+Z in the JSON textarea edits its text; it must not undo the accepted dashboard. Keep browser reload, address bar and other standard browser commands intact.

The error navigator shows bounded shared validation diagnostics, raw parse errors and the current JSON syntax location. Semantic validation retains one source of truth: extract checks into a visitor that can stop at the first error for firmware or collect up to 128 entries for Studio, with a visible omitted count. Preserve the existing first-error order. Invalid references or geometry skip dependent accesses rather than indexing nonexistent views or shifting invalid cells. Do not obtain extra errors by repeatedly mutating the candidate to suppress earlier failures.

Each error links to a stable field or widget. Duplicate or missing identifiers link to the JSON location and list position rather than selecting the wrong object. Navigation is permitted while invalid, but mutation rules remain explicit. Keep the accepted configuration, inspectable candidate and invalid text separate. Selection needed to inspect another invalid widget must not accidentally accept or erase a candidate. The prominent INVALID PREVIEW marker stays visible until all blocking errors are repaired or discarded.

## Appearance operations

An appearance preset contains widget colors, gauge style, decimal precision, and supported history/average/extrema/trend visibility only. It does not contain widget IDs, placement, units, gauge bounds, band/reference values, smoothing/peak timings, source definitions, warning enablement/rules, fuel assumptions, polling or device controls. This positive allowlist prevents applying a boost range or reference to a temperature widget. Provide the existing default palette, a high-contrast monochrome preset and a red-accent preset, all checked after RGB565 quantization.

Copy Appearance stores a typed snapshot in memory. Apply to selected / this view / all widgets is one atomic reducer edit and one undo entry. A small preview lists the target count and incompatible fields. Apply only common supported fields; list omissions before committing. The operation never changes a warning threshold or disables a warning. Colors for the combined warning popup remain a separate global control.

A user can name and save appearance presets in a separate versioned browser record: at most 16 presets, 32 visible ASCII characters per name, 32 KiB total storage. Imported or stale preset records pass the same color/style validators. Browser storage failure leaves the in-memory preset usable and offers export; a failed save never displays Saved. No preset can bypass current configuration validation or carry executable content.

The Web Locks promise and its storage callback can resolve in either order. A rejected lock reports a save failure only while that request is still current; a later rejection does not duplicate the callback's already reported outcome. This ordering has a native regression test; forced browser permission or promise-order failures were not injected during integration QA.

## Diagnostics and console logging

Implement the event model in framework-independent Rust and the browser adapter with [`web_sys::console`](https://docs.rs/web-sys/latest/web_sys/console/index.html). No JavaScript logging framework or remote telemetry is required.

Store at most 512 events. An event has a monotonically issued sequence, elapsed milliseconds, optional wall-clock timestamp, severity, a fixed category and a bounded message. Categories are Editor, Validation, Storage, Import/export, Simulation, Rendering, Host tools and Device. Message storage is capped at 512 UTF-8 bytes with explicit truncation; do not slice inside a code point. Record dropped and repeated-event counts separately. Sequence exhaustion stops accepting new entries and reports that state without wrapping into old identities.

Warnings and errors are enabled in the browser console by default. Routine Info events remain in the bounded event history. Debug/trace console output is opt-in and limited to 10 events per second. Repeated identical warning/error causes are coalesced by a bounded key table with a count and last occurrence; new distinct errors remain visible. Suppression and rate-limit summaries are observable. Do not log each animation frame, sample, keystroke or successful PWM write by default.

Useful events include initialization/fallback, configuration acceptance/rejection, import/export outcome, persistence conflict/failure, renderer failure/recovery, control recovery, simulator source/fault changes, reset/ack outcome and helper connection/stage transitions. Successful configuration changes log the affected field/category and revision, not the full JSON. Never log local helper capabilities, full environment variables or credentials. File paths in displayed/exported helper events use the helper's redacted form.

Filtering, text search, pause scrolling, clear, copy and JSON-lines export operate on the retained records. A paused log view shows a new-event count and preserves the anchor until that record is evicted; eviction is indicated. Clear does not reset sequence identity. Render event data as escaped text. A malformed export, missing clipboard permission or download error becomes a visible bounded event and notice without recursive logging.

The existing 512-input performance trace remains separate from the event log. Fix counter exhaustion to produce an explicit stopped state, not a panic. Keep measured browser input-to-presentation-opportunity time distinct from physical display FPS. Add no allocation or logging to firmware inner pixel loops.

Firmware retains its existing fixed log storage. Add transition/error records where they help reproduce faults: configuration fallback, recovery, source quality transitions, warning ack rejection, clock selection and exceptional renderer/transfer state. Use existing bounded messages, change detection and suppression for repetitive faults. Preserve latest-fault storage even if the ordinary ring is full. No nested logging lock and no logs inside a critical section or DMA interrupt.

## All-screen simulator

Extend the existing `ControlSimulator`; do not implement a second button map, gesture recognizer, brightness controller, warning engine or LED policy. The browser owns synthetic inputs, time scheduling and frame-completion injection. Shared code owns the resulting state.

Extract Debug-page formatting/drawing from `firmware/pico2/src/screens/profiling.rs` into a shared renderer with small line/section methods. Firmware retains cooperative `RenderWork` checkpoints between those methods. Logs already uses shared `LogPage`; reuse it. Dashboard uses the configured scene and acquisition model. A simulator caption outside the display marks every screen as synthetic. Hardware-only values in simulated Debug are unavailable or explicitly simulated, never inferred as measured RAM, cycles, voltage or scanout.

Remove hardcoded control hints such as `Y: Logs` once per-screen remapping is active. Render an applicable short hint from the current map, while always documenting fixed Y-hold recovery. Long strings, maximum numeric fields and unavailable metrics are clipped to their column/row and the screen boundary. Keep the distinction between observed transfer completion and physical scanout in Debug.

Provide Dashboard/Debug/Logs navigation through the configured buttons/encoder, physical Tap/Hold/Release controls, continuous held-key testing, simultaneous buttons, encoder motion/clicks, protected recovery and reset acknowledgment. DOM pointer cancellation, blur and hidden-tab transitions release simulated physical inputs and cancel pending browser callbacks. Monotonic simulated elapsed time drives controls and animations; do not infer a long hold from a later release or run an unbounded catch-up loop.

The regular live simulator uses a persistent acquisition owner. Configuration replacement, source changes and an explicit Restart have named reset behavior. Switching workspace or firmware screen does not restart it. Viewing a rejected configuration is labeled INVALID SIMULATION: accepted acquisition/source settings remain in force, and bounded candidate controls/presentation are inspectable without becoming a firmware configuration.

Add named synthetic scenarios: Normal demo, Warnings, Channel minima/maxima, Width stress, Missing data, Stale data, Source fault, Delayed reset acknowledgment and Dropped display completion. Per-channel custom finite inputs are bounded by the existing ingestion gate. Out-of-domain, NaN/Infinity and backdated events can be injected as rejected samples for diagnostics; they must not become accepted ECU data. Source/fault behavior must use the same acquisition methods as firmware.

Completion simulation is explicit. Normal mode submits a frame and reports its completion before applying presented-state effects. Dropped/delayed completion leaves the previously completed state authoritative: no premature LED switch or backlight Off. Show current and pending frame identity in Diagnostics. Off remains a black display with accessible recovery controls outside the canvas. A simulator completion is not evidence that a real panel displayed those pixels.

The preview caption identifies synthetic pixels and frame completion separately from the Appearance preview's render time. It does not display the Appearance timing as physical scanout or simulator FPS.

## JSON editing, diff and resources

Use a Rust-controlled textarea with a monospace font, line/column diagnostics and an explicit Apply action. A local Wrap long lines option changes textarea presentation only; the text, native caret/undo and parsing remain the same. Do not add a JavaScript code editor. Limit input to 128 KiB before parsing, preserve serde's recursion bound and cap any diagnostic or diff expansion. Keep the original input visible on failure; oversized pastes are rejected with the previous text intact.

Opening JSON starts a draft tied to the current editor identity and revision. Typing does not modify accepted state or undo history. After a short bounded debounce, a structurally parseable candidate feeds automatic diagnostic preview; invalid semantic candidates are clearly marked INVALID PREVIEW. Syntactically invalid JSON shows its exact line/column and the last accepted display with an INVALID JSON notice. Never display an older successful candidate as though it represents the current invalid text.

A parsed, valid but unapplied candidate shows UNAPPLIED JSON PREVIEW. Save/export/build are blocked while any JSON edit is pending, valid or invalid, to prevent exporting different bytes than the user is inspecting. Apply rechecks the owner/revision and full shared validation, then makes one atomic accepted change and one undo entry. Discard restores accepted preview. Visual mutation while a JSON draft is pending asks the user to Apply or Discard in the editor; navigation and inspection remain available. A stale parse callback cannot replace newer text or an imported/replaced editor. Do not silently autosave raw invalid JSON as firmware configuration.

Parsing inspection must not provide an unchecked `ValidatedConfig` route. A host-only unvalidated JSON parse result may expose `Config` for diagnostics; export and firmware entry still require shared validation. Legacy file imports continue through frozen migration. The inline editor starts from the current version and clearly requests ordinary Import for an older schema.

The diff compares JSON fields structurally using JSON-pointer paths and stable view/widget IDs when unique. Do not run an unbounded line-LCS over 128 KiB. Show at most 512 changed paths and 256 UTF-8 bytes per before/after value, with omitted counts and access to the full bounded text. Reordered arrays are distinguished from changed values. Duplicate IDs retain array indexes and are marked ambiguous. Unknown fields, duplicate JSON properties and capacity errors remain blocking parse diagnostics.

The resource inspector shows actual current encoded EAV bytes, compact JSON/saved-envelope bytes, 8-view/64-widget counts, effective 4x2 landscape or 2x4 portrait occupancy, text limits, gauge capabilities, decoded source/route/poll/freshness/evidence and canonical/display unit relationships. Invalid candidates show blocked or candidate-only values, not a successful export size. No arbitrary decoder expressions are evaluated; supported decoders remain the researched catalogue.

The resource panel reports configuration sizes and fixed layout limits and links to Build & Flash for admitted firmware. That workspace binds artifact eligibility to project/options/session identities and marks changed inputs. It does not yet project all SRAM/flash/stack manifest fields into the resource panel. Any future projection must also bind source, toolchain, profile and features. Host/Wasm `size_of` values are not Pico SRAM admission. Physical FPS, voltage accuracy, interrupt overlap, long-duration stability and ECU qualification remain hardware checks.

## Verification boundaries

Native tests cover candidate/accepted/JSON transitions, stale callbacks, invalid navigation/repair, atomic multi-target appearance edits, presets and storage limits, validation collection order, log truncation/eviction/coalescing/counters, simulator time/press/recovery/reset/ack/fault/completion traces and source identity replacement. Pixel tests compare all three shared screens with extremes, missing values, long hints and both framebuffer histories.

Browser checks exercise keyboard focus, modal/shortcut interactions, hidden panels, overlap repair, inline JSON failure and recovery, console output/rate limiting, export/clipboard/storage failures, reduced motion, hidden-tab time and desktop/narrow layouts. Use the existing production Trunk build and preserve screenshots/traces as ignored evidence. Recheck the standard traced Demo and Vehicle target builds after extracting shared Debug rendering; stack/resource gates and cooperative-yield behavior still apply.

The final audit traces edit -> validation -> preview -> export -> embedded load -> input/acquisition -> rendering -> completion -> LED/backlight, including all page transitions. The independent implementation plans name the exact tasks and tests before each stage starts.

### Workspace integration evidence and limits, 2026-10-14

- All default root, Configurator and Host suites passed: 118 + 323 + 477 tests, with zero failures or ignored tests. Strict native and WASM Clippy, WASM type check and formatting passed. The retained-frame fixture compares complete 320×240 RGB565 buffers on both dirty histories through Dashboard -> Debug -> Logs -> Dashboard, including the three Debug brightness labels, changed text and a remapped hint. It does not exercise the backlight state machine.
- The actual Studio launcher built an isolated release. In the available IAB browser, 1440/768/320 CSS pixel layouts had one intrinsic 320×240 canvas and no page horizontal overflow. Workspace switching kept simulator state. Actual Full/Dim/Off completion and protected Y recovery, dropped completion, source fault and delayed reset were observed in the browser. JSON wrap, valid pending/invalid repair/Discard, tab focus/activation, resolved search/inspection labels, event filtering, copy and JSONL download were exercised. The 768/320 preview is reachable in vertical flow, with page scrolling needed to return to controls.
- A four-case, 30-sample-per-case host Criterion capture provides initial shared Debug and hinted Logs renderer baselines. They are host timings, not measured gains or physical FPS. The prior traced Demo/Vehicle firmware artifacts were reused only after declared inputs, compiler and artifact identities matched; no new ARM build or physical measurement was made. The Vehicle predecessor had 65,968 B static headroom, 432 B above its 65,536 B floor.
- Browser tooling did not expose 200% zoom, Chrome/Firefox or native app control. Forced storage quota/security/corruption, Web Locks ordering, canvas allocation/consumer/timer faults, hidden/disposal/IME and atomic mid-drag pointer cancellation were not browser-injected. Native/source checks do not turn those into browser passes. Reduced-motion and the complete warning/latch/snooze matrix were not repeated as a full browser matrix in this integration gate.
- At this October 14 checkpoint Build & Flash was reserved. Its later implementation does not change these historical test results. Hardware display, GPIO/PWM, scanout, voltage, ECU behavior, electrical safety and soak qualification remain unverified.
