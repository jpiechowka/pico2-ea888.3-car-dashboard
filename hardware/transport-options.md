# Transport options: isolated Longan and BLE OBD

Research reviewed 2026-10-17. The owned board is **Raspberry Pi Pico 2 W**.
The comparison below records the earlier isolated Longan choice. On 2026-10-18
the owner selected the **USB-powered BLE prototype with the retained encoder**;
see [current hardware](README.md) and [runtime limits](../docs/vehicle/ble-runtime.md).
Longan/Pololu wiring remains historical. Neither route establishes exact DNUC
ECU support, and no adapter purchase or vehicle test is implied.

## Earlier recommendation and tradeoffs

The selected isolated Longan design reuses the owned adapter and avoids adding
radio pairing and reconnect behavior to the firmware. Consider a separate CX bench
prototype if removing the UART cable is worth the additional integration work.
This comparison does not authorize purchases or vehicle communication.

| Concern | Isolated wired Longan baseline | OBDLink CX BLE candidate |
|---|---|---|
| Hardware | Owned kit plus protection, UART isolator and isolated DC/DC | Additional dongle; owned Pico 2 W supplies the radio capability |
| Firmware | Existing optional UART profile is a starting point; bridge and ECU qualification remain | New BLE central, encrypted pairing, bond storage, GATT stream and ELM/ST parser |
| Data connection | Both UART directions cross the isolation barrier | Radio removes the conductive data cable |
| Power isolation | Isolated DC/DC, with no accidental ground bridge | Still needs isolated dashboard power if galvanic isolation is required |
| Covered switch | Can cut the adapter and dashboard vehicle supply together | A dashboard switch leaves a directly plugged CX powered |
| Timing | UART, bridge and ECU timing require measurement | Also requires RF loss, startup and reconnect measurement |

See the [power protection plan](protection-plan.md) and
[proposed wiring](wiring-diagram.txt) for the baseline power domains.

## Documented CX interface

The manufacturer's [CX developer notes](https://support.obdlink.com/support/solutions/articles/43000746707-obdlink-cx-adapter-notes)
publish BLE 5.1, mandatory encrypted communication and a five-minute initial
bonding window after power-on. The custom UART service uses these UUIDs:

| Purpose | UUID |
|---|---|
| Service | `0000FFF0-0000-1000-8000-00805F9B34FB` |
| Notifications | `0000FFF1-0000-1000-8000-00805F9B34FB` |
| Writes | `0000FFF2-0000-1000-8000-00805F9B34FB` |

The documented maximum ATT MTU is 247; the negotiated value can be smaller.
QueuedWrites are unsupported. A transport needs bounded chunking, notification
reassembly and write-response handling. The notes' platform examples are
illustrative; their overhead discussion should not become an unchecked Rust
constant.

The [family programming manual, revision F](https://www.scantool.net/scantool/downloads/682/obdlink_frpm_f.pdf)
identifies CX as STN2310 (printed page 6). It documents CAN header selection
(`ATSH`, page 18), protocol selection (`STP`, section 8.6), flow-control address
pairs and segmentation (`STCFCPA`, `STCSEGR`, `STCSEGT`, page 38). These are
transport primitives for separately qualified OBD/UDS requests. They do not provide
DNUC DID definitions, scaling or session requirements. The `STPX` size table on
page 36 omits CX, leaving its long-message limits unconfirmed. `ATSP` can report
OK for an unsupported CX protocol; `STP` rejects unsupported choices. Page 48
classifies MX+/MX/LX as Classic Bluetooth and CX as BLE, so they are not
interchangeable choices for this BLE design.

## Power, shutdown and evidence limits

[CX product specifications](https://www.obdlink.com/products/obdlink-cx/) list
8–18 V operation, 55 mA operating current, -20 to 55°C operation and protection
up to 100 V. Sleep current conflicts: the product page says below 1 mA; the
developer notes say below 2 mA. Use the less favorable figure for provisional
budgeting and measure the actual adapter. The public protection claim does not
supply a complete pulse waveform, duration and source impedance qualification.

The manual's sleep sections (printed pages 67, 71–72) describe a default
600-second inactivity timer and wake on Bluetooth connection or voltage change.
Ongoing monitor commands can prevent inactivity sleep. Sleep therefore does not
establish immediate electrical cutoff.

The covered switch removes only the supply path it controls. USB can keep the
Pico powered, and capacitors retain energy after a supply is opened. A directly
plugged CX remains connected to OBD power when the dashboard is switched off;
a command already executing there may continue. Cutting that adapter's power
requires a separately designed switched feed. A last-second BLE sleep command
cannot guarantee shutdown when dashboard power is removed mechanically.

BLE isolates the data path. Shared vehicle ground or ordinary non-isolated
power still creates a conductive connection. USB, SWD, switch-lamp wiring and
mounts must also preserve any intended isolation barrier.

## Rust feasibility and qualification work

[Upstream Trouble](https://github.com/embassy-rs/trouble) lists Pico 2 W examples
and BLE central/GATT-client support. Its
[Pico 2 W example manifest](https://raw.githubusercontent.com/embassy-rs/trouble/main/examples/rp-pico-2-w/Cargo.toml)
selects RP235x, CYW43 Bluetooth and `cyw43-pio`. On the review date, its Embassy
patches referenced `3cd51e6d8eb6aff8b0d64d9e56a75a538bcfc65a`.
This is upstream feasibility evidence, not a tested dependency set for this
repository. Individual example implementations were not fully retrieved.

[Official Trouble documentation](https://embassy.dev/trouble/) describes pairing,
cryptographic drivers, secure randomness, application-owned bond persistence and
bounded packet resources. The current project has no CYW43/Trouble integration.
A future prototype must:

1. Verify encrypted pairing and persistent reconnect with the exact CX firmware,
   including power interruption, bond repair and notification resubscription.
2. Allocate radio PIO/DMA/IRQ resources alongside the display and preserve both
   153600-byte framebuffers, stack reserves and existing flash coordination.
3. Validate prompt parsing, ISO-TP boundaries, failures and stale-response rejection
   on a bench; qualify ECU requests separately using exact vehicle evidence.
4. Measure linked memory, stack margin, acquisition age and display/control
   responsiveness under reconnect, RF interference and long-response load.
5. Validate protected power, USB behavior, shutdown timing and temperature limits
   on the assembled hardware.

No BLE build, adapter interoperability test, electrical validation or vehicle
qualification is established by this comparison.
