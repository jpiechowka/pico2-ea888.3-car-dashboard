# Configurable dashboard and Rust view studio

Design date: 12 October 2026. Base firmware: `e710216`.

This record preserves the original landscape design. The later [orientation
contract](orientation.md) adds native portrait and independent screen overrides.

## Confirmed requirements

The user confirmed a **4-column, 2-row grid**, Rust application code compiled
to WebAssembly with generated browser bindings allowed, and exporting a file
which is embedded during the firmware build and parsed at startup. No custom
JavaScript application source, Node or npm is needed. The existing encoder
click cycle remains Full, Dim at 50% duty, Off, with the current two-second
Off popup/completion guard. Dashboard rotation switches named views; Debug and
Logs retain their controls. Returning to Dashboard restores the selected view.

The browser editor supports drag/drop, selection, keyboard alternatives,
multiple named views, bounded widget settings, source information, warnings,
fuel-aware optional AFR, import/export and saved drafts. The unit button switches
FollowBoost widgets and their statistics between bar and PSI. Fixed bar/PSI/kPa
readouts retain their selected units; an equal secondary unit is omitted.

## Architecture and alternatives

Recommended: a static Leptos CSR application, a shared `no_std` configuration
crate, and a shared `no_std` dashboard core. Typed JSON is the editable file;
a compact versioned Postcard envelope is the Pico runtime file. Firmware,
browser and command-line validation consume the same rules. The browser
preview runs the actual framebuffer renderer, not a second CSS implementation.

Alternatives considered:

| Choice | Tradeoff | Decision |
| --- | --- | --- |
| JSON directly on Pico | Readable, but larger and brings a text/escape parser into startup | Keep JSON for editing; bounded binary for device |
| CBOR/minicbor | Standard self-describing encoding; strict duplicate/unknown-field rules need care | Postcard fits the closed Rust schema |
| YAML | Readable but complex syntax/implicit typing and larger parser surface | Excluded |
| Dioxus | Broad cross-platform tooling, useful if native applications become a requirement | More scope than this static browser editor needs |
| Yew | Mature component model and current release | Viable alternative; Leptos signals suit inspector/drag state |

As checked on this date, stable releases are Leptos 0.8.22, Dioxus 0.7.10,
Yew 0.23.0 and Postcard 1.1.3. Leptos is feature complete and maintained more
lightly; that maintenance tradeoff is recorded, and the data/renderer crates
remain independent of it. Use stable library releases on the latest available
nightly Rust, pinned by date for reproducible builds. `rustup update` confirmed
nightly-2026-10-11 remains current when this task began. Exact Wasm build and
source compatibility are acceptance gates, not assumptions from release notes.

## Ownership and data flow

```text
Editor draft -> strict JSON import + shared validation -> validated config
                         |                         |
                         |                         +-> exact Rust pixel preview
                         +-> JSON save / binary export

Binary file -> build-time validation + embedding in flash
            -> one bounded runtime decode and validation before core1 starts
            -> one immutable shared Config
                |                           |
                | core1                     | core0
                v                           v
       canonical acquisition          selected view + input state
       source/quality/context         bounded scene + warning presentation
       histories + per-widget dwell   per-physical-buffer retained caches
                |                           |
                +-> coherent snapshot ------+
                                            v
                               existing affine transfer/completion path
                                            v
                               presentation-gated backlight and RGB
```

Configuration never enters a render-loop parser, a mutable cross-core global,
or a DMA-owned framebuffer. Reading an imported source descriptor does not
qualify the adapter or ECU. Keep the existing transport qualification gate.
No configuration-controlled executable expressions or diagnostic write services.

## Concrete bounds

| Resource | Bound / rule |
| --- | --- |
| Display | 320x240 RGB565; 26-pixel header; 320x214 content |
| Grid | 4x2; spans 1x1, 2x1, 2x2 only |
| Widget rectangles | 80x107, 160x107, 160x214 pixels respectively |
| Views | 1..8; stable nonzero unique IDs; explicit boot view |
| Widget instances | At most 64 total and 8 in a view; stable unique IDs |
| Layout | Integer coordinates; checked edges; no overlaps; gaps and empty individual views allowed |
| View name | 1..14 printable ASCII characters, using the 10x20 header font |
| Widget label | 1..12 characters at width1, 1..24 at width2, using 6x10 |
| Warning message | 1..20 printable ASCII characters; no embedded control/newline |
| Text handling | Reject unsupported glyphs/lengths; never silently truncate an export |
| Binary file | At most 16 KiB, including envelope; reject trailing data |
| Editor import | At most 128 KiB before parsing; typed bounded vectors/strings |
| Decoded Config | At most 8192 bytes on the ARM target, enforced by size checks |
| Runtime heap | None in firmware configuration, layout or warning evaluation |
| Core0 headroom | At least 64 KiB in all accepted release profiles |
| Blink/fade/rotation | Monotonic elapsed time; no frame-count clocks |

