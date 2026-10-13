# Configurable controls and recovery

## Scope

Per-screen mappings cover A, B, X and Y tap/hold, encoder rotation and encoder click. The three screens are Dashboard, Debug and Logs. Y hold is reserved: a stable two-second hold returns to Dashboard, retains the last selected view and applies Full brightness. It cancels pending shutdown and feedback. Configuration cannot disable, shorten, reassign or depend on the encoder for this recovery.

## Configuration contract

`dashboard_config::ControlPage` is the shared screen enum with `ALL`, `index`, `next`, `previous` and compatibility `toggle`. `ButtonId` is A/B/X/Y with `ALL` and `index`. `ControlAction` is a finite enum: None, NextScreen, PreviousScreen, Dashboard, Debug, Logs, NextView, PreviousView, ToggleBoostUnits, CycleFps, ResetStatistics, CycleBrightness, ToggleBacklight, BrightnessUp, BrightnessDown, ScrollOlder, ScrollNewer, LogsLive, AcknowledgeWarnings. There is no arbitrary command execution or custom ECU write action.

`RotationAction` is None/Views/Brightness/Logs. `ButtonBinding { tap, hold }`, `ScreenBindings { buttons: [ButtonBinding; 4], encoder_rotation, encoder_click }` and `ControlMap { hold_ms: u16, screens: [ScreenBindings; 3] }` use fixed arrays, serializable enums and no strings. `ControlMap::default()` defines the defaults below. Ordinary holds accept 400..=1500 ms, default 800 ms. Y's serialized hold must be None, and the UI instead displays its locked recovery action.

Navigation actions work on every screen. Dashboard also permits views, units, FPS, statistics reset, warning acknowledgment and brightness cycle. Debug permits brightness up/down and backlight toggle. Logs permits older/newer/live log navigation. None is valid everywhere. Encoder click uses the same compatible action list. Rotation is None everywhere, Views only on Dashboard, Brightness only on Debug and Logs only on Logs.

| Screen | A tap / hold | B tap / hold | X tap / hold | Y tap / hold | Encoder rotate / click |
| --- | --- | --- | --- | --- | --- |
| Dashboard | Boost units / previous view | Reset statistics / next view | FPS / brightness cycle | Next screen / fixed recovery | Views / brightness cycle |
| Debug | None / brightness down | None / brightness up | None / backlight toggle | Next screen / fixed recovery | Brightness / backlight toggle |
| Logs | None / older logs | None / newer logs | None / live logs | Next screen / fixed recovery | Logs / None |

Validation rejects incompatible actions, out-of-bounds hold duration and any non-None Y hold. A bounded three-node reachability check uses onboard button actions only plus the immutable recovery route to Dashboard. Dashboard must reach Debug and Logs. At least one onboard Dashboard binding must select NextView or PreviousView: view selection wraps, so either direction reaches every view. This requirement also applies to a single-view configuration so later view additions cannot silently remove fallback navigation. Encoder bindings never satisfy these guards. An enabled latch or snooze requires AcknowledgeWarnings on an onboard Dashboard tap/hold; an encoder alone is insufficient. The command targets the last completed warning page and reports Pending/Busy until its matching producer result arrives.

Errors identify duration, screen/button/gesture, rotation, missing screen route or missing view route. The editor allows invalid mappings to be repaired and marks the preview invalid; it never disables the entire repair form because of its current errors. Runtime consumes a validated map; invalid embedded configuration takes the existing logged default fallback.

## Gesture and runtime contract

- Active-low buttons use existing 50 ms debounce. A tap is committed on stable release so it cannot also invoke a hold. Holds fire once, never repeat, and suppress the release tap. A button held at boot or after maintenance must release before it can trigger a new gesture.
- If an ordinary button has no hold assignment, a long press still commits its tap on release. Y always has fixed recovery instead. A sampled release cannot itself trigger a hold: a delayed render tick must not reinterpret an earlier short press as a two-second recovery. The current sampled level must still be pressed to fire a hold.
- Time regressions do not fire actions or age a hold. Hold duration is measured from the first observation of the subsequently debounced press. Rejected release bounce cannot cause a tap. Y recovery requires a continuously held debounced input and must not fire before two seconds.
- One input transaction captures the current screen. Recovery wins over all other simultaneous actions; a screen transition wins over other actions in the same batch. Remaining actions use deterministic A/B/X/Y order, then encoder motion/click. Discarded events are counted. A screen change cancels old-screen gestures and encoder batches so they cannot execute on the new screen.
- Recovery disarms every held key until release even when already on Dashboard. Self-navigation is an intentional no-op and does not suppress unrelated actions; two real transitions in the same transaction use the first in A/B/X/Y order and discard the rest.
- The shared dispatcher returns bounded actions/effects; firmware performs hardware I/O, statistics reset signaling and observation accounting. The simulator uses the same dispatcher. Statistics-reset feedback remains gated by Core1's matching acknowledgment.
- View selection survives screen changes. Recovery wakes Full from every backlight phase and invalidates pending Off completion tokens. Normal Dashboard Off retains the existing two-second popup measured from completed presentation, followed by a completed clear frame before PWM zero.
- Debug's default encoder brightness adjustment and Logs scrolling remain available, with onboard fallback holds. Maintenance discards outstanding gestures; no old release executes a remapped/new-screen action.

## Release gates

Test exact defaults, every enum/screen compatibility pair, missing routes including encoder-only layouts, reserved Y holds, hold-duration boundaries, malformed imports, codec round trips and historical migration. Runtime tests cover debounce/bounce, startup-held buttons, short tap, threshold hold, long hold, release suppression, reversal/stalls, maintenance, simultaneous recovery/page change and interrupted Off states. Browser and firmware must use the same action semantics. Target builds must preserve stack and ownership guards; host verification is not hardware timing qualification.
