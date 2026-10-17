# Longan V1.4: owned-board identification and UART checks

Reviewed **2026-10-18**. This record reconciles the owner's photographs,
manufacturer specifications, Longan's circuit files and the dashboard's UART
source. It updates the [protection plan](protection-plan.md) and
[conditional module BOM](module-build.md). No electrical measurements have been
made; the photographs remain private. The isolated-Longan architecture is unchanged.

## What the new photographs establish

| Item | Visible evidence | Conclusion and limit |
| --- | --- | --- |
| Board | `Serial CAN Bus V1.4, Aug 2020` on rear | Owned revision identified; no matching public V1.4 circuit obtained |
| Bridge MCU | `ATMEL / MEGA168PA` | **ATmega168PA**; firmware, clock fuses, silicon revision and temperature grade are not identified |
| CAN controller | `MCP2515` | Standalone CAN controller behind the UART bridge |
| CAN transceiver | `SIT65HVD230` | Use SIT specifications; neither MCP2551 nor TI SN65HVD230 describes this fitted part |
| Regulator | `662K`, three-terminal package | Consistent with a **nominal 3.3 V XC6206-family** part under Torex's marking scheme; exact manufacturer/accuracy suffix unconfirmed |
| Resistor near CAN connector | `121` | Nominal **120 Ω** value; not a reference designator and not proof that termination is connected |
| Crystal | `16.000` | Nominal 16 MHz; does not identify the MCU's clock source or operating frequency |

The MCU appears to use the small leadless footprint inside unused larger pads.
Confirm its package and pin-1 orientation before probing; do not transfer the
older drawing's package pin numbers directly to the photographed assembly.

## Regulator: why the old 3.0 V assumption no longer fits