Pixel allocation uses half-open rectangles and an explicit content inset;
every draw is clipped to its widget. Trend arrows get a reserved position
below the label so a maximum-length label cannot push them into a neighbor.
Value font selection measures the actual string, including sign, decimal and
units; it falls back through the existing font sizes rather than losing digits.
Unavailable and unrepresentable values have explicit bounded text.

Header lanes reserve left status, central view name, and right FPS. Demo must
remain visibly identified even though the central title becomes the view name.
Render every status and combined FPS maximum in pixel-boundary tests.

Two RGB565 framebuffers use 307200 bytes. The current traced Demo build has
428592 bytes static allocation and 95696 bytes core0 headroom within the linked
512 KiB main SRAM. RP2350 has an additional 8 KiB of scratch SRAM which this
layout does not use. The current 8192-byte journal at flash offsets
`0x1FE000..0x200000` remains reserved; its 4-byte preference payload is not a
place to store the full view configuration. The serialized config stays in
flash and is decoded once. Core1 receives only a shared immutable reference.

Provisional incremental static envelope: Config8192, warning states4096,
snapshot expansion/copies2560, cache identities1024, input/transitions512,
scratch2048, async/alignment contingency4096 =22528 bytes. Final ELF and type
sizes decide acceptance; this is a budget, not measured final consumption.

## Configuration contract

The shared schema has closed enums and typed bounded collections. It contains:

- schema version, boot-view ID, ordered named views and a flat widget pool;
- per-widget view/source references, position/span, label, decimal precision,
  graph/average/extrema/trend switches, graph auto/fixed range, and optional AFR;
- per-widget warning enable, high/low or lambda-deviation mode, canonical-unit
  trigger/clear thresholds, on/off dwell, message and needed operating context;
- warning scope (all configured views by default or current view), presentation
  (cycle one or list three), and bounded wall-time page interval;
- fuel mode: gasoline-equivalent14.7, explicit stoichiometric ratio, or an
  explicitly assumed/measured ethanol composition with its calculation inputs;
- one source descriptor per canonical channel, referenced by widget instances,
  with catalogue revision, access kind, request/responder IDs when known,
  PID/DID or fixed derivation, decoder identity/width/endianness/scale, timing,
  semantic identity and documentary evidence status.

Every source descriptor survives export, including unqualified candidates.
Browser-only prose/URLs are held in a deduplicated source catalogue. Catalogue
IDs and revisions bind compact records to that explanation. A qualification
flag in an untrusted imported file cannot enable polling or manufacture valid
source metadata. Candidate addresses and unsupported decoders remain explicit.

Route choices are `DeploymentProfile`, `Physical11 { request, response }` and
`Unknown`. The four implemented OBD defaults use DeploymentProfile; DSG stays
Unknown. The existing single-responder session must report RouteNotAvailable
for a differing explicit pair, never silently add another ECU. Tightened
freshness applies to sample visibility and warning expiry/continuity alike.

The binary envelope is magic `EAV1`, schema version `u16LE`, payload length
`u16LE`, CRC32 `u32LE`, then Postcard payload. Version1 uses CRC-32/ISO-HDLC
(reflected polynomial0xEDB88320, initial/final XOR0xFFFFFFFF). CRC covers the
first eight header bytes followed by the payload, skipping its own four bytes.
Envelope and model versions must both equal1. Validate size, magic, exact
length, supported version and CRC before decoding. Use `take_from_bytes` and
require an empty remainder. Reject unknown variants, oversized arrays/strings,
duplicate IDs, dangling references, overlaps, impossible bounds and invalid
numeric values. CRC detects accidental corruption, not authenticity.

All floating-point fields must be finite and range checked. Warning hysteresis
must have the correct direction; dwell remains1..30000ms. Graph fixed minimum
must be less than maximum. Configured source timing cannot bypass reviewed
transport floors or freshness caps. Device validation uses the same contract
independently of the browser. Build-time validation rejects a bad supplied file;
runtime failure selects a reviewed default layout, shows/logs a config fault
and leaves vehicle data unqualified. There is no partially applied config.

