# Dashboard backlight and warning indicator

Default Dashboard encoder clicks cycle Full, Dim, Off, Full. The duty settings are
100%, 50% and 0%; they do not represent perceived luminance. Every firmware boot
starts Full, including when a legacy journal contains an Off value. Unit/FPS
settings retain persistence; brightness is runtime-only. Rotation selects
configured views and has no effect with a single view.
The immutable boot configuration supplies per-screen button and encoder bindings;
see [the defaults and action rules](../configurator/control-mapping-design.md).
Onboard taps commit on stable release, ordinary assigned holds fire once after
800 ms by default, and every hold suppresses its release tap. Hold Y continuously
for two seconds to recover Dashboard and Full brightness from any screen,
including Dashboard Off. Recovery retains the selected view and cancels pending
shutdown/feedback; it is fixed and independent of the optional encoder.
Held keys at boot or after maintenance must release before acting.

Full and Dim feedback lasts 1.5 seconds. Off feedback stays lit for two seconds
measured from the first successful transfer of that popup. If its pixels already
match the completed reference, the no-damage iteration starts the timer instead.
The first popup-free frame must complete before PWM is set to zero. Affine frame
tokens reject earlier/wrong-buffer completions; submission alone never authorizes
Off. A subsequent click wakes Full and invalidates the old pending completion.
Page changes or other feedback cancel a pending dashboard shutdown, preserving
the prior lit level. A failed transfer cannot release a shutdown; normal display
failure recovery still applies.

Debug keeps its previous rotation and on/off controls, including 1.5-second
percentage feedback. Its old 0..100 slider maps nonzero values into roughly
45..100% duty. Rotating away from Dim starts at the nearest old slider position,
9 (49.95% duty), then applies the usual 5-point step. Debug off/on without
rotation restores the exact preset. Logs scrolling and its ignored click stay
unchanged. Main owns all PWM and RGB writes; the encoder task only publishes
bounded debounced input batches.

## Electrical/source checks

1. The [PIM715 schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048)
   connects GP20 to the AP2502 active-high EN input. RGB has a common 3V3 anode
   and 330-ohm cathode resistors to GP26 red, GP27 green and GP28 blue. These GPIO
   channels are active low. Red and blue initialize High (off), then green
   initializes Low (on) when firmware takes ownership.
2. The [AP2502 datasheet](https://www.diodes.com/datasheet/download/AP2502.pdf),
   pages 2 and 5, specifies PWM up to 150 kHz, minimum high 1 us and low 100 ns.
   The retained 1 kHz configuration supplies 500 us high/low at Dim.
3. [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
   section 12.5.2 defines noninverted output as CC greater than the counter.
   TOP=9999 therefore uses CC=10000 for Full, 5000 for Dim and 0 for Off.
   Running CC writes latch on wrap, so a scheduled change can take one more
   approximately 1 ms PWM period. Keep the slice enabled: disabling can freeze
   its previous output level.

The exact vendored Embassy RP 0.10.0 implementation was checked against these
specs: `pwm.rs` Config defaults and `configure` preserve enabled, noninverted,
non-phase-correct operation; `set_config` writes CC without waiting for wrap.
`gpio.rs` writes initial levels before enabling output and uses SIO SET/CLR for
subsequent levels. The published [PWM source](https://docs.rs/embassy-rp/0.10.0/src/embassy_rp/pwm.rs.html)
and [GPIO source](https://docs.rs/embassy-rp/0.10.0/src/embassy_rp/gpio.rs.html)
provide upstream reference. No dependency or clock profile was changed here.

## Indicator and physical limits

The last completed frame's effective overlay determines the warning indicator.
Its red phase lights red; its white phase turns red off. Green is suppressed
throughout either warning phase. Feedback covering a warning, diagnostic pages
and backlight Off use continuous green. Blue remains off. Green indicates no
visible warning; it does not certify sensor availability or engine health.
Future queued/rendering frames cannot change the indicator early.

The pure indicator state is exclusively Off, Green or Red. GPIO application
disables the previous color before enabling another. Completion paths apply it
before settings maintenance can await or continue, including serial and idle
presentation. The encoder task and acquisition core never own these pins.

Completion means the HAL returned after the transfer. LCD scanout follows its
own timing, and GPIO/PWM application and optical response have additional delay.
These contracts do not prove exact optical dwell, scanout synchronization,
physical FPS or overclock stability. Validate the visible cycle, cancellation,
RGB phase and measured GP20 duty/frequency on the actual device.
