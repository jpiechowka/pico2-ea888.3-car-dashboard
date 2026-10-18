# Selected hardware: USB dashboard and BLE

Direction selected 2026-10-18: **Raspberry Pi Pico 2 W, Pimoroni PIM715 display,
Adafruit 5880 encoder and the BLE prototype**, with dashboard power from USB.
The owned W board is identified by photograph. Exact adapter interoperability,
DNUC ECU support, electrical margins and physical display timing remain unverified.

## What stays connected

| Part | Connection / role |
| --- | --- |
| Pico 2 W and PIM715 | Existing display/header assembly; USB powers the dashboard |
| Adafruit 5880 encoder | Existing 3.3 V Qw/ST I2C connection, GP4 SDA / GP5 SCL; retain rotation and click controls |
| PIM715 buttons and RGB | Existing assignments; two-second Y hold remains recovery to Dashboard at Full brightness |
| BLE OBD adapter | Separate vehicle-powered radio peer; no UART/CAN cable to the Pico |

Keep the encoder cable approximately ≤20 cm, with strain relief and checked
connector orientation. Existing connectors remove the proposed protection-board
assembly. A completely solder-free installation depends on fitted Pico/display
headers and the actual encoder connector/cable. Inspect those before assembly.
Use the [W pin audit](../docs/hardware/pico2w-board-audit.md) for physical pins.

The USB route does not use the Longan, Pololu, covered Adafruit switch, isolated
DC/DC or UART isolator. Do not connect the old external VSYS supply alongside USB
as part of this arrangement. The covered switch is unnecessary for the selected
USB-only dashboard power path; the supply's own behavior determines shutdown.

## Power and qualification limits

USB power off stops the dashboard, **not the OBD dongle**. The dongle remains
connected to vehicle power/CAN until physically unplugged or its own qualified
sleep behavior takes effect. Unplug OBD for long parking or physical isolation.
Automatic sleep has not been measured with this runtime, and repeated adapter
commands may prevent inactivity sleep. A car USB outlet may share vehicle ground;
wireless data alone does not certify galvanic isolation or an automotive supply.

MC+ and CX are source-backed software profiles, not approved purchases or tested
installations. Their current firmware path performs adapter initialization and
identity reads only. No live ECU query or vehicle alarm is enabled. The unique
test-peer service is synthetic and never provides vehicle acquisition samples.
See [BLE runtime and limits](../docs/vehicle/ble-runtime.md).

This software prototype does not authorize purchasing, flashing, connecting to
the car or injecting faults. Future bench work starts with the vehicle disconnected
and a checked image/device, under the [validation runbook](../docs/validation/pico2-dnuc-runbook.md).

## Historical wired research

These records remain available for a separately chosen wired alternative:

- [Isolated Longan module build](module-build.md)
- [Protection audit](protection-plan.md) and [wiring reference](wiring-diagram.txt)
- [Owned Longan V1.4 identification](longan-v1.4-identification.md)
- [Capacitor candidates](capacitors.md)
- [Dated wired/BLE comparison](transport-options.md)

Their conditional BOM, soldering, switch and regulator requirements are not the
selected USB/BLE assembly list. The optional `longan-rx12` backend remains in the
repository for that historical route.
