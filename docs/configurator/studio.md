# EA888 View Studio

The editor is a static Rust/Leptos application compiled to WebAssembly. It runs
the same validator, acquisition simulation, warning pager and pixel renderer as
the firmware. HTML and CSS provide the surrounding workbench; wasm-bindgen and
Trunk generate the browser loader. There is no authored JavaScript, Node/npm
toolchain or account. Editing stays in the browser. Optional Build & Flash uses
an authenticated local Rust helper; accepted build inputs are sent only to that
helper on this computer.

## Start on Windows

From the repository root:

```powershell
./configurator/Studio.ps1 -Setup
./configurator/Studio.ps1 -Check
./configurator/Studio.ps1 -Serve
```

Open `http://127.0.0.1:8080`. Explicit Setup installs the pinned toolchain and
repository-local tools; Check reports remaining prerequisites. Normal builds
and serving run offline and do not install missing tools. Use `-Port 8081` for
editing if necessary; the native helper accepts only port 8080. Stop the
foreground server with Ctrl+C. See [setup details](setup-tools.md).

For a static release directory only, omit `-Serve`. Output is
`configurator/dist`. On another platform, enter `configurator`, install
`trunk --version 0.21.14 --locked` with Cargo, add the `wasm32-unknown-unknown`
Rust target, and run `trunk serve --release --locked`. If `NO_COLOR` is set,
Trunk expects `true` or `false`, not `1`.

Serve the files over HTTP on localhost or HTTPS on a static host. Opening
`index.html` directly as a local file is not supported. Browser saving requires
Web Locks and permitted local storage; file export remains usable without them.

## Build views

**Device controls** edits A/B/X/Y tap and hold bindings independently on
Dashboard, Debug and Logs, plus encoder rotation and click. Menus use the shared
action catalogue and offer only compatible assignments. Ordinary holds accept
400–1500 ms. Y hold remains a fixed two-second recovery to Dashboard, the last
selected view and Full brightness on every screen. The panel lists onboard
navigation routes; an encoder cannot satisfy the screen/view fallback checks.
Invalid mappings remain editable, with linked errors, while save/export stay
blocked. Mapping repairs retain incomplete hold text and its parse blocker until
corrected; they cannot silently clear unrelated raw input.

The control simulator runs deterministic press/debounce/release traces through
the firmware's shared gestures and action dispatcher. Its readout shows page,
view, brightness/PWM, FPS, units, popup, log anchor and discarded events. Tap,
Hold, Recover and encoder controls advance a fixed sample clock. Statistics
reset feedback appears only after **Acknowledge statistics reset**. Off feedback
uses synthetic completed presentations, including its clear phase. Changing
the accepted configuration restarts the simulator explicitly; navigating device
pages retains view/units/FPS choices. Rejected bindings are inspection only under
**INVALID SIMULATION**, with the accepted layout and protected recovery. This
simulator also offers Dashboard, Debug and Logs pixels with explicit submit and
completion, sample/fault/stale/recovery controls, shared-history gaps/duration,
and warning latch/acknowledgment state. **Device → Startup → Replay full startup**
exercises the shared status/splash controller. Startup-only edits cancel replay
without discarding acquisition history; dashboard configuration changes restart
the configured source session.

1. Start with Overview or add an empty view. Name it using 1–14 printable ASCII
   characters. Reorder view tabs using the arrows; that is the Pico encoder order.
   Choose the view displayed after reboot with **Set as boot**.
2. Drag a sensor from the library onto a free cell, or use its **+** button.
   Drag existing widgets to move them. Esc cancels. Invalid drops preserve the
   accepted layout and show the rejected placement as an invalid diagnostic
   draft. Drag again, use the context menu or edit placement fields to repair it.
   Keyboard users can select a widget and use arrows to move it or
   Delete to remove it. Size, column and row fields provide another alternative.
