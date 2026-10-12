# Optional encoder input and recovery

The supported assembly is the Adafruit PID5880 preassembled encoder, with
PID4991 seesaw firmware, default address `0x36` and button on seesaw GPIO24.
Before GPIO configuration, the driver checks SAMD09 hardware ID `0x55` and the
high 16 bits of the big-endian firmware version for `4991`. Other identities
remain unavailable until their pin mapping is established.

## Three source checks

1. [Adafruit's board guide](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder?view=all)
   documents the address, GPIO24, firmware product check, and the reversed
   direction of the supplied PID5880 encoder. Raw deltas are now negated so
   clockwise increases the legacy Debug brightness setting. Dashboard rotation
   selects configured views. Direction remains to be checked physically.
2. [Arduino driver source](https://github.com/adafruit/Adafruit_Seesaw/blob/985b41efae3d9a8cba12a7b4d9ff0d226f9e0759/Adafruit_seesaw.cpp)
   and [encoder example](https://github.com/adafruit/Adafruit_Seesaw/blob/985b41efae3d9a8cba12a7b4d9ff0d226f9e0759/examples/encoder/encoder_basic/encoder_basic.ino)
   establish status/version, GPIO input/pull-up commands, four-byte big-endian
   deltas and the high-word product ID. The `getVersion` prose swaps product/date
   bit ranges, but `getProdDatecode` and both Arduino/CircuitPython examples use
   the high word for product. The firmware follows these implementations. The
   500 us select-to-read delay exceeds the Arduino default 250 us, while
   [CircuitPython](https://github.com/adafruit/Adafruit_CircuitPython_seesaw/tree/13ba8ba2a2cecb9df5a41390f991ce6c0cd76e14)
   defaults to 8 ms. The sources agree on the protocol, but this difference
   leaves timing acceptance to actual device tests under load.
3. Locked `embassy-rp` 0.10.0 `i2c.rs` was read through async transfer setup,
   FIFO waits, abort/STOP handling and IRQ masking/waking. These paths have no
   transfer-drop cleanup or deadline. The application therefore retains the
   pending I2C future and its owner. Independent input progress is required
   within 500 ms by the watchdog supervisor; a stalled transfer eventually
   resets the board instead of cancelling and reusing a half-finished bus.
   [Published HAL source](https://docs.rs/embassy-rp/0.10.0/src/embassy_rp/i2c.rs.html)
   and [watchdog contract](../performance/progress-recovery.md) define the limits.

## Bounded application policy

Each raw poll must be within +/-128 counts. Corrupt/extreme values are rejected
before negation, including `i32::MIN`. Pending motion saturates at +/-128 and
pending presses at eight; rejected/saturated events are counted. The UI consumes
each coherent value/timestamp batch once. Dashboard applies every press through
Full/Dim/Off; Debug preserves press parity for its on/off toggle. Dashboard
rotation no longer changes brightness. Legacy Debug brightness and Logs scroll
arithmetic use wide intermediates and clamp their result. Oversized
register writes are rejected instead of silently truncated.

Buttons must be stable for 50 ms. Held buttons at boot/reconnect do not generate
a press until released and pressed again. The producer discards stale rotation
on reconnect, clears pending events on failure, and retries ordinary errors
every 200 ms. Completed failures count as useful input-task progress; absence
alone does not cause a watchdog reset. A genuinely wedged I2C future does.

Host tests cover identity/endianness/direction, all integer extremes,
saturation/drop accounting, debounce/held reconnects/clock regression, and
100000 concurrently produced motion/press events. Watchdog tests independently
stall input while other tasks advance. Host tests do not prove bus recovery,
debounce timing under load, or physical direction. Validate unplug/replug, wrong
device/address, held/bouncing button, stuck SDA/SCL, and watchdog reset on the
actual assembly before hardware acceptance.

The timestamped mailbox replaces independent atomics with a bounded blocking
critical section, using the same reviewed RP2350 multicore provider as other
shared state. Only fixed scalar metadata moves under the lock. Rejected clock
regressions and reconnect discards are counted, and values cannot be consumed
with timestamps from a different batch. See [input correlation and its measured
host cost](../performance/frame-traces.md#input-correlation).

## Onboard transaction boundary

Main samples A/B/X/Y on GP12/13/14/15 with pull-ups and passes their
active-low levels to the shared gesture recognizer. The optional encoder batch
is taken before capturing input time. One ControlSession applies the captured
page's actions, with protected Y recovery first and screen transitions next.
A page-changing button discards same-batch motion/clicks rather than applying
them to the destination; held old-page releases cannot trigger new bindings.
Maintenance resets onboard gestures and clears pending encoder events. The
encoder owner also reinitializes its debouncer and consumes accumulated motion
at its existing resume boundary. Statistics feedback waits for the matching
acquisition reset generation, even when navigation occurs before acknowledgment.

The [official board definition](https://github.com/pimoroni/pimoroni-pico/blob/main/libraries/pico_display_2/pico_display_2.hpp)
lists A/B/X/Y as GP12/13/14/15. The [PIM715 schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048)
connects A/B/X/Y switches from GP12/13/14/15 to ground. Vendored Embassy RP
0.10.0 `gpio.rs` Input::new configures input then pull; Input::is_low delegates
to the SIO input bit being zero. The adapter retains these electrical settings.
The encoder's bounded driver mailbox and GPIO/PWM ownership are unchanged.
Tap/hold timing and recovery still require physical validation under load.