The build script reads the selected external binary once, validates those bytes,
then writes the identical bytes to OUT_DIR for embedding. It tracks both the
selection environment variable and the source file with Cargo rerun directives.
Artifact capture includes the actual embedded bytes and their hash. The shared
config crate has no dependency on the dashboard core; core maps the closed
configuration channel enum to its existing SensorId enum explicitly. A shared
optional JSON facade serves the host compiler and editor with the same strict
typed import/export behavior, while firmware enables no JSON or std features.

The selection variable is `DASHBOARD_CONFIG`; absolute paths are recommended,
and relative paths resolve against `firmware/pico2`. An absent variable selects
the bundled default. An explicitly empty, missing, unreadable or invalid path
fails the build. Runtime fallback owns a persistent config-fault flag shown in
Debug and as bounded dashboard status, in addition to the boot log entry.

## Widget and warning behavior

Start with the existing eight canonical channels and sensible display defaults:
boost, lambda, ECU voltage, coolant, engine oil, DSG oil, IAT and EGT. Widget
instances can repeat channels in different views without duplicating polling
or histories. Optional presentation fields belong to the instance. Source
identity and accepted samples belong to the canonical channel.

The source resolver has an explicit result: implemented OBD binding, disabled,
or unavailable with a closed reason (candidate decoder, unresolved scaling,
missing route or unsupported semantic). It is exercised by the real Session
scheduler. Current implemented bindings are0105 coolant,010F IAT,0142 voltage
and015C engine oil; duplicates reuse one canonical poll. Catalogue choices
cannot bypass `Profile::UNQUALIFIED`, capability discovery or source matching.
Unknown routes remain unspecified, especially for DSG; no functional broadcast
or diagnostic write service is configurable. A deployed profile remains a
separate reviewed transport contract. Poll periods can only slow the reviewed
defaults (1000/1000/500/1500ms respectively, maximum10000ms), and configured
freshness can only tighten the canonical caps (500ms boost/lambda,2000ms
voltage,3000ms temperatures). Contextual alarm freshness retains its500ms cap.

All vehicle warning defaults are disabled until explicitly configured. Demo
uses clearly labelled synthetic warning examples; enabling a Demo example
does not endorse a vehicle threshold. Config records retain the enabled state
and limit even while disabled, with separate DemoOnly/VehicleAndDemo applicability.
Vehicle threshold provenance is user-configured and distinct from source evidence.
Lambda permits contextual deviation rules only,
not a context-free high/low substitution. Battery and EGT keep their operating
context requirements independently of user-configured display options.

Warnings are evaluated by the acquisition owner for every configured instance
on accepted samples/faults, before latest-value publication. Extract the existing
source/context/hysteresis/dwell evaluator and reuse it; render calls never
advance dwell. Selecting a view does not reset an alarm. Disabled rules never
contribute. Existing measured-lambda/target/load/fuel-cut and EGT-location
requirements remain; an absent context cannot be replaced with a guessed one.
Synthetic Demo rules stay labelled examples rather than vehicle recommendations.

Multiple active warnings are ordered deterministically by configured view and
widget order. Cycle mode displays one at a time. List mode displays at most
three 20-character messages in a separately sized210x130 popup, with a small
page count; additional messages rotate by elapsed time. All/visible-view scope
is configurable. Stale/invalid sources suppress their warnings immediately.
User feedback retains priority and the warning sequence resumes after expiry.
Debug and Logs remain unobstructed. Red/off follows the completed warning
frame, including a multi-warning page; other visible states remain green.

Rule slots and channel indices are separate types. Rule63 may reference channel0;
every context, freshness and snapshot lookup uses the channel, while dwell and
message ownership uses the rule slot. Page timing uses an anchored cursor and
deadline, retains a still-active current rule when the set changes, and cannot
starve earlier entries by repeatedly resetting on newly arriving warnings.

Retained caches identify the config, selected view, geometry and effective
overlay for each physical buffer. A view/layout change restores the entire
buffer, including old gaps/dividers. Each submitted warning presentation is
bound to its frame token; a queued view must not change the LED early. Serial,
overlapped, unchanged, maintenance and failure paths remain in the review scope.
Resolve one effective overlay per frame for scene drawing, retained keys, LED
presentation and input exclusion. Rebind/reset visible color-transition slots
when their WidgetId or ViewId changes; equal slot numbers are not identity.