3. Select a widget to rename it, choose 1×1, 2×1 or 2×2, change gauge style,
   scale limits, precision, colours, statistics and optional lambda AFR. Numeric
   widgets also support history graphs and trend. The inspector shows text
   limits. Wider and taller tiles automatically enlarge the primary value,
   shrinking the whole string (sign, digits and unit) when space is needed.
   A wide label must be shortened before shrinking the widget.
4. Configure each instance's warning independently: enablement, high/low limits
   or lambda deviation, clear threshold, lambda load guard, activation/clear dwell,
   message and applicability. Changing high/low swaps the two endpoints to
   preserve strict hysteresis. Warning order buttons move it within its view.
5. **Warning display** chooses all-view/current-view scope and one-message or
   three-message pages. Feedback popups take precedence on the Pico. Warnings
   resume afterward. The configured interval uses elapsed time. Combined list
   warnings have their own palette; one-message pages use the warning widget's.
6. **Fuel & AFR** selects equivalent 14.7, an explicit stoichiometric ratio, or
   an ethanol estimate with declared mass/volume basis, composition origin,
   component ratios/densities and common reference temperature when required.
   E5 alone does not identify the actual ethanol percentage in the tank.
7. **ECU sources** shows catalogue evidence, requests, decoding and references.
   A channel has one source shared by all its widgets. Route, polling and
   freshness remain bounded. Changing a source selects its catalogue defaults.
   Unimplemented/candidate sources remain unavailable in vehicle mode.

The [gauge and colour guide](gauges-and-colours.md) describes fixed/expanding
scales, per-style capabilities, RGB565 validation and the shared preview/device
renderer. Historical v1/v2/v3/v4 documents import through explicit migration; exports
use the current v5 schema. See
[advanced controls](advanced-controls.md) for units, smoothing, history, bands,
markers, priorities/latch/snooze and the RGB indicator panel.

The grid has four columns and two rows in landscape, or two columns and four
rows in portrait. [Screen orientation](orientation.md) explains device defaults,
Boot info/Splash/view overrides, mounting flips and one-Undo repacking. Up to eight
named views and 64 total
widgets are supported. Overlaps, unsupported glyphs, inconsistent limits,
nonfinite numbers and incompatible source fields are rejected. Invalid text
remains visible for correction and blocks saving/export. The canvas automatically
shows **INVALID PREVIEW**, a red border and affected-widget badges. Rejected
labels and overlapping in-bounds placements are drawn for inspection; the exact
attempted inputs and errors appear below it. Overlapping widgets remain draggable;
Size, Column, Row and Remove widget can repair rejected layouts in steps. Right
click (or Shift+F10) opens Move left/right/up/down and Delete. The menu also lists
widgets under that point, including a fully covered widget; select one to raise
its drag target. Movement keys stop at the grid boundary. Save/export remain
blocked until the entire draft validates. Incomplete or otherwise invalid raw
fields keep layout locked; field/options and control-route repairs remain
available and preserve unrelated raw text. Parsed fields awaiting only placement
repair permit layout repair. **Discard invalid edits** restores accepted state.
Undo/redo restores accepted edits after the invalid draft is resolved.

Diagnostics are deliberately approximate: numeric/style errors use bounded
fallbacks, source/fuel calculations use the accepted document, and warnings are
suspended. Unsupported glyphs become `?`; bounded label prefixes are clipped.
Off-screen positions project to the nearest valid grid position, preserving
size/style; the exact attempted position and fallback are described. The invalid watermark
is embedded in the canvas pixels, and badges are browser overlays. No invalid
draft is saved, exported or passed to the firmware configuration parser.

## Preview and save

**Play** advances synthetic data using monotonic elapsed time; scheduling delays
do not slow the simulation. **Next sample** advances one second while paused.
Playback starts paused and CSS respects reduced motion. **bar / PSI** uses the
same canonical boost conversion as the Pico. **Warnings** demonstrates their
appearance with illustrative values and compressed activation dwell. Threshold,
scope, context and enablement still apply; not every rule must activate. This
mode is not an ECU, dwell-timing or physical FPS test.

