# Display orientation: source contract and physical checks

This guide separates the implemented command/ownership contract from measurements
still required on the Pico 2 W and PIM715 Display Pack 2.8. The [Studio guide](../configurator/orientation.md)
describes default/override selection, native grids and custom artwork conversion.
No physical acceptance result is implied by the source checks below.

## Controller contract

| Orientation | MADCTL | CASET inclusive | RASET inclusive | Offset |
| --- | --- | --- | --- | --- |
| Landscape | `0x60` | 0..319 | 0..239 | 0,0 |
| Portrait | `0x00` | 0..239 | 0..319 | 0,0 |
| LandscapeInverted | `0xA0` | 0..319 | 0..239 | 0,0 |
| PortraitInverted | `0xC0` | 0..239 | 0..319 | 0,0 |

Portrait means turning the board clockwise from the existing landscape mounting;
PortraitInverted is the opposite direction. Board rotation and content-rotation
names in another driver can describe opposite directions. Verify with asymmetric
labels on the actual assembly, not a guessed connector-at-top convention.

The implementation preserves the existing MADCTL ML/MH/RGB bits. In particular,
it retains ML=0 rather than copying Pimoroni's landscape `0x70` literal. COLMOD,
RAMCTRL, gamma, inversion and TE policy retain their prior settings. Both shapes
contain 76800 RGB565 pixels (153600 bytes) and use the full 240 × 320 controller
memory. Offsets from smaller ST7789 panels do not apply.

Source evidence:

- [ST7789V specification v1.2](https://files.seeedstudio.com/wiki/Grove-1.2inch-ips-display/ST7789V_SPEC_V1.2.pdf),
  sections 8.11/8.12 and Figure 33: MX/MY/MV address mapping and required RAM
  rewrite when those bits change; sections 9.1.20–22 and 9.1.28: inclusive
  windows, RAMWR and MADCTL. The reviewed PDF SHA256 is
  `06b07863bc47c032f93ac1c328a13e941b506d68dd79cea5812e4e0d7f72b966`.
- [Pimoroni driver at b41010d1](https://github.com/pimoroni/pimoroni-pico/blob/b41010d1e9809a62c878e067d7e17ae56f3c4684/drivers/st7789/st7789.cpp)
  corroborates the native windows and address modes. The [PIM715 specification](https://shop.pimoroni.com/products/pico-display-pack-2-8)
  identifies the board; neither source identifies the user's installed mount.
- The resolved local Embassy RP 0.10.0 [SPI implementation](../../firmware/vendor/embassy-rp/src/spi.rs)
  arms DMA before first suspension and waits for SPI BSY to clear after DMA
  completion, then drains RX state before returning. Its SHA256 is
  `dc3e20729f5486318c931b29c07d617ed93136617885819740d005f50fccefca`;
  the [DMA implementation](../../firmware/vendor/embassy-rp/src/dma.rs) SHA256 is
  `38683e0873573e158b32800a7a4524be19b51a9f5c21e3fbf8407ba6ab4a535f`.
  This identifies the checked source, not a general cancellation guarantee.

The flusher owns the immutable request through completion. On an orientation
change it requires a full plan, sends MADCTL, sets CASET and RASET, then sends RAMWR
and the full payload. No subsequent mode command can interleave with the previous
payload. A same-orientation update does not emit redundant MADCTL. Controller mode
and successfully completed content are tracked separately; a failed command or
payload cannot authorize partial updates or completion effects. Timeout handling
does not cancel an active DMA transfer or return its buffer for rendering.

Host panel models check both strides, all four address modes, identical-black
transitions, rapid alternation, delayed completions and failures at command/payload
boundaries. The actual SPI Error enum has no variants: injected host failures test
the ownership protocol, not physical link-error detection. Normal builds do not
synchronize to TE, so correct command order does not promise atomic or tear-free
scanout. See [partial updates](../performance/partial-updates.md).

## Physical acceptance matrix

Use an explicitly selected, captured image and the power/wiring/flash procedure in
the [device runbook](../validation/pico2-dnuc-runbook.md). Record its source revision,
ELF/config/profile/asset hashes, clock profile, panel identity, supply, instrument
settings and actual mounting. Begin at stock clocks and repeat at each clock
profile being accepted. Source review and host tests do not substitute for this run.

1. **Corners and colours:** display distinct TL/TR/BL/BR labels, a directional
   arrow, RGB primaries, low-green and grayscale ramps in all four modes. Check
   every edge and the last pixel, including portrait row 319. Reject mirrors,
   cropped edges, row wrap or stale landscape content. Record photographs/video
   and physical board rotation. Follow the [colour check](display-color.md).
2. **Whole screens:** exercise Boot info, each procedural Splash, custom Splash,
   all dashboard spans and holes, warnings/combined warnings, brightness and
   feedback popups, Debug and scrolled Logs in both shapes. Give Boot info, Splash
   and the selected view different overrides; verify Debug/Logs use the default.
   Check skip, startup fault/recovery and popup removal without stale borders.
3. **Transitions:** rapidly alternate views/pages of different orientations while
   transfers are pending; include landscape → portrait → landscape and identical
   black images. Check complete replacement before subsequent partial updates,
   stationary same-mode idle and correct pixels after repeated buffer reuse.
4. **Physical controls and completion effects:** test the captured A/B/X/Y
   bindings and encoder direction/click in each mounting. Y hold must still
   recover Dashboard and Full brightness after two seconds. Check startup input
   consumption, warning LED timing, Full/Dim/Off and the final completed clear
   before Off. Delaying one frame must not apply a newer frame's completion effects.
5. **Electrical ordering:** capture CS GP17, D/C GP16, SCK GP18 and MOSI GP19;
   include TE GP21 and backlight GP20 where relevant. Require the previous final
   SCK edge before its CS release and before the next MADCTL (`0x36`). Then require
   CASET (`0x2A`), RASET (`0x2B`), RAMWR (`0x2C`) and 153600 payload bytes with the
   table's bounds. CS stays low through the payload. Same-mode narrow rectangles
   must use the native stride; they must not send MADCTL.
6. **Timing and stability:** capture transition tearing/scan behavior optically,
   update-interval percentiles and input latency under representative changing
   content and stationary workloads. Keep analyzer traces and raw samples.
   Compare matched images/options at stock and accepted overclocks, including the
   serial/full-frame fallback, and run the device runbook's soak checks.

Host render times measure host CPU work. SPI payload counters measure transfer
work. Neither is physical FPS or a panel scanout measurement. A passing display
matrix qualifies only the recorded image and hardware conditions; it does not
qualify ECU transport, DNUC signal decoding or transmission support. Complete the
separate vehicle procedures before making those claims.
