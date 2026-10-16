# Configuration v2 contract

Historical wire contract, retained for explicit migration. New exports and
firmware use [current v5](configuration-format.md#current-v5-contract).

Version 2 added gauges and RGB565 palettes. The catalogue remains version 1;
source, decoder, fuel, warning and placement contracts in [v1](schema-v1.md)
otherwise remain unchanged. The 16 KiB binary and 128 KiB JSON limits remain.
The frozen v2 fixtures remain under `crates/dashboard-config/tests/fixtures`;
the checked-in `configs/default.json` follows the current contract.

## Wire changes

The existing 12-byte EAV envelope now declares version 2. CRC, canonical
Postcard encoding, exact payload length and whole-document validation are
mandatory. Field and enum order below are part of the versioned binary format.

| Type | Fields appended after its v1 fields |
| --- | --- |
| Config | `warning_colors: WarningColors` |
| Widget | `gauge: Gauge`, `colors: WidgetColors` |

| New type | Fields in order |
| --- | --- |
| Gauge | `kind: GaugeKind`, `min: f32`, `max: f32`, `expand: bool` |
| WarningColors | `foreground: Color`, `background: Color` |
| WidgetColors | `background`, `border`, `label`, `value`, `secondary`, `graph`, `gauge`, `track`, `ticks`, `average`, `minimum`, `maximum`, `trend`, `peak` (all Color), then `warning: WarningColors` |

`GaugeKind` order is Numeric, Circular, Dial, Horizontal, Vertical, Segmented.
Gauge limits use canonical channel units, are finite and within the channel's
configuration domain, and differ by at least 0.001 in the stored f32 values.
They are visual scales, independent of warning thresholds and history ranges.
`expand` starts from these limits and only expands from accepted run extrema.
The existing statistics reset also resets that expansion; it is not persisted.

JSON Color is exactly `#RRGGBB`, accepting either hex case. Input is quantized
once to RGB565 and exported as the represented RGB888 colour in uppercase hex.
Binary Color is a Postcard u16 containing R5:G6:B5, most significant red bits.
It occupies two bytes in RAM and up to three bytes on the wire. WidgetColors
holds sixteen colours (32 bytes in RAM), including its warning pair.

All stored text and indicator roles, including currently hidden roles, must
contrast with the widget background by at least 3:1 after RGB565 quantization.
The gauge indicator must also contrast with its track; each warning pair is
checked separately. Border and track are decorative and are not independently
required to contrast with the background. The bounded integer implementation
conservatively checks sRGB relative luminance. This is a visibility guard, not
a claim of accessibility conformance or physical sunlight readability.

CycleOne uses the displayed warning widget's pair, including warnings from
other views. ListThree uses Config.warning_colors even when only one warning
is active. Blink exchanges foreground and background. Warning selection, colour
and phase all participate in the retained overlay identity.

## Migration and rejection

The host/browser JSON feature explicitly decodes the frozen v1 model and
upgrades it to Numeric gauges with channel defaults and the default palette.
It accepts validated v1 JSON, v1 EAV and the existing saved-browser envelope.
This does not use Serde defaults to reinterpret an old binary layout.

At the v2 stage, the firmware parser accepted only v2. An unsupported or damaged embedded file
takes the existing fault/default path. The stage
compiler accepted v1 input, validated and upgraded it to canonical v2 bytes.
The current compiler explicitly migrates v1/v2/v3/v4 to v5; current firmware
accepts only v5. Captures
retain the original input and its hash plus the normalized/embedded hash;
they independently compile the frozen input and require byte equality.

Missing or extra fields, duplicate fields, unknown enum variants, bad CRC,
noncanonical encodings, trailing payload bytes, oversized collections and
invalid semantic values are rejected. Old firmware cannot read v2; export
and flash a matching firmware/configuration pair.