## Input and persistence

Dashboard rotation uses signed modulo over ordered views, including coalesced
large positive/negative deltas; one view is a no-op. View feedback shows the
configured name and uses the same feedback priority/expiry policy. The name
also remains in the header. A view selection survives visiting Debug/Logs.
It changes no core1 acquisition/history state. Existing bar/psi and statistics
reset behavior applies regardless of the selected view.

Encoder press continues the existing brightness controller. Replacing a pending
Off popup with view feedback cancels that pending shutdown; an already black
screen stays black until the existing click cycle lights it. View changes do
not create flash writes. On reboot the configured boot view is selected;
existing persistent unit/FPS preferences remain in the current journal format.
The user's requested page-switch persistence does not require a journal migration.

Physical page buttons continue to run before the encoder batch. On Dashboard,
apply the batch's view motion and interrupt older feedback first, then process
the existing click count. Click feedback wins if both occur, so motion cannot
cancel an Off request created by the same batch. View feedback uses a210-pixel
box for its14-character name. Motion while PWM is zero changes selection but
must not claim input-to-visible success. No-op/full-wrap motion does not either.

## AFR calculation

Current AFR uses the same prepared canonical presentation as lambda before
rounding, including enabled display smoothing and the same source/freshness
guards. Hiding AFR returns its space to the widget. Raw warnings, history,
extrema and peaks retain unsmoothed canonical lambda. Bar/psi changes do not affect mixture values.

Keep the current `EQ14.7` default. For a user-selected ethanol blend, distinguish
mass fraction from component-volume fraction. For volume fraction `x`:

```text
w = x*rho_ethanol / ((1-x)*rho_gasoline + x*rho_ethanol)
stoich = (1-w)*stoich_gasoline + w*stoich_ethanol
AFR = lambda*stoich
```

Component densities/stoichiometric ratios are explicit editable assumptions;
measured inputs require a compatible declared reference basis. Percentage is
0..100 inclusive and all denominators are positive. Resolve stoichiometry once
when validating the immutable configuration. E5 is an upper label, not a
measurement; do not silently set the user's tank to exactly5%. The UI explains
the selected estimate and the device has a short corresponding scale label.

## Rust editor visual and interaction design

Use an instrument-workbench layout, with quiet surrounding controls and the
actual Pico display as the central visual. Dark by default with restrained red
selection/drop outlines. No marketing hero, filler metric cards or remote fonts.

- Palette: canvas `#0b0d12`, panels `#151922`, border `#2b3240`, red `#ff3b52`,
  text `#f0f3f7`, secondary `#929dae`. Positive validation has a separate muted
  green token; errors always have text, not color alone.
- Type: Bahnschrift/DIN-style display fallback, Segoe UI body, Consolas/data
  labels. Local font stacks keep export/editor use independent of a font CDN.
- Left: searchable widget library and source availability. Center: named view
  tabs, integer-scaled pixel preview with editing hit regions. Right: selected
  widget Display/Warnings/Source settings and character/size limits.
- Footer: validation summary, saved-state feedback and import/export actions.
  Layout, tuning and export are workflow steps, not separate copies of state.

Implement drag/drop with Rust `web-sys` pointer events, pointer capture and
checked target coordinates. Include Escape/cancel, out-of-grid drops, overlap
feedback, resizing and moving existing widgets. Keyboard and click-to-place
alternatives are required. Touch input uses the same placement reducer.
No asynchronous late event may apply a drag from another view or imported draft.

The editor owns a draft, validation errors and a last validated document. A later
[preview hardening stage](preview-hardening.md) adds automatic, prominently marked
diagnostic rendering of rejected edits, separate from validated rendering. Partial
numeric edits are allowed but invalid drafts cannot export or claim to be saved.
Import is transactional. Undo/redo is bounded. Browser local storage stores a
versioned draft only after successful validation and reports quota/access
failure without discarding export capability. Import/export round-trips preserve
all configuration fields. Demo preview clearly remains synthetic.

