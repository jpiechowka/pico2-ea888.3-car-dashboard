# Optional TE observations

Build with `--features te-trace,frame-trace` to enable the panel's vertical-blank
TE output and retain the most recent 64 software-observed rising-edge timestamps.
The normal build leaves TE disabled. This experiment changes neither FRCTRL2
nor the transfer scheduler. It does not establish a 60/90/120 Hz physical result.

## Three source checks

1. The [PIM715 schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048)
   connects LCD FPC2 pin 19 through LCD_TE and a 330 ohm RN2 resistor to Pico
   header pin 27, GP21. There is no inverter or external TE pull shown. The
   input therefore uses no internal pull and remains exclusively on core 0.
2. [ST7789V v1.2](https://files.seeedstudio.com/wiki/Grove-1.2inch-ips-display/ST7789V_SPEC_V1.2.pdf),
   pages 129-130 and 210, defines TEON `35h` with parameter `00h` as TEM=0,
   called Mode 1 in the datasheet: vertical blank only. High is blanking;
   physical consecutive rising edges measure frame period. Sleep holds TE low.
   Startup already sends SWRESET, which disables TE before the single TEON.
   TEON does not change an already enabled mode. MADCTL ML does not invert TE;
   visible scan orientation still needs measurement on the actual panel.
3. The exact locked [Embassy RP 0.10.0 GPIO implementation](https://docs.rs/embassy-rp/0.10.0/src/embassy_rp/gpio.rs.html)
   clears sticky raw edges when arming, then enables the selected event. Its
   ISR disables all events for that pin and wakes the waiter. InputFuture has
   no Drop cleanup. The task keeps one uncancelled rising-edge wait, records
   immediately after waking and rearms without logging or another awaited
   operation. No timeout or trigger switching is applied to the edge future.

The RP2350 [interrupt priority issue #6695](https://github.com/embassy-rs/embassy/issues/6695)
remains relevant to latency. Software observations include IRQ/executor delay;
edges during disarmed intervals or a delayed task can be missed. Ring overwrite
counts discarded **observations**, never a claimed count of lost physical
edges. No interpolation or filtering turns long gaps into guessed frames.
The connected TE line is driven; a disconnected/unpowered input may float.
RP2350 A2's [E9 erratum](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
also affects high-impedance Bank0 inputs. Observations must not be used as a
panel-presence or health assertion; verify the silicon revision and signal.

## Evidence and use

`tasks::te::TE_OBSERVATIONS` and `tasks::flush::FRAME_TRACES` share the monotonic
microsecond clock and can be inspected through a probe. Every two seconds,
the ordinary log path reports the last observed interval and age; before two
edges it explicitly reports that it is waiting. Logging happens after releasing
the short ring lock. An absent TE signal does not hold a framebuffer, stop DMA,
or affect watchdog eligibility. Duplicate/regressed observations are rejected
and counted; old entries are overwritten with a saturating counter.

Three host tests cover startup, absence, long gaps, regressed/equal/extreme
timestamps, wrap, invalid indices, bounded storage and overwrite accounting.
They validate the recorder only. Local target build and RAM checks validate
integration; they cannot validate the physical signal or interrupt latency.
Stock and 300 MHz releases with both trace features pass the ELF gate at
380192 static RAM bytes and 144096 bytes core-0 headroom. Strict feature-enabled
firmware Clippy and host Clippy pass. An independent source review found no
actionable defect in the edge task, initialization or bounded recorder.

On hardware, capture GP21, CS, DC and SCK together at stock clocks. Compare
physical edge intervals with software observations and frame traces; record
interrupt delay, missed edges, transfer duration, scan orientation, input
latency and visible tearing with camera/photodiode evidence. Repeat across
temperature and any chosen overclock. A TE-aligned start alone cannot prevent
tearing when a transfer crosses scanout. Keep controller-rate experiments
disabled until these measurements justify a separate reviewed change.
