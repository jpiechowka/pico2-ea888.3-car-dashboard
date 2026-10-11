# Frame ownership and DMA start

Two fixed aligned framebuffers are allocated in `ConstStaticCell`. A one-time
take creates unique, non-copyable leases. Rendering borrows the current lease;
submitting moves it into a capacity-one channel. The flusher has read access
until the HAL write completes. Boot rendering also ends each mutable borrow
before starting SPI. There are no global mutable framebuffer references.

The production lifecycle checks `(buffer ID, monotonically increasing frame ID)`
through Free, Rendering, Ready, Queued and InFlight. Completion must match both
IDs. Sequence exhaustion, stale/double events and invalid transitions fail
closed. A failed or overdue transfer is quarantined, with no further submission.

After RAMWR completes, the driver polls the actual HAL payload future before
sending Started. The renderer waits for that event before its next synchronous
render. A generic executor yield is not the start condition. The compile-time
`serial-display` feature preserves the same ownership checks but waits for
completion before rendering another frame.

Channel-receive deadlines are 250 ms. A timeout does not cancel the SPI/DMA
future or return its lease. The application stops on a protocol/transfer fault;
independent watchdog recovery is described in `progress-recovery.md`. The HAL's synchronous
BSY and abort loops cannot be preempted by an async timeout.

## Source cross-check, 11 October 2026

- PIM715 is a 320 x 240 ST7789V panel according to its
  [manufacturer](https://shop.pimoroni.com/products/pico-display-pack-2-8).
  The [manufacturer driver](https://github.com/pimoroni/pimoroni-pico/blob/main/drivers/st7789/st7789.cpp)
  confirms the RAMWR/DC protocol, 16-bit pixel data and inclusive 0..319 / 0..239
  window bounds. The existing panel orientation and RGB565 byte order remain.
- The published `embassy-rp` 0.10.0 source was inspected locally: `spi.rs::write`
  creates `tx_dma.write(...)` before its first await; `dma.rs::configure` writes
  the addresses/count, fences, and enables the channel. After DMA completion,
  SPI write waits for BSY to clear before returning. CS rises only afterward.
  Crate source commit: `84444a19eb57d978e3c09fcdc8e60cdd7278eb03`.
- The [Raspberry Pi SDK hardware reference](https://www.raspberrypi.com/documentation/pico-sdk/hardware.html)
  distinguishes abort completion and warns about RP2350-E5 when aborting a
  chained channel. This pipeline never cancels its payload future or chains DMA.
  The locked HAL Drop implementation waits synchronously for busy to clear;
  that is another reason to retain ownership and reset on a stall.
- `embassy-sync` 0.8.0 Channel rejects a full slot and retains a message until
  received, unlike Signal's overwrite semantics. Host tests use this exact
  channel with `critical-section` 1.2.0's host implementation. RP2350 spinlocks,
  IRQ delivery and real DMA ordering still require device tests.

## Evidence and remaining measurements

Tests cover delayed/immediate completion, first-poll order, bounded channel
ownership, stale/wrong/double events, quarantine and ID exhaustion. Compile-fail
tests reject mutation after submission and release while pixels are borrowed.
Stock and serial fallback builds pass. FPS now counts completed HAL transfers,
not render-loop iterations. This is software-observed completion; executor delay
can extend its measured duration after the wire transfer has finished.

Logic-analyzer acceptance remains open: trace payload start before the next
render, CS through the final SCK edge, alternating buffer contents, DMA stalls,
and physical scan/TE behaviour. Host tests cannot prove physical FPS, tearing,
input latency, overclock stability or ECU support.
