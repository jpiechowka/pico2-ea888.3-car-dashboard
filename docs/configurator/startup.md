# Startup

Device → Startup configures the shared Rust startup scenes: AEZAKMI, SUTUTU,
GOTTA GO FAST, HOW MUCH BOOST BRO and WHERE LAMBO. **Preview startup** uses the
existing 320×240 Studio display, with play/pause, restart, seeking and a
reduced-motion still. The preview labels its OBD status as synthetic; browser
timing is not physical display, LED or ECU qualification.

**Replay full startup** runs the shared startup controller with simulated acquisition,
controls, warning LED and matching frame completions. Replay and cancellation after
project edits preserve acquisition history and its clock. Seekable artwork preview
and full replay are separate modes; neither qualifies physical hardware. Use the
simulated buttons, encoder, warning conditions and frame-completion controls to
inspect Dashboard transitions, skips and recovery.

Timing text remains a draft until **Apply timing** or **Discard timing**. Invalid
text stays inspectable and marks the preview INVALID; accepted pixels/configuration
remain unchanged. Valid unapplied timing also blocks save, export, build and other
project changes. Apply creates one Undo entry for both fields; Discard creates none.

The defaults are a **Brief 600 ms** completed-system-check screen followed by
**AEZAKMI for 2500 ms**, looping. Status can be Off, Brief or Detailed and has its
own 0–2000 ms duration. Splash duration is 0–10000 ms: zero skips only the splash.
Each procedural cycle is two seconds. Turning looping off holds its final frame
until the splash deadline; reduced motion holds a representative still.

On hardware, each phase's visible timer starts with its first matching completed
transfer. An additional entry deadline bounds delayed startup presentation.
Neither timer bypasses transfer ownership or the existing display watchdog.
Acquisition continues, and status reports completed configuration/preferences
loading plus the current derived OBD status. A source selected in configuration
does not prove connection, response or qualification.

By default, warnings finish the splash. The configured warning LED can still
indicate attention while warning text waits for Dashboard. Hidden warning text
is never marked presented and cannot be acknowledged. Both warning interruption
and startup warning LED indication are configurable. Configuration fallback,
transport reset-required faults and protected Y recovery exit startup. A fresh
button tap or encoder rotation/click skips startup and consumes that transaction.
Boot-held buttons require stable release first; delayed startup events do not
also invoke ordinary dashboard actions. Y recovery remains a two-second hold.
Normal Dashboard encoder rotation and Full/Dim/Off clicks resume afterward.

## Profile files and builds

Startup settings now belong to the accepted Studio project and share Undo/Redo. Export a [complete `.eavp` project](projects.md) to keep dashboard, startup, converted pixels and original artwork together. The explicit smaller export omits only the original; dashboard-only `.eav` and JSON omit startup. A standalone `startup.easp` remains useful for profile-only transfer and does not contain custom pixels.

For a local firmware build, set `DASHBOARD_STARTUP_PROFILE` to the absolute path
of the exported `.easp` alongside the normal `DASHBOARD_CONFIG` selection. Omit
the startup variable to build the defaults. The build rejects invalid selected
profiles, copies the canonical 16-byte profile into `startup_profile.bin` in
`OUT_DIR`, and exposes `DASHBOARD_STARTUP_SHA256`. Existing config hashes alone
do not identify a changed startup profile. Helper upload and complete artifact
identity binding use the converted-only accepted project; the helper verifies linked
profile/asset bytes before admitting an artifact.

EASP v1 is exactly 16 bytes: `EASP` magic at 0..4, version 1 at byte 4, theme
0..5 (Custom) at byte 5, status Off/Brief/Detailed as 0..2 at byte 6, flags at byte 7
(bit 0 loop, bit 1 warning LED, bit 2 attention interruption), little-endian
u16 status/splash milliseconds at 8..10 and 10..12, and four zero reserved bytes.
Unknown versions, flags, enums, reserved bytes and out-of-range durations fail
validation. Reduced motion uses the dashboard's existing setting; the extra
Studio checkbox is a preview-only override. The implemented format does not expose
the earlier proposed separate cycle-duration, background or accent fields. Procedural
cycles are fixed at two seconds; custom frame timing comes from the converted asset.
A corrupt optional embedded profile skips startup with a fault rather than panicking;
explicit invalid build inputs still fail the build.

## Validation

Complete project packaging, original retention, converted-only export and shared project history are implemented. Artwork conversion/rendering and helper binding have their own focused integration checks; no browser or physical success is implied by the package codec.

Shared state/render, bounded configuration/asset, controls/LED and failure/recovery
tests run with Wasm/ARM checks in CI. The firmware profile matrix also runs
practical whole-image checks for RAM, stack assumptions, flash and linked inputs.
Actual Studio checks covered timing drafts, conversion, retained-original
reconversion, project restore and repeated startup replay; see the
[workflow gallery](gallery.md). These checks do not establish physical timing,
device stack high-water or every browser failure interleaving. Hardware
qualification remains in the [runbook](../validation/pico2-dnuc-runbook.md).
Resource limits have not been increased.

## Custom artwork conversion

Rust `dashboard-startup-tools` converts PNG, row-major PNG sprite sheets and
animated GIF into EASA v1 RGB565 animations. Conversion controls select contain,
crop-to-fill or stretch, an opaque matte, 1–20 fps sampling, and a clip up to ten
seconds. PNG alpha is composited over the matte; sRGB is assumed. GIF frames are
composited before disposal (keep, background or previous), with selectable logical
or transparent background. Zero delays become 100 ms. GIF loop counts do not
replace the startup Loop option. Unsupported controls and animated PNG fail.

Sources are limited to 8 MiB, 4096 pixels per side, 1,048,576 canvas pixels,
300 source frames, 32 million total decoded frame pixels and 60 seconds. Decoder
buffers are explicitly limited; accounted conversion buffers fit within 32 MiB
(this is not a measured browser process RSS claim). Output is 320×240, at most
300 frames and 512 KiB, with at least 34 ms per output frame. Identical sampled
frames coalesce; short final intervals merge. Incompressible animation can exceed
the output limit and is rejected with advice to shorten or reduce sampling.

The dedicated same-origin Rust Worker is replaceable and cancellable, with a
10-second deadline. Editor changes invalidate pending results. Conversion failure
retains accepted artwork. Trunk emits `startup-worker_loader.js` and its associated
JavaScript/Wasm assets; packaged Studio must serve these with `worker-src 'self'`.
The retained-original callback lets the project UI restore options and reconvert
without selecting the original again. A converted-only project needs reimport
for source edits.

Malformed conversion numbers stay visible with **INVALID CONVERSION OPTIONS**.
Both file import and retained-original conversion are blocked until corrected or
the original recipe is restored. These are settings for a future conversion;
accepted artwork and valid project export/build remain available.

EASA validates header/table/payload bounds, reserved fields, frame times and CRC32.
Frames use raw big-endian RGB565 or row-bounded RLE. Seeking uses the elapsed time;
firmware decodes rows into the existing rendering buffer without a GIF decoder or
another framebuffer. Boot validation is incremental, yielding between bounded
chunks, with a separate 250 ms readiness deadline. Invalid assets exit startup.
Per-frame CRC is also checked during rendering before submission.

Set `DASHBOARD_STARTUP_ASSET` to the EASA file when the EASP selects Custom.
Custom without an asset and procedural profiles with an asset fail the build.
The asset is embedded as `startup_asset.bin`, with digest
`DASHBOARD_STARTUP_ASSET_SHA256`; the profile digest remains
`DASHBOARD_STARTUP_SHA256`. Runtime assets do not contain original images.