Document revision, editor-selected view/widget, raw field strings/errors,
validated snapshot, active pointer transaction and last saved revision are
separate state. Pointer commits require the original pointer ID and revision.
Imports carry request ID plus base revision; a late completion cannot replace
a newer import or subsequent edit. Undo/import/view changes cancel old drag and
field events. Invalid raw fields block selection/view changes and both exports
until corrected or explicitly discarded. Bounded undo/redo keeps32 accepted
documents. Async persistence never marks a newer revision saved after an older
write. A versioned editor envelope stores selected view separately from the
device boot-view setting.

Deleting the last view is rejected. Deleting another view removes its widgets
atomically and selects the next surviving ordered view; if needed, boot view
falls back to the first surviving view. Duplication assigns fresh IDs and fails
atomically on capacity. Shrinking rejects an overlong label instead of truncating
it. Import accepts the editable JSON or checked device binary, explicitly named
as separate export choices. Every route converges on the shared validator.

Pointer hit testing maps the actual canvas content rectangle to320x240, rejects
the header and out-of-bounds coordinates, and preserves the grab offset. Canvas
pixels are read as big-endian RGB565 then expanded to opaque RGBA; guides remain
separate. Pointer rendering is coalesced through requestAnimationFrame. On the
recorded development browser, target p95 input-to-visible-preview is at most50ms
for64 widgets/eight visible, with no attributable editor task above100ms during
two warmed traces. Record startup/import/export times and release asset sizes;
set bundle/startup budgets from that measured baseline, without claiming Pico FPS.

Motion is limited to short editor hover/drag feedback with reduced-motion
support. Firmware view changes redraw immediately. In v2, warning tiles switch
between contrast-checked colour endpoints; warning timing uses absolute time.
Renderer speed, browser interaction latency,
binary size and device SRAM are measured separately from physical display FPS.

## Build topology and pinned tools

A root Cargo workspace owns `crates/dashboard-core`, `crates/dashboard-config`
and `crates/dashboard-config-tools`. The configurator, firmware and host are
independent workspaces with their own lockfiles and shared path dependencies,
preserving separate feature resolution and target flags. All use the repository's
nightly-2026-10-16 toolchain (five Cargo roots/lockfiles). Firmware
forwards `simple-outline` explicitly; the editor uses that default for matching
preview output. CI checks root formatting/native pure tests, core/config no_std
Thumb and Wasm builds, production Trunk output, and the existing firmware/host
gates. No root Cargo target/linker override is introduced.

Pin Leptos0.8.22 CSR (without its optional nightly feature), wasm-bindgen0.2.129,
web-sys/js-sys0.3.106, wasm-bindgen-futures0.4.79, Serde1.0.229,
serde_json1.0.154, heapless0.9.3 and Postcard1.1.3 with defaults disabled.
Use Trunk0.21.14 and matching wasm-bindgen CLI0.2.129. Browser release keeps
binding metadata until wasm-bindgen finishes; firmware's strip/linker profile
does not apply to it. Verify the resolved sources/features and all three target
families before accepting those pins. Browser tests open served release assets.

## Source register and evidence limits

- [Leptos releases](https://github.com/leptos-rs/leptos/releases),
  [current crate](https://docs.rs/crate/leptos/0.8.22),
  [maintenance statement](https://github.com/leptos-rs/leptos/issues/4707).
- [Dioxus releases](https://github.com/DioxusLabs/dioxus/releases),
  [Yew documentation](https://yew.rs/docs/getting-started/introduction),
  [Trunk maintained documentation](https://trunk-rs.github.io/trunk/).
- [Postcard decoding API](https://docs.rs/postcard/1.1.3/postcard/fn.take_from_bytes.html).
- [PIM715 product](https://shop.pimoroni.com/products/pico-display-pack-2-8),
  [schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048),
  [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf).
- [CANChecked manual](https://www.canchecked.de/updater/MFD2832-Gen2-manual.pdf)
  identifies encrypted TRX information. Supplied U-DNU bytes are not a DID map.
- [SimosTools pinned source](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PIDs.kt)
  and [VW_Flash pinned definitions](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)
  disagree on speed DID2033 scaling. Preserve provenance and candidate status.
- [E5 explanation](https://www.gov.uk/guidance/e10-petrol-explained) and
  [AEM fuel-dependent AFR explanation](https://documents.aemelectronics.com/techlibrary_30-4100_digital_wideband_uego_gauge.pdf).

Detailed research/source revisions stay in ignored `firmware/target/configurator-audit`.
No exact DNUC support, optical FPS, sustained overclock stability or measured
fuel composition is claimed. The working280/70 profile remains unchanged.
