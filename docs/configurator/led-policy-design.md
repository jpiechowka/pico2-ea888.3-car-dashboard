# Configurable RGB indicator

This implements approved decision 2. Normal and warning operation have independent colour, brightness and Off, Solid, Blink or Breathe patterns. The defaults remain steady green and red blinking with the warning actually displayed. Configuration, the browser simulator and firmware use one policy implementation.

## Configuration and editing

Each pattern stores a native RGB565 colour, brightness from 0 through 100 percent, pattern kind, period from 200 through 10,000 milliseconds and blink on-time from 10 through 90 percent. Colours use the existing #RRGGBB editor and show the quantized result. LED colours have no text-contrast requirement; Off, zero brightness and black are valid explicit choices. Hidden timing fields remain bounded so switching pattern cannot activate malformed values.

Normal defaults: Solid, #00FF00, 100 percent. Warning defaults: Blink, #FF0000, 100 percent, 400 ms period, 50 percent on-time. A warning option synchronizes Blink to the completed popup phase; it defaults on. Independent timing applies when synchronization is disabled or a background-warning policy has no visible popup to follow. The editor explains this distinction and disables irrelevant controls without deleting their stored values.

Separate switches allow active warnings to remain indicated during Dashboard feedback, on Debug, on Logs, and with the backlight off. All default off. The off-screen switch is a prerequisite when the backlight is off; the applicable page/feedback switch must also permit a hidden warning. With no warning selected by the policy, use the normal pattern. Warning Off means no LED light while a warning is selected, not a return to the normal colour.

Warnings use existing source, freshness, applicability and configured view-scope checks. Extract a bounded shared active-rule query for both warning presentation and LED selection; do not recreate ECU threshold rules in the LED adapter. Stale data cannot keep a non-latched warning active. Future approved latching/acknowledgment rules will feed the same query.

## Presentation and time

Extend the existing presented-frame tracker to capture page, visible overlay and eligible-warning state with the submitted FrameToken. A matching completion commits that state. Wrong, duplicate or superseded completions cannot change the LED. An unchanged scene can update its equivalent presentation without inventing a transfer. A pending page transition must not prematurely change the LED to the destination page's policy.

The pure controller computes three light-duty values from 0 through 10,000. Its animation epoch changes when normal/warning selection changes, not on every frame. Repeated timestamps are stable; regressed timestamps preserve the last output and cannot reset an epoch. Large gaps use bounded modulo arithmetic, never a catch-up loop. Blink starts on. Breathe uses a bounded integer smoothstep over a triangular phase, avoiding floating point and allocation. The same elapsed time yields the same duty regardless of repaint frequency.

Default synchronized warning blinking follows the committed overlay's phase rather than advancing ahead of the displayed warning. Other patterns advance by monotonic time using the committed selection. The later reduced-motion option makes decorative breathing solid; explicitly configured warning blinking remains available, and users can choose Solid or Off.

## Hardware adapter

PIM715's common-anode RGB LED has active-low cathodes on GP26, GP27 and GP28. Use one owned PWM slice 5 for its A/B red/green channels and one owned slice 6 for blue channel A. Backlight keeps slice 2. There is one Core0 owner and no additional executor, DMA channel, interrupt or shared peripheral handle.

Use TOP 9,999, divider 1, phase-correct disabled and inverted outputs. With inversion, compare 0 is off and compare 10,000 is full light. Initial configuration sets all compares to zero before the pins are assigned to PWM. The carrier is the actual system clock divided by 10,000; it is distinct from the much slower selected blink/breathe period. Retain owners for the application lifetime and update only changed duties. Use separate A/B compares in the reviewed HAL configuration; its whole-slice SetDutyCycle operation writes both channels and cannot represent independent red/green colours.

Do not use PWM Drop or disabling a slice as the LED off operation: the HAL disconnects pads with pull-down, which is unsuitable as an active-low off policy. Do not change regulator, display clocks or backlight algorithms in this stage.

The shared controller, owned PWM adapter and Studio RGB indicator panel are implemented locally. Final release integration, physical colour, carrier and startup validation remain pending. Integer conversion and smoothstep use nearest rounding with ties upward.

## Verification

- Test all pattern endpoints, period/on-time/brightness boundaries, RGB primaries and mixed colours, zero light, rounding and channel independence.
- Replay irregular, repeated, regressed and near-overflow timestamps and long stalls; compare trajectories across repaint schedules.
- Cover normal/warning transitions, every page, user feedback, backlight Off/Full, stale/cleared warnings and every background-policy switch combination.
- Verify pending/matching/wrong/duplicate frame completion, unchanged scenes and two-frame backlight shutdown ordering.
- Test strict configuration bounds and migration defaults, repairable invalid editor inputs, undo/import/save/export, and simulator output from the same controller.
- Build the actual target, inspect PWM ownership and initialization in the locked source, and run resource gates. Benchmark the pure output path. Physical colour, electrical duty/carrier, startup transient and long-duration behavior remain device validation.

## Source basis

- [PIM715 product](https://shop.pimoroni.com/products/pico-display-pack-2-8) and [schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048): RGB wiring and pins.
- [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf), GPIO function table and section 12.5.2: PWM mapping, counter period, output inversion and exact endpoints.
- [Raspberry Pi PWM source](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_pwm/include/hardware/pwm.h) and the exact vendored `firmware/vendor/embassy-rp/src/pwm.rs`: construction, register updates, pin ownership and Drop behavior. The original HAL top-level period comment reversed the phase-correction factor; the tracked comment-only correction now agrees with the datasheet and field documentation.