**Test values** offers channel minima/maxima, extreme digit widths and zero
(including unavailable AFR). These are synthetic inspection tools, not suggested
operating limits. Select **Animated demo** to restore the same elapsed-time
generator used by the firmware. A failed render displays **PREVIEW ERROR** over
the previous frame instead of silently presenting it as current.

Accepted changes and selection are saved locally. Two studio tabs coordinate
with an exclusive Web Lock and compare the last loaded/written saved bytes
before replacing them. An external change stops automatic replacement; export
your work or explicitly use **Save here**. A denied/quota write never says Saved.
Other pages on the same origin that ignore the studio's lock are outside this
coordination contract. Always export a portable backup.

**Diagnostics → Configuration files** imports/exports complete `.eavp` projects,
dashboard-only JSON and compact `.eav` runtime files. Complete projects retain
startup settings and original artwork by default; an explicit smaller export
keeps only converted animation. See [project storage](projects.md). Imports
are transactional, including async stale-file protection. Limits are 9 MiB for
projects, 128 KiB for JSON and 16 KiB for `.eav`. The shared decoder checks version,
length, checksum, canonical encoding and every field. Decimal input and JSON
use identical single-precision rounding, including midpoint boundaries.

From the repository root, after exporting `dashboard.eav`:

```powershell
./firmware/scripts/Build-Dashboard.ps1 -Name my-views -Profile 280-70 -Mode Demo -Config ./dashboard.eav
```

This captures and checks the image; it does not flash. See [building.md](building.md)
for the complete workflow. The device parses the file once before core 1 starts.
Returning from Debug/Logs restores the selected view in RAM. Reboot uses the
configured boot view. Default encoder click cycles Full / 50% Dim / delayed
Off; accepted per-screen mappings select the actual actions. Protected Y hold
always recovers Dashboard and Full brightness. **Build & Flash** can build the
complete accepted project and explicitly program an inspected Pico 2 through the
local helper. Exporting, downloading a recipe or running the wrapper above never
flashes a device.

## Verification and limits

Native tests exercise the same transactional reducer, scaled drag geometry,
pixel conversion, actual configured renderer, warning paging and elapsed-time
clock used in the page. CI also compiles/lints Wasm and builds production assets.

Under **Diagnostics → Configuration files**, **Browser performance trace** records up to 512 interactions
and long tasks locally. Warm the desired document, start a trace, perform edits,
stop and export it. Input latency runs from handler entry through the animation
boundary after canvas submission, including an opportunity for browser paint.
It is not optical timing. The Long Tasks API reports browser main-thread tasks
of at least 50 ms; unsupported detection is explicit. These measurements concern
the tested browser/computer, not Pico performance.

The [browser verification record](browser-verification.md) gives measured
startup, file operations, maximum-document latency, asset sizes and test limits.

Pico hardware, display scanout, electrical timing and 2019 DNUC ECU support need
physical validation. Simos/CANchecked documentary candidates do not qualify an
ECU session. The user's 280/70 profile worked previously; this configurable image
has not been flashed or physically tested.

Implementation references: [Leptos 0.8.22](https://github.com/leptos-rs/leptos/releases/tag/v0.8.22),
[Trunk documentation](https://trunk-rs.github.io/trunk/),
[Pointer Events](https://www.w3.org/TR/pointerevents3/),
[Web Locks](https://www.w3.org/TR/web-locks/), and
[Long Tasks](https://www.w3.org/TR/longtasks-1/).

## ECU Reference

The retained ECU Reference workspace adds offline searchable signal and research cards, section/status filters, and source-detail links without changing configuration or acquisition. See [usage](ecu-reference.md) and the [technical library](../vehicle/ecu-reference/README.md).
