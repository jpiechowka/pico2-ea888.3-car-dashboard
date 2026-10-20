# WiFi + Bluetooth firmware blobs

Firmware obtained from https://github.com/georgerobotics/cyw43-driver/tree/main/firmware

Licensed under the [Infineon Permissive Binary License](./LICENSE-permissive-binary-license-1.0.txt)

## Changelog

* 2026-10-20 local overlay: Bluetooth patch **0065** from
  [`cyw43-driver` at `bf3fa8ad`](https://github.com/georgerobotics/cyw43-driver/blob/bf3fa8ad5f61a30c1b4e7ee5ce86f7dbd8b8d2e4/firmware/cyw43_btfw_43439.h).
  The 6970 binary bytes are the `cyw43_btfw_43439` C array, without the surrounding
  source text. This includes the upstream
  [LE Data Length Extension fix](https://github.com/georgerobotics/cyw43-driver/commit/7f422fef4ea5bff7285fb78340d3f28f5461cff2).
  The original Embassy copy was patch0031 (6164 bytes). Both use the same compact
  HEX download format. Wi-Fi and Pico W NVRAM remain byte-identical to the pinned
  Embassy source. `nvram_rp2040.bin` is also used by Embassy's Pico 2 W example;
  the filename is not a CPU compatibility selector.
  Exact original/overlay hashes and extraction provenance are in
  `../cyw43-firmware-provenance.json`. The Rust provenance checker and real-blob
  loader tests verify these bytes. Device behavior needs separate validation;
  this update alone does not establish a cause for observed resets or missing TVs.
* 2023-08-21: synced with `a1dc885` - Update 43439 fw + clm to come from `wb43439A0_7_95_49_00_combined.h` + add Bluetooth firmware
* 2023-07-28: synced with `ad3bad0` - Update 43439 fw from 7.95.55 to 7.95.62

## Notes

If you update these files, please update the lengths in the `tests/rp/src/bin/cyw43_perf.rs` test (which relies on these files running from RAM).
