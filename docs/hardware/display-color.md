# ST7789 RGB565 expansion setting

Initialization sets RAMCTRL (`0xB0`) to `00 C0` immediately after COLMOD.
The ST7789V reset value is `00 F0`. Changing EPF from `11` to `00` makes the
extra red/blue low bits zero during 16-to-18-bit color expansion, instead of
copying the low green bit into them. MCU interface mode and MSB-first order
remain selected. For example, RGB565 `0x0020` no longer introduces those red
and blue low bits. Gamma tables are not changed.

Orientation changes preserve these RAMCTRL and colour settings. See the
[orientation contract and physical matrix](display-orientation.md) for rotation
commands and per-mounting colour checks.

Three source checks support this targeted correction:

- [Sitronix ST7789V v1.2](https://files.seeedstudio.com/wiki/Grove-1.2inch-ips-display/ST7789V_SPEC_V1.2.pdf),
  RAMCTRL fields and expansion diagrams on pages 255 through 257.
- [Pimoroni issue 1040](https://github.com/pimoroni/pimoroni-pico/issues/1040),
  including the maintainer's low-green reproduction with default gamma.
- [Pimoroni's exact driver revision](https://github.com/pimoroni/pimoroni-pico/blob/b41010d1e9809a62c878e067d7e17ae56f3c4684/drivers/st7789/st7789.cpp),
  which applies the same two bytes. The locked Embassy SPI path was also
  checked to complete writes before changing D/C or releasing CS.

The optimized firmware builds. Hardware acceptance still requires primary
colors, low-intensity green and grayscale ramps on the actual PIM715 assembly,
at each accepted SPI rate. No physical color defect or correction was measured
on the user's display.
