# EA888 View Studio

The editor is a static Rust/Leptos application compiled to WebAssembly. It runs
the same validator, acquisition simulation, warning pager and pixel renderer as
the firmware. HTML and CSS provide the surrounding workbench; wasm-bindgen and
Trunk generate the browser loader. There is no authored JavaScript, Node/npm
toolchain, server API, account or uploaded configuration.

## Start on Windows

From the repository root:

```powershell
./configurator/Studio.ps1 -Serve
```

Open `http://127.0.0.1:8080`. Use `-Port 8081` if necessary. The script installs
the pinned Trunk 0.21.14 into the repository's ignored `target` directory when
missing, ensures the pinned nightly's Wasm target is present, then serves a
release build on localhost. Stop the foreground server with Ctrl+C.

For a static release directory only, omit `-Serve`. Output is
`configurator/dist`. On another platform, enter `configurator`, install
`trunk --version 0.21.14 --locked` with Cargo, add the `wasm32-unknown-unknown`
Rust target, and run `trunk serve --release --locked`. If `NO_COLOR` is set,
Trunk expects `true` or `false`, not `1`.

Serve the files over HTTP on localhost or HTTPS on a static host. Opening
`index.html` directly as a local file is not supported. Browser saving requires
Web Locks and permitted local storage; file export remains usable without them.

## Build views

1. Start with Overview or add an empty view. Name it using 1–14 printable ASCII
   characters. Reorder view tabs using the arrows; that is the Pico encoder order.
   Choose the view displayed after reboot with **Set as boot**.
2. Drag a sensor from the library onto a free cell, or use its **+** button.
   Drag existing widgets to move them. Esc cancels, and invalid drops preserve
   the layout. Keyboard users can select a widget and use arrows to move it or
   Delete to remove it. Size, column and row fields provide another alternative.
3. Select a widget to rename it, choose 1×1, 2×1 or 2×2, change precision, graph
   limits, statistics, trend and optional lambda AFR. The inspector shows text
   limits. A wide label must be shortened before shrinking the widget.
4. Configure each instance's warning independently: enablement, high/low limits
   or lambda deviation, clear threshold, load guard, activation/clear dwell,
   message and applicability. Changing high/low swaps the two endpoints to
   preserve strict hysteresis. Warning order buttons move it within its view.
5. **Warning display** chooses all-view/current-view scope and one-message or
   three-message pages. Feedback popups take precedence on the Pico. Warnings
   resume afterward. The configured interval uses elapsed time.
6. **Fuel & AFR** selects equivalent 14.7, an explicit stoichiometric ratio, or
   an ethanol estimate with declared mass/volume basis, composition origin,
   component ratios/densities and common reference temperature when required.
   E5 alone does not identify the actual ethanol percentage in the tank.
7. **ECU sources** shows catalogue evidence, requests, decoding and references.
   A channel has one source shared by all its widgets. Route, polling and
   freshness remain bounded. Changing a source selects its catalogue defaults.
   Unimplemented/candidate sources remain unavailable in vehicle mode.

The grid has four columns and two rows. Up to eight named views and 64 total
widgets are supported. Overlaps, unsupported glyphs, inconsistent limits,
nonfinite numbers and incompatible source fields are rejected. Invalid text
remains visible for correction and blocks saving/export; the canvas explicitly
identifies the last valid preview. Undo/redo restores accepted edits.

## Preview and save

**Play** advances synthetic data using monotonic elapsed time; scheduling delays
do not slow the simulation. **Next sample** advances one second while paused.
Playback starts paused and CSS respects reduced motion. **bar / PSI** uses the
same canonical boost conversion as the Pico. **Warnings** demonstrates their
appearance with illustrative values and compressed activation dwell. Threshold,
scope, context and enablement still apply; not every rule must activate. This
mode is not an ECU, dwell-timing or physical FPS test.

Accepted changes and selection are saved locally. Two studio tabs coordinate
with an exclusive Web Lock and compare the last loaded/written saved bytes
before replacing them. An external change stops automatic replacement; export
your work or explicitly use **Save here**. A denied/quota write never says Saved.
Other pages on the same origin that ignore the studio's lock are outside this
coordination contract. Always export a portable backup.

**Files & build** imports/exports editable JSON or the compact `.eav` runtime
file. Imports are transactional, including async stale-file protection. A
128 KiB JSON or 16 KiB binary limit applies. The shared decoder checks version,
length, checksum, canonical encoding and every field. Decimal input and JSON
use identical single-precision rounding, including midpoint boundaries.

From the repository root, after exporting `dashboard.eav`:

```powershell
./firmware/scripts/Build-Dashboard.ps1 -Name my-views -Profile 280-70 -Mode Demo -Config ./dashboard.eav
```

This captures and checks the image; it does not flash. See [building.md](building.md)
for the complete workflow. The device parses the file once before core 1 starts.
Returning from Debug/Logs restores the selected view in RAM. Reboot uses the
configured boot view. Encoder click remains Full / 50% Dim / delayed Off;
the other screens retain their controls. No new firmware is flashed by the studio.

## Verification and limits

Native tests exercise the same transactional reducer, scaled drag geometry,
pixel conversion, actual configured renderer, warning paging and elapsed-time
clock used in the page. CI also compiles/lints Wasm and builds production assets.

Under Files & build, **Browser performance trace** records up to 512 interactions
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
