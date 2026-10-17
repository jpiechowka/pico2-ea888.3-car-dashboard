# Leon Cupra DNUC: hardware protection audit and proposed build plan

Reviewed 2026-10-17 against manufacturer documentation, all four published
Longan schematics, the Pico/PIM715 drawings and the resolved firmware source.
This is a **design proposal, not an assembly-ready schematic or purchasing BOM**.
Owner photographs identify the Longan revision and the Pololu/switch models;
no electrical or vehicle measurements have been performed. The owner requires
reuse of these parts and selected isolated dashboard power plus UART.

[Architecture diagrams](../README.md#hardware-protection-and-wiring-for-the-dnuc-cupra)
| [Pin and connection reference](wiring-diagram.txt)
| [W board/code audit](../docs/hardware/pico2w-board-audit.md)
| [Wired versus Bluetooth](transport-options.md)
| [Selected module-based build](module-build.md)
| [Capacitor parts and Allegro offers](capacitors.md)
| [Vehicle/bench runbook](../docs/validation/pico2-dnuc-runbook.md)

## 1. Retained parts and evidence status

| Part named in the repository | Disposition | Before final wiring |
| --- | --- | --- |
| Raspberry Pi **Pico 2 W**, RP2350 | Retain; board photograph identified | Inspect headers, mounting and antenna clearance; radio is not initialized by current firmware |
| Pimoroni PIM715 Display Pack 2.8 | Retain | Check intact RGB traces and unpopulated SP/CE connector |
| Adafruit 5880 encoder | Retain | Confirm assembly, cable length and address; use the display's 3.3 V Qw/ST supply |
| Adafruit 3218 covered illuminated switch | Retain; packaging identified | Identify terminals; relocate to protected low-voltage switching/control |
| Longan Serial CAN Bus **V1.4, Aug 2020**, kit bag Seeed **114991438** | Retain; MCP2515 and **SIT65HVD230** photographed | Identify LDO/MCU and revision-matched circuit; measure termination/logic levels |
| Pololu **3782 D36V28F5** | Retain; label and both board faces photographed | Measure load, rail tolerance and thermal margin |

The owner's photographs identify these boards and the Longan transceiver;
raw photos remain outside the repository. Encoder identity follows the existing
project. Photos do not establish rail voltage, continuity, termination, flash
capacity or the unreadable Longan MCU/regulator markings.

## 2. Corrections that affect the design

### Longan is a revision-dependent UART bridge

The old baseline incorrectly showed a self-powered 12 V Longan input. Commit
`18d2418` already removed that wiring. **OBD pin 16 must never feed module VCC**.
The kit contains a serial CAN module and connector; it is neither an ELM327 nor
a bare MCP2515 wired to Pico SPI. See the official [kit](https://docs.longan-labs.cc/1030002/)
and [module revision table](https://docs.longan-labs.cc/1030001/#version-track).

The new audit found a second trap: the module introduction still names MCP2551,
but the **V1.3 schematic component value is SN65HVD230**. The old Eagle symbol
name remains MCP2551. Reading only the symbol or product introduction gives the
wrong transceiver. All four schematics were checked at revision
[`29fcdaf1`](https://github.com/Longan-Labs/Eagle_File_Serial_CAN_Bus/tree/29fcdaf1df0e249a6bfcde49f8d229e4743c611b).

**The owner's V1.4 has SIT65HVD230**, not the similarly named TI part. The
following published history stops at V1.3; its internal-rail assumptions are not
qualified for V1.4. The rear `5V` connector label is not proof of UART output
voltage or 12 V input tolerance. A matching V1.4 schematic remains unavailable.

| Module revision | Published supply/logic guidance | Schematic evidence |
| --- | --- | --- |
| V1.0 | 5 V supply, 5 V UART | MCP2551; external UART interface required for Pico |
| V1.1 / V1.2 | 5 V supply; optional reduced TX level | MCP2551; P2 closes a 1 kΩ/2 kΩ TX divider, not a bidirectional power-off-safe translator |
| V1.3 | 5 V or 3.3 V supply; lower-voltage logic supported | SN65HVD230; XC6206P302MR regulator; UART connects directly to the module MCU |

The V1.3 drawing's regulator denotes 3.0 V ±2%, while the transceiver requires
at least 3.0 V. That leaves no guaranteed lower-rail margin. Resolve the actual
regulator marking and rail under load/temperature before accepting that design;
this is a **published-drawing concern, not a diagnosed defect in the owner's kit**.
Sources: [V1.3 schematic](https://github.com/Longan-Labs/Eagle_File_Serial_CAN_Bus/blob/29fcdaf1df0e249a6bfcde49f8d229e4743c611b/V1.3/Serial_CAN_Bus_V1.3.sch),
[Torex XC6206, pp. 2–4](https://product.torexsemi.com/system/files/series/xc6206.pdf),
[TI SN65HVD230, pp. 5–6](https://www.ti.com/lit/ds/symlink/sn65hvd230.pdf).

The depicted 16 MHz MCP2515 oscillator is within the current manufacturer's
2.7–5.5 V oscillator limit; it is not an additional finding. SPI timing is a
separate limit. [MCP2515 DS20001801J, table 13-2](https://ww1.microchip.com/downloads/en/DeviceDoc/MCP2515-Stand-Alone-CAN-Controller-with-SPI-20001801J.pdf).

### CAN protection cannot be selected from the kit name

The actual SIT65HVD230 requires 3.0–3.6 V. SIT lists bus-fault voltage ±16 V,
common-mode range −2 to +7 V and **no dominant timeout**. Its datasheet gives
±18 V DC bus absolute limits and ±25 V transient stress without an installation
test envelope. Do not treat those stress ratings as normal operating targets,
or apply TI's different limits. [SIT product data](https://en.sitcores.com/show-38-154-1.html),
[manufacturer datasheet, REC V1.9, pp. 1–5](https://en.sitcores.com/uploadfile/2023/1012/20231012041534872.pdf).
The same revision was independently read from a
[manufacturer-authored copy hosted by Waveshare](https://files.waveshare.com/wiki/common/SIT65HVD230DR_DataSheet_EN.pdf)
when the manufacturer's download host was unavailable; byte identity was not established.

A generic “24 V CAN TVS” is therefore not an approved choice. For example,
PESD2CAN has 24 V stand-off and a specified 41 V clamp at 5 A, which does not
establish protection for the fitted SIT65HVD230. Match both polarities, pulse energy,
capacitance, leakage and layout to the actual transceiver. A TVS cannot safely
absorb an indefinitely sustained battery miswire. [Nexperia PESD2CAN, table 8](https://assets.nexperia.com/documents/data-sheet/PESD2CAN.pdf).

The kit page says termination is connected by default; the module page says
to solder a pad to add it. Both drawings and assembly must be checked. Measure
the **disconnected, unpowered module** and inspect P1 before changing anything.
Do not add another 120 Ω terminator to a diagnostic tap on an already terminated
bus. A gateway-facing vehicle socket is not guaranteed to give a universally
interpretable 60 Ω reading. Never measure resistance on an energized circuit.

V1.2 includes a common-mode choke. V1.3's drawn CAN lines go directly to the
terminal and transceiver apart from the P1/120 Ω branch. A choke is neither
galvanic isolation nor a substitute for coordinated surge protection.

### Switch, regulator and USB

Adafruit specifies **12 V maximum and recommends 1–2 A**, despite a 20 A case
marking. The switch joins `+` to the headlamp terminal; its LED is internally
connected from that terminal through a resistor to the side ground terminal.
Retain it on protected low voltage or as an enable control, with contact/inrush
checks. Do not retain the old raw vehicle-supply placement.
[Adafruit 3218](https://www.adafruit.com/product/3218).

Pololu specifies 5.3–50 V input, 5 V ±4% output and reverse protection to 40 V.
Its current plots are typical and thermally dependent. EN has a 100 kΩ pull-up
to VIN; it must not connect directly to Pico GPIO. PG's approximately +20%/−10%
window is too wide to serve as precision Pico overvoltage protection. Typical
no-load consumption is 2–3 mA; disabling EN reduces it but does not make it zero.
[Pololu 3782](https://www.pololu.com/product/3782).

Independent output protection must account for a failed buck passing its input
to the nominal 5 V rail. A protector must survive that fault voltage. In the
selected isolated topology, Pololu's tolerance applies to Longan and the isolated
converter input. Pico's 5.5 V VSYS ceiling instead constrains the converter's
secondary output, including tolerance, ripple, load steps and the source diode.
A “5 V TVS” alone is not a precision 5.5 V clamp. Isolation does not by itself
guarantee a regulated output under an internal fault.

USB feeds Pico VSYS through its onboard diode. The isolated converter's output
needs its own reverse-blocking/ORing path to prevent USB backfeeding its secondary.
That alone does not imply current crosses an intact isolation barrier. The old
direct shared-ground supply could instead backfeed regulator/Longan. USB can
still intentionally power Pico when the vehicle branch is off. Signal and ground
paths need independent treatment.
[Pico 2 W datasheet, §§3.4–3.5](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf).

## 3. Architecture choice

**Owner-selected architecture:** isolate the dashboard's power and both UART
directions. Retain Pololu and Longan on the vehicle side. This reduces
vehicle-to-USB ground fault paths. It does not make the Longan immune to faults
or prevent all unwanted CAN traffic.

After the [Bluetooth comparison](transport-options.md), the owner confirmed
isolated Longan and chose **mostly ready-made modules with some soldering**.
The [practical shortlist](module-build.md) supersedes any suggestion that a
custom carrier is required. BLE remains an unselected alternative.

| Option | Protection and behavior | Tradeoff |
| --- | --- | --- |
| **Isolated dashboard, selected** | Fused input, Pololu, regulated isolated DC/DC and two UART isolation channels; separate grounds; independent rail-fault protection decision remains open | More parts, board area, heat and EMI work; every cable/mount must preserve the barrier |
| Protected shared ground | Same input/output protections; external power ORing and power-off-safe UART interface | Smaller and more efficient; USB/computer and vehicle remain electrically linked |
| Surge ride-through addition | Either design with a dimensioned surge stopper/energy reserve | More complex thermal/SOA and capacitor work; only useful if uninterrupted operation is required |

Clean restart during supply dips is approved. Intended use is switch off, start
engine, then switch on. Keep capacitors for ripple/load-step smoothing; no
battery, supercapacitor or dedicated crank ride-through circuit is selected.
Input protection and Pololu remain connected while the branch switch is off.

The approved covered switch cuts the regulated vehicle-side 5 V branch before
Longan and the isolated converter. Its LED return stays in that same domain.
Pololu remains enabled; unplug OBD for long parking or physical CAN isolation.
USB can independently keep the dashboard powered. See the
[cutoff state table](module-build.md#what-the-approved-cutoff-does).

Vehicle-side TVS returns, CAN reference and power return need deliberate routing.
Keep OBD pin 4 (chassis ground) and pin 5 (signal ground) distinct until the return
topology is designed. Do not route load or surge current through a signal-reference
wire. With isolation, neither becomes a direct Pico/USB ground connection.

For stronger protection of the vehicle from an adapter fault, evaluate a normally
open two-pole CAN disconnect with independent supervision. This remains an
**unselected option**: contact/switch loading, fault detection, latency and stuck
contacts need analysis. Cutting Longan power does not clear a physical bus short;
parts on the bus side of a disconnect can themselves fail. A UART isolator's
idle-high output does not enforce CAN recessive state.

## 4. Capacitors: local bypass, local storage and bulk

The remembered “fast, medium and slow” capacitors correspond to different
frequency/energy roles. Select them as a network, including what is already on
the boards:

| Role | Initial analysis range, not a buying value | Checks |
| --- | --- | --- |
| Local bypass | Around 100 nF ceramic beside each new IC supply | Short loop, low ESL, actual voltage/temperature rating |
| Local storage | Roughly 1–10 µF effective ceramic at a power branch | DC-bias loss, tolerance, aging, regulator stability and load steps |
| Bulk/damping | Tens to hundreds of µF where measured load/lead inductance warrants | ESR at cold/hot, ripple current, lifetime, inrush and mechanical support |

**Lowest ESR everywhere can worsen ringing.** Cable inductance and low-ESR
ceramics can create destructive plug-in spikes; controlled ESR or a separate
damping network may be needed. Extra capacitors do not replace reverse-polarity,
overvoltage or fuse protection. [Pololu LC-spike application note](https://www.pololu.com/docs/0J16/all),
[Murata DC-bias guidance](https://www.murata.com/support/faqs/capacitor/ceramiccapacitor/char/0005).

Rate capacitors for their actual node: a part before the input disconnect sees a
different fault envelope from one on regulated 5 V. Include `I = C × dV/dt`,
stored energy `½CV²`, mixed-capacitor resonance and switch bounce. Keep power
storage capacitors off CAN-H/CAN-L and I2C signal lines.

The [exact-part shortlist](capacitors.md) now includes Panasonic FR 100 µF / 63 V
and a lower-impedance 220 µF / 50 V option, with manufacturer ratings, Allegro
offers and conditional locations. These do not yet define a coordinated input
filter or replace the listed electrical checks.

## 5. BOM planning register

Retained modules are listed above. These are **design blocks to select**, not
approved substitutions or purchase quantities:

| Block | Candidate direction | Selection gate |
| --- | --- | --- |
| OBD harness and fuse | Keyed connector, short twisted CAN pair, strain relief, inline fuse close to pin 16 | Actual pin continuity, wire/contact gauge, fuse time-current and breaking capacity |
| Automotive front end | Coordinated fuse/TVS/damping or a documented assembled protection board; custom controller circuits are research alternatives | Defined surge/reverse/jump-start envelope, clamp energy and any switch/FET limits |
| 5 V fault containment | High-input-voltage-tolerant disconnect/current protection | Buck input-to-output short; trip minimum above normal maximum and fault peak below every load limit |
| Isolated supply, selected architecture | Regulated 5-to-5 V converter sized after load measurement | No-load/full-load voltage, thermal derating, startup capacitive load, isolation working voltage and EMI |
| UART interface | Assembled opposite-direction isolator; MIKROE-6518 is a conditional candidate | Actual Longan logic supply, threshold margins, brownout and every supply order |
| CAN protection | Revision-matched low-capacitance protection; optional supervised disconnect | Both-polarity clamp/fault energy and complete bus-loading qualification |
| Switch interface | Approved covered switch on regulated vehicle-side 5 V before both loads | Contact/inrush limit, same-domain LED, idle draw and USB exception |
| Capacitor network | Effective ceramics plus bulk/damping selected per node | ESR/ESL, bias, ripple, temperature/lifetime, inrush and regulator stability |
| Enclosure/interconnect | Insulated mounting, rated crimps/connectors, test points and service disconnect | Abrasion, vibration, accidental shorts, barrier clearance and heat |

Starting references: [TI LM7480-Q1](https://www.ti.com/product/LM7480-Q1),
[ADI LTC4368](https://www.analog.com/en/products/ltc4368.html),
[TI ISO7721 supply/default-state tables](https://www.ti.com/lit/ds/symlink/iso7721.pdf),
[Littelfuse fuse/wire coordination](https://www.littelfuse.com/~/media/automotive/catalogs/littelfuse_fuseology.pdf).
Component capability is not certification of this assembly. The old 3 A fuse,
SS34, SMAJ14A and 500 mA PTC are not an approved coordinated set.

## 6. Pin, firmware and control checks

The [PIM715 schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048)
and [Pico 2 W header drawing](https://datasheets.raspberrypi.com/picow/pico-2-w-pinout.pdf)
were visually checked against `firmware/pico2/src/main.rs` and the drivers.
GP0/1 are UART TX/RX, not physical pins 0/1. PIM715 RGB uses GP26/27/28;
GP21 is LCD TE. GP7–11 also route to SP/CE and are spare only when that expansion
is unused. LCD reset is connected to Pico RUN. Keep connector-facing and
solder-side views distinct; use moulded pin numbers and continuity, not colours.

Pico 2 W GP23/24/25/29 serve its wireless interface/shared VSYS sensing. They are
not the non-W SMPS/VBUS/LED assignments. Current application pins remain valid;
see the [HAL and board audit](../docs/hardware/pico2w-board-audit.md).

Qw/ST supplies the encoder at 3.3 V, with SDA GP4 and SCL GP5. Adafruit documents
10 kΩ pull-ups; the locked Embassy RP 0.10.0 configuration also enables weak
internal pull-ups. Firmware runs 400 kHz. Check combined pull-ups, capacitance,
rise time and ESD protection for the actual cable rather than assuming any cable
length works. [Encoder pinout](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder/pinouts),
[NXP I2C specification, Fast-mode timing](https://www.nxp.com/docs/en/user-guide/UM10204.pdf).

The optional `longan-rx12` path uses UART0 at externally provisioned 115200 8N1.
It remains unqualified and does not enable diagnostic polling. This is **not a
hardware listen-only guarantee**: an ordinary CAN controller can acknowledge or
signal errors independently. The bridge source also omits received DLC/flags.
See [transport qualification](../docs/vehicle/diagnostic-transport.md) and
[pinned bridge source](https://github.com/Longan-Labs/Serial_CAN_Bus/blob/c11ae43a81e9f9fc87fae56f70c6a14ccff53ce9/Serial_CAN_Firmware/cmdMode.cpp).

No firmware control, button mapping, LED or encoder behavior changes in this audit.
Studio/helper board confirmation is deliberately migrated to explicit Pico 2 W consent.
Dashboard brightness remains Full/50% duty/Off; the backlight is near 1 kHz from
the resolved clock. [Control/electrical contract](../docs/hardware/backlight-controls.md).

## 7. Proposed implementation order and acceptance

1. **Finish identifying the owned hardware.** Pico 2 W, V1.4 Longan/SIT65HVD230,
   Pololu and switch are recorded. Resolve Longan LDO/MCU, jumper circuit and
   unpowered cable continuity. Resolve actual V1.4 supply
   limits without extrapolating from V1.3.
2. **Apply the approved operating choices.** Isolated Longan, ready-made modules,
   5 V branch cutoff, restart on dips and compact layout are selected. Use
   approximately ≤1 m OBD and ≤20 cm encoder leads; size the enclosure from the
   selected modules and thermal/clearance checks. Retain smoothing caps.
3. **Produce the detailed schematic and purchasing BOM.** Assign net names and
   connector views; calculate power/thermal/inrush budgets, fuse/wire coordination,
   TVS/FET stress, rail OVP tolerance and capacitor damping. Independently review
   failure paths and update the README diagrams from the same design.
4. **Bench-test power with dummy loads first.** Use a current-limited supply;
   scope hot-plug, bounce, ramps, dips and branch faults. Verify all USB/vehicle
   supply orders, no backfeed, rail maxima and shutdown/restart. Simulated faults
   belong on an isolated bench fixture, never the car's port.
5. **Qualify controls and CAN on a bench network.** Check encoder cable/rise time,
   all buttons/LEDs, termination, adapter power-off loading, startup, baud,
   ACK/errors and watchdog behavior. Record module/bridge identity. Automotive
   transient/ESD survival needs appropriate equipment and a defined test envelope.
6. **Connect to the parked vehicle only after electrical acceptance.** Follow
   the runbook and establish exact ECU support separately. Preserve an accessible
   physical disconnect. Software tests do not establish electrical safety.

Final evidence must distinguish schematic review, bench measurements and vehicle
results. The Pico itself has temperature limits; automotive-rated protection
parts do not turn the whole stack into an automotive-qualified module.

## Open decisions

- Longan V1.4 matching circuit, LDO/MCU and measured rails/termination: open;
  revision, MCP2515/SIT transceiver, Pico 2 W and Pololu/switch identified.
- Isolated power plus UART: approved; detailed component design remains pending.
- Wired versus BLE: isolated Longan selected after comparison; BLE not selected.
- Assembly: mostly ready-made modules and some soldering; no custom PCB default.
- Covered switch: regulated 5 V branch cutoff selected; unplug OBD for long
  parking. Restart on dips accepted, smoothing caps retained, no crank backup.
- Compact layout approved: modules near display, approximately ≤1 m OBD and
  ≤20 cm encoder leads. Exact enclosure fit, cooling and signal integrity remain
  physical checks.
- Exact protection parts/values and purchase-ready BOM: depend on those decisions
  and the electrical worksheets above.
