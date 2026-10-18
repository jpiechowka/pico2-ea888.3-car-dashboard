# BLE prototype in Studio

## Configure the peer

Open **Device → Bluetooth connection**. BLE is disabled by default. Enable it,
choose the adapter profile, then enter the peer's six-pair address and address
kind. The example address in the hint is illustrative; use the actual peer's
address. Random static addresses must meet the shared validation rules. Rotating
private addresses and automatic discovery are unsupported.

An enabled peer cannot be all zero. A rejected edit stays visible in the invalid
preview and error navigator; save, export and build stay blocked. Correct every
invalid field or discard the rejected edits. A repaired group is one accepted
edit, and Undo/Redo restores the complete accepted configuration. Advanced
connection timing uses the same bounds as firmware and project admission.

- **vLinker MC+** and **OBDLink CX** currently initialize and identify the adapter.
  They do not enable unqualified ECU requests or prove physical compatibility.
- **OBDLink CX** requires encryption. Prototype bonds are volatile, so reboot
  requires pairing again. Projects contain no PIN or bond key.
- **Synthetic test peer** uses a separate service and synthetic diagnostic
  fixtures. Its success does not qualify the real vehicle route.

USB powers the Pico 2 W dashboard and retained encoder. Turning USB off does
not turn off an OBD adapter. Studio cannot display a physical BLE connection;
its status statement makes that limitation explicit.

## Exercise the protocol

Choose **Open synthetic BLE diagnostics**, or open **Diagnostics → Timing
simulation**. These controls run the shared bounded protocol with simulated
milliseconds, without opening browser Bluetooth:

1. **Connect synthetic peer** runs initialization and identity fixtures.
2. **Begin fixture reply** starts one pending response.
3. Deliver the fragmented reply, inject a notification loss, expire the deadline,
   or inject an old-epoch reply. An old-epoch reply must leave the current request
   pending. A loss or deadline fault retires it.
4. Reconnect after a fault to create a new epoch and repeat initialization.
   **Disconnect fixture** explicitly ends the synthetic connection.

The state, completed response count, lost notifications and response duration
are all synthetic. No fixture publishes vehicle measurements. Accepted project
changes reset the fixture; invalid edits and unapplied JSON disable its actions.
Encryption, real radio timing and automatic reconnect need device testing.

## Repeatable browser regressions

From `configurator/`, using the pinned Trunk and Rust toolchain:

```powershell
trunk build --release --locked --features browser-tests --dist target/browser-tests
python scripts/browser_failure_tests.py --browser "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --window-size 1440,1000
python scripts/browser_failure_tests.py --browser "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --window-size 760,1000
```

Each invocation owns a temporary browser profile and ephemeral loopback server.
It preserves existing Studio processes and saved projects. The suite drives
Rust DOM event handlers for invalid peer/timing repair, Undo/Redo, address kind,
CX help, BLE fixture faults, JSON blocking, accepted-change reset and build copy.
It also checks the rendered BLE controls fit the selected viewport. The normal
browser failure suite still covers storage, workers, transport and rendering.
Interactive screenshot inspection is a separate visual check.
