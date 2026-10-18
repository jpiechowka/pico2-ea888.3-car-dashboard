# Build and flash from View Studio

This is the design and acceptance contract for the Build & Flash workspace. The native helper and Studio workflow are implemented; final browser/image qualification and physical validation are tracked separately. The current entry points are documented in [setup](setup-tools.md) and the [README](../../README.md#build--flash-from-studio). A successful host check does not establish hardware operation.

## Board identity and helper compatibility

The supported board is **Raspberry Pi Pico 2 W**. USB descriptors and chip
inspection do not distinguish it reliably from every other RP2350 board or
measure flash capacity. The owner must confirm this model and its manufacturer
specified 4 MB flash for the selected inspection. The conservative 2 MiB firmware
layout and runtime JEDEC checks remain in force.

Helper protocol **2** uses `genuine_pico2_w` board consent. Protocol 1 and its
`genuine_pico2` consent are rejected; refresh Studio and restart the updated helper
after upgrading. No saved confirmation is migrated. The `/api/v1/` route namespace,
project/configuration formats and `pico2` Cargo/script identifiers stay compatible.
Existing flash journals remain readable through a separate historical decoder;
legacy records cannot create current consent or resume an operation.
That identity-only correction preceded the separate [BLE prototype](ble-prototype.md).

## Workflow

The Build & Flash workspace sits alongside Views, Device, Data & warnings and Diagnostics, with a link from Device settings. The build uses the accepted configuration only; invalid or unapplied JSON edits block a new build or flash and remain visible in the diagnostic preview. The user chooses Demo or Vehicle, a named clock profile or Custom, and supported build options. A read-only details panel shows the actual rustup toolchain, `rustc -Vv`, Cargo version, target, features, optimization/outline/trace options, configuration hash, and exact executable arguments and relevant environment overrides.

The project's dated nightly is required. Resolve it and report the installed compiler used by the build. A toolchain update is a separate visible operation: an in-flight build keeps its resolved compiler. Missing components, unavailable nightly, build failures and unsupported options are reported without silently selecting another compiler or profile.

Custom firmware embeds the selected configuration, so it requires a local build. Studio's static server runs on loopback port 8080; the native Rust helper exposes its typed API separately on 127.0.0.1:8765. It launches fixed Cargo/build-validation/picotool operations with argument arrays. It never accepts arbitrary shell text, arbitrary executable paths or unrestricted environment variables. The PowerShell entry point starts each service explicitly. The static editor remains usable for configuration export and command inspection without the helper.

Flashing is initiated from the web UI after a successful build, device selection and review of the artifact summary. The approved native path uses BOOTSEL and verified picotool loading; this also works in Firefox. If native USB access fails, offer a validated UF2 download and a specific driver/access diagnostic. Never silently choose another connected device. The user selected the local helper workflow; direct WebUSB is outside this delivery.

## Prerequisite setup

The launcher checks the computer running the helper, not the browser user agent. Check rustup, the selected dated nightly, rustc/Cargo identity, the appropriate host linker, the ARM and Wasm targets, rustfmt/Clippy where required by validation, Rust source and LLVM tools, the pinned Trunk build tool, Python and modules used by the existing artifact gates, and picotool with its supported command capabilities. Check the repository/workspace and lockfiles too. Report missing, incompatible, inaccessible and unsupported separately. An installed executable alone does not prove that a build or USB operation will work.

Provide an explicit setup/update action using official upstream distributions, recorded versions and integrity verification where upstream publishes it. The bootstrap must run before compiling the native helper; a missing Rust installation cannot be diagnosed only by an unavailable Rust application. Keep compiler installation separate from firmware jobs. A setup error leaves a readable, retryable report and cannot enable a stale artifact. Driver changes and privileged host dependencies need clear instructions or an explicit user action, never a silent system modification.

The native helper repeats relevant checks before a job. Resolve tool paths at startup, validate versions/capabilities and capture them in the manifest. Use the resolved executable and arguments throughout that job. Show actual toolchain/target availability, rather than reporting an expected pin as installed. Test missing PATH entries, incompatible versions, failed downloads/installs, offline operation with valid cached tools, directories with spaces, permission errors, interrupted setup and a working tool installation. Never report flashing readiness until the selected device is accessible; distinguish that from build readiness.

## Profile catalogue and validation

Use one Rust profile catalogue and clock resolver for the editor, native helper and firmware build. Every named profile also has a Cargo feature and build/run alias in `firmware/.cargo/config.toml`; the PowerShell wrapper selects the same catalogue entry. Keep existing names compatible. Reject multiple profile features and a profile combined with custom clock overrides.

Preserve these existing profiles, showing requested and achievable clocks separately:

| Profile | Requested CPU / SPI, MHz | Achievable CPU / SPI, MHz | VSEL | Status |
| --- | --- | --- | --- | --- |
| stock | 150 / 62.5 | 150 / 37.5 | 1.10 V | Existing default; panel clock exceeds generic ST7789 timing specification |
| 250-62.5 | 250 / 62.5 | 250 / 62.5 | 1.10 V | Experimental CPU and panel clocks |
| 280-70 | 280 / 70 | 280 / 70 | 1.30 V | User reports working; retain in Expert mode; voltage exceeds absolute maximum |
| legacy 290-72.5 | 290 / 72.5 | 290.4 / 48.4 | 1.30 V | User reports failure; PLL rounding changes SPI divisor; never recommend automatically |
| 300-75 | 300 / 75 | 300 / 75 | 1.30 V | Experimental and unverified; voltage exceeds absolute maximum |

Add candidate profiles 132/66/1.10 V and 140/70/1.10 V, plus a 150/15/1.10 V timing reference. The first two use SPI divisor two and nominal core voltage; their panel clock remains outside the generic controller specification. They are test candidates, not proven improvements. The timing reference is for comparison and does not claim whole-device electrical qualification.

Custom settings are bounded integer values, with checked parsing and arithmetic. Resolve the 12 MHz crystal, legal PLL feedback/post dividers, peripheral clock, SPI prescaler and serial clock rate before building. New custom CPU requests must be exactly representable: do not silently use Embassy's nearest-frequency fallback. Show attainable SPI alternatives and the selected actual rate when a requested maximum rounds down. Do not permit zero, non-finite, overflowing or out-of-range inputs. Keep independent USB/ADC clock sources and flash timing assumptions unchanged unless separately reviewed.

The standard path selects stock settings. Expert mode explicitly exposes out-of-spec clocks and regulator selections. Preserve 280/70/1.30 V as requested. Do not disable the RP2350 voltage limiter or offer a value above the existing 1.30 V ceiling. Voltage encodings are discrete; arbitrary decimal values are invalid. A 1.30 V encoding being supported by the HAL is not evidence that it is safe. The UI distinguishes nominal operation, out-of-spec operation, the absolute maximum and device-specific observations.

## Divisors, performance and measurement

SPI is derived from `clk_peri / (CPSDVSR * (SCR + 1))`, with an even prescaler 2 through 254 and a second divisor 1 through 256. CPU divided by four is one current choice. At 280 MHz, requesting 69 MHz yields about 46.7 MHz, while requesting 70 MHz yields 70 MHz. Current Embassy resolves the legacy 290 MHz request to 290.4 MHz, so its requested 72.5 MHz SPI falls to 48.4 MHz. The resolver and profile descriptions must expose these discontinuities.

Show transfer cost as a labelled estimate. A complete 320 x 240 RGB565 payload is 153,600 bytes: at 70 MHz it requires at least 17.55 ms, before command bytes, serial frame gaps, DMA/executor overhead and rendering. The ideal complete-payload ceiling is about 56.97 transfers/s. Partial updates can complete faster; neither number measures panel scanout or visible FPS.

Compare profiles using the same firmware source, configuration, sample sequence and screen. Cover full-frame and retained rendering, Dashboard, Debug and Logs, warnings, input activity and acquisition load. Record actual clocks, payload bytes, completion-rate windows, render/flush timings, CPU cycles, input latency, missed/drop counters and watchdog behavior. Compare overlapped and diagnostic serial display modes. Lower CPU clocks can leave wire time unchanged while slowing rendering and other tasks; host timings cannot predict that tradeoff reliably. Keep new recommendations provisional until target results and display integrity support them.

## Job and device boundaries

### Visible status and stage logs

Always show the current operation, a textual status, elapsed time, and a stage timeline. The stages are prerequisite checks, configuration snapshot, compilation, artifact validation, device detection and selection, transfer, verification, and reboot request. Setup/toolchain installation runs through the explicit PowerShell bootstrap with terminal progress; no browser Setup endpoint is exposed. Each job stage is Pending, Running, Succeeded, Failed, Cancelled, or Skipped with a reason. Do not rely on colour alone. Flash success requires successful verification; reboot requested and observed running firmware are separate facts.

Each operation has a fixed typed stage layout. Build shows Prerequisites, Snapshot, Compilation and Validation. Flash shows Prerequisites, Validation, Device selection, Transfer, Verification and Reboot. The retained typed Setup model defines Check, Download, Verify, Install and Validate with bounded per-tool substeps, but is not a deployed HTTP operation. A prerequisite check or device scan has its own single stage. Only stages belonging to that operation appear; interrupted pending stages show Skipped with a reason.

Every stage emits timestamped start, meaningful progress, and terminal records with a job ID and stage ID. Stream bounded stdout/stderr and structured tool results into the stage's expandable log, with filtering, copy and export. Show a truncation/dropped-record count when bounds are reached. Preserve failed and completed jobs through navigation and browser reconnection while the helper remains alive; an expired or restarted helper session is explicitly unavailable, not a successful job. Redact session capabilities and avoid dumping unrelated environment variables.

Build and Flash are distinct explicit operations linked by the immutable artifact ID. A successful Build ends at Ready to flash; it does not imply a device was written. A Flash job checks that artifact and the selected device again, then runs separate transfer, verification and reboot commands so the timeline represents actual stage boundaries. There is no combined load-and-reboot command that could reboot before the separately displayed verification stage. A failed verification leaves reboot skipped. A reboot-command failure preserves the fact that bytes verified and reports that the device has not been confirmed running.

Keep at most 16 job summaries and 1,024 log records per retained job, each with at most 1,024 UTF-8 bytes. Retain the latest terminal summary separately from the rolling log. Events carry a monotonically increasing sequence, UTC timestamp and monotonic elapsed time. Poll batches contain at most 128 records and explicit oldest/next cursors and dropped counts. Splitting long output lines and carriage-return progress must remain bounded; do not wait for an unlimited newline or collect the complete process output before publishing it. Render all tool output as text. Redact the session capability before records reach either the rolling buffer or console.

The browser polls while the helper is connected and a job is active, with slower polling while hidden. Finalization closes the event stream and publishes its immutable final cursor after draining or accounting for pending output. The browser drains the remaining bounded batches, then stops automatic polling; Refresh explicitly fetches completed history. It deduplicates records by session/job/sequence and cancels obsolete requests. Network errors display Disconnected while retaining the last known status; they cannot complete a stage. A different helper session ID marks old jobs Unavailable until explicitly selected from any retained evidence. The log view has a pinned stage summary, collapsible output, level/stage filters, copy/export, auto-scroll toggle and a new-record count while paused. Stage transitions use a polite live region; streaming every log line through that region would drown out meaningful announcements.

Parse protocol responses as text through Rust serde so 64-bit sequence, revision and elapsed counters stay exact. UTC is optional when the host clock cannot produce a valid timestamp. Browser date formatting uses checked conversion and retains exact integer text outside its supported range; unavailable time is never displayed as a fabricated epoch timestamp. Ordering follows sequence and monotonic elapsed time.

Only display percentages when the underlying operation supplies a meaningful total. Use an indeterminate indicator for Cargo compilation and device waiting; never invent an overall percentage from unrelated stages. Keep the log pane responsive through bounded batching and optional auto-scroll, with a visible new-record indicator when the user scrolls back. Failure identifies the failed stage, actionable cause, command exit status when available, and retry prerequisites. A retry creates a new job linked to the previous attempt; it cannot hide a failed verification or silently reuse stale firmware.

Each build snapshots the configuration bytes, compiler identity, profile resolution and option list. Its ID identifies an immutable artifact plus source/configuration/ELF/UF2 hashes and validation results. Changes during compilation cannot replace the pending request. A changed editor revision makes the previous artifact visibly stale and requires rebuilding before flashing the current configuration.

Use the existing ELF, flash-helper, static RAM and stack gates. No failed, incomplete or mismatched artifact becomes flashable. Reject wrong chip/family, unexpected flash ranges, reserved settings/partition/OTP writes, ambiguous devices and changed device identity. Firmware selection and configuration data cannot supply paths or shell arguments. Do not expose erase-all, OTP, boot security changes or a generic memory-writing interface.

Only one write job owns a device. Disconnects, permission failures, timeouts, cancellation and application restart lead to explicit states. Cancellation before writing is immediate; do not kill an active flash write and report success. A completed transport is distinct from verified bytes, requested reboot and observed firmware operation. Never infer running firmware merely from the BOOTSEL device disappearing.

During transfer, Cancel becomes Cancel requested and explains that the current device write must finish before the job can stop. Keep the child process owned by the helper even if the browser disconnects. A forced termination, timeout or lost device leaves the write outcome unverified; it cannot enable reboot or show success. Graceful helper shutdown waits for a critical transfer boundary and records any skipped verification/reboot. A retry always receives a new job ID and repeats the applicable prerequisites and identity checks.

Bind the helper to loopback, enforce the expected Host/Origin and a per-session capability, limit body sizes and job history, reject path traversal and cross-origin state changes, and serialize build/flash resources. Include bounded progress and logs in Diagnostics without leaking session tokens or arbitrary environment contents. The copyable commands are informational and reflect the exact typed request.

## Acceptance

- Shared resolver tests cover all catalogue entries, legal/exact PLL combinations, rounding boundaries, integer extremes, invalid voltage encodings and feature conflicts.
- Cargo features, aliases, wrapper and web catalogue must resolve identically. Build the new candidates and preserve the legacy profile names with truthful metadata.
- Exercise build snapshots, changed drafts, missing toolchains, process failures, cancellation, stale results, malformed requests, device ambiguity/disconnect and verify failures using deterministic adapters; no unit test writes a real device.
- Browser checks cover native-helper presence/absence, read-only command details, profile/custom validation, invalid-draft blocking, progress/failure recovery and keyboard access.
- Exercise each stage's start/progress/success/failure/cancellation/skip records, monotonic ordering, bounded log overflow, reconnect/resume, helper restart, tool stdout/stderr errors, honest indeterminate progress, and keyboard/screen-reader status announcements. No flashing success state may appear before verified bytes.
- Hardware flashing and profile comparison require an explicit user operation. Record exact image and device identity; keep physical performance and DNUC support unverified until measured.

## Primary references and exact source

- [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf), sections 12.3.3.3 and 12.3.4.6.1 for SPI; section 14.9.1 for 1.21 V absolute maximum and section 14.9.5 for the nominal supply range.
- [Raspberry Pi SPI source](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_spi/spi.c) and [clock source](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_clocks/clocks.c); exact local `firmware/vendor/embassy-rp/src/{spi,clocks}.rs` is the build authority.
- [ST7789V v1.2](https://files.seeedstudio.com/wiki/Grove-1.2inch-ips-display/ST7789V_SPEC_V1.2.pdf), page 43: 66 ns minimum serial write cycle under the listed conditions.
- [Pico BOOTSEL documentation](https://www.raspberrypi.com/documentation/microcontrollers/pico-series.html), [picotool load/verify](https://github.com/raspberrypi/picotool#load), [WebUSB requirements](https://developer.chrome.com/docs/capabilities/usb).
- [Rustup Windows host prerequisites](https://rust-lang.github.io/rustup/installation/windows-msvc.html), [Axum request-body limits](https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html), and [Tokio subprocess ownership](https://docs.rs/tokio/latest/tokio/process/struct.Command.html). Select and pin releases after checking the registry, release notes and exact source; a framework default is not the helper's security policy.
