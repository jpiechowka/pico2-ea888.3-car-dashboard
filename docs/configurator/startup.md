# Startup

Device → Startup configures the shared Rust startup scenes: AEZAKMI, SUTUTU,
GOTTA GO FAST, HOW MUCH BOOST BRO and WHERE LAMBO. **Preview startup** uses the
existing 320×240 Studio display, with play/pause, restart, seeking and a
reduced-motion still. The preview labels its OBD status as synthetic; browser
timing is not physical display, LED or ECU qualification.

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
identity binding remain part of the next project delivery.

EASP v1 is exactly 16 bytes: `EASP` magic at 0..4, version 1 at byte 4, theme
0..5 (Custom) at byte 5, status Off/Brief/Detailed as 0..2 at byte 6, flags at byte 7
(bit 0 loop, bit 1 warning LED, bit 2 attention interruption), little-endian
u16 status/splash milliseconds at 8..10 and 10..12, and four zero reserved bytes.
Unknown versions, flags, enums, reserved bytes and out-of-range durations fail
validation. Reduced motion uses the dashboard's existing setting; the extra
Studio checkbox is a preview-only override.

## Next delivery and validation

Complete project packaging, original retention, converted-only export and shared project history are implemented. Artwork conversion/rendering and helper binding have their own focused integration checks; no browser or physical success is implied by the package codec.

Focused state/render tests, existing controls/LED regressions and Wasm/ARM
compile checks cover this slice. Whole-image RAM/stack/flash and performance
gates, maximum configurations, browser matrices and physical qualification are
deferred to the integration batch. Resource limits have not been increased.