[Torex XC6206, ETR03005-009, p. 14](https://product.torexsemi.com/system/files/series/xc6206.pdf)
decodes `662K` as family `6`, voltage group `6`, output code `2` = **3.3 V**,
and lot character `K`. The marking does not distinguish the 1% and 2% accuracy
variants. `XC6206P33xMR` is a conditional family description, not a verified
ordering code or proof of authenticity.

The same document's SOT-23 **top view**, p. 3, assigns pin 1 = VSS, pin 2 = VOUT
and pin 3 = VIN. Check continuity before applying this candidate pinout. Its
maximum operating input is **6.0 V**; **7.0 V** is an absolute stress limit.
Neither permits a raw vehicle-battery connection. The current/thermal ratings
also do not establish spare capacity on this particular board.

The older V1.3 drawing instead names **XC6206P302MR**, nominal 3.0 V. That
drawing's lower supply-margin concern is historical evidence, **not a diagnosed
defect in the owner's V1.4**. The new marking supports expecting roughly 3.3 V;
actual voltage, ripple, startup behavior and loading remain measurements.

## Circuit comparison: read nets, not product headlines

Longan's [module revision table](https://docs.longan-labs.cc/1030001/#version-track)
stops at V1.3. Its generic MCP2551 introduction conflicts with both the V1.3
component value and the owned-board photograph. The
[V1.3 Eagle XML at `29fcdaf1`](https://github.com/Longan-Labs/Eagle_File_Serial_CAN_Bus/blob/29fcdaf1df0e249a6bfcde49f8d229e4743c611b/V1.3/Serial_CAN_Bus_V1.3.sch)
was inspected at the component and net level:

| Older drawing's net/path | What it establishes for V1.3 |
| --- | --- |
| External `5V` → U5 VIN | Connector power and internal logic supply are different nets |
| U5 VOUT → internal `VCC` | Supplies MCU VCC/AVCC, MCP2515 VDD and transceiver VDD |
| Connector RX → MCU PD0(RXD); MCU PD1(TXD) → connector TX | Direct UART, without the older TX divider |
| CANH → P1 → R3 (`120R`) → CANL | A jumper controls the termination branch |
| 16 MHz crystal → MCP2515 OSC1/OSC2 | Controller oscillator; MCU XTAL1/XTAL2 are separately drawn to ground |

These are **comparison nets**, not qualified V1.4 solder points. A `5V` connector
label says nothing by itself about the UART high level. The MCU's voltage/frequency
limits must be checked against its actual clock configuration, not the nearby
CAN-controller crystal. No firmware or fuse settings were read from the board.

The [kit page](https://docs.longan-labs.cc/1030002/) and module page disagree on
default termination. The `121` marking cannot resolve that disagreement. Measure
the fully disconnected, unpowered board and P1 before changing any jumper.

## UART electrical and software contract

[Microchip DS40002061A, pp. 322–323](https://ww1.microchip.com/downloads/en/DeviceDoc/ATmega48A-PA-88A-PA-168A-PA-328-P-DS-DS40002061A.pdf)
specifies ordinary ATmega168PA inputs at VCC = 2.4–5.5 V as:

- Low-input upper threshold: **0.3 × VCC**.
- High-input lower threshold: **0.6 × VCC**. Both thresholds assume the pin
  remains inside the applicable voltage limits.
- At an illustrative 3.3 V rail these are 0.99 V and 1.98 V; the actual rail
  has not been measured. The ordinary-pin absolute upper limit is VCC + 0.5 V,
  so a nominal 3.3 V MCU supply does **not** permit a 5 V UART driver.

The [current family errata, DS80000855C](https://ww1.microchip.com/downloads/aemDocuments/documents/MCU08/ProductDocuments/Errata/ATmega48APA88APA168APA328P-SiliConErrataClarif-DS80000855.pdf)
was checked too. Its listed ATmega168A/PA issues concern TWI/analog comparison;
its clock-startup clarification is explicitly for ATmega328P. It does not
replace the UART input thresholds above. A photo does not identify silicon revision.

The candidate [ISO6521, SLLSFU0B §§5.3/5.8](https://download.mikroe.com/documents/datasheets/ISO6521_datasheet.pdf) has a
different high-input requirement, **0.7 × its local supply**. At 3.3 V this is
2.31 V. Do not equate the AVR's input threshold with its output guarantee:
Microchip specifies output voltage at particular supply/load/temperature points.
Typical light-load plots alone do not prove worst-case noise margin. Qualify both
directions with the actual rails, load, temperature range and waveform, including
startup and power-off. The isolator remains conditional, not electrically admitted.

The [candidate module wiring](module-build.md#uart-module-and-pin-verification)
uses the verified MCU rail for its vehicle side and Pico 3V3 for its dashboard
side. These grounds stay separate. Drawing power from the MCU rail requires a
verified accessible connection and spare regulator capacity. Do not substitute
the connector's 5 V supply or add an independent live supply that can backpower
an unpowered MCU. An idle-high UART isolator does not force CAN recessive state.

The actual [Pico adapter](../firmware/pico2/src/drivers/longan.rs),
[baud configuration](../crates/dashboard-core/src/config/vehicle.rs) and
[vendored Embassy UART defaults](../firmware/vendor/embassy-rp/src/uart/mod.rs)
agree on **UART0, GP0 TX / GP1 RX, 115200 baud, 8N1, no inversion or RTS/CTS**.
The adapter is buffered on core 1, without UART DMA. The
[pinned bridge source](https://github.com/Longan-Labs/Serial_CAN_Bus/blob/c11ae43a81e9f9fc87fae56f70c6a14ccff53ce9/Serial_CAN_Firmware/cmdMode.cpp)
supports configurable rates, but neither its presence online nor the MCU marking
proves the installed firmware/EEPROM settings. The dashboard expects external
provisioning and remains [unqualified for diagnostic polling](../docs/vehicle/diagnostic-transport.md).

## Remaining bench evidence

Keep the board disconnected from the car, Pico, USB adapters and all other
supplies for continuity/resistance work. Check for zero voltage first; meter
readings through semiconductors or charging capacitors can be misleading.

| Check | Record before final wiring |
| --- | --- |
| Regulator and connector continuity | Actual ground, input, output and MCU supply nets; safe accessible test point |
| UART continuity | Connector TX/RX directions to MCU functions, using the confirmed package orientation |
| Termination | CANH–CANL resistance and P1 state on the isolated, unpowered board; do not cut/bridge from a photo alone |
| Controlled bench power | Internal rail, input current, ripple, startup/dips and regulator temperature, first without added isolator load |
| Added isolator | Rail/load margin, high/low noise margins and both supply orders; no phantom powering |
| Bridge qualification | Actual UART rate/configuration, controller clock/CAN timing and power-off bus loading on a separate bench CAN network |

A multimeter can resolve the first continuity/rail questions; fast transients,
UART edges and thermal/current margin require additional equipment and a defined
test envelope. No resistance measurements or injected faults belong on an
energized vehicle socket. The [final assembly/acceptance sequence](protection-plan.md#7-proposed-implementation-order-and-acceptance)
still includes input/rail/CAN protection, USB ordering and current/thermal checks.
