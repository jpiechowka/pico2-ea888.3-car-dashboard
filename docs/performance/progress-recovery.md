# Progress supervision and RP2350 recovery

The watchdog starts immediately after HAL initialization and before core 1 is
launched. The initial 15-second limit covers startup, including the current
roughly 13-second cosmetic boot sequence. Once that finishes, a core-0 task
checks progress every 50 ms and feeds a two-second hardware deadline only while:

- acquisition task progress has changed within 500 ms;
- rendering progress has changed within 500 ms;
- input task progress has changed within 500 ms, including completed reconnect failures;
- completed transfers have changed within 250 ms while display work is pending;
- the supervisor itself has not missed more than 250 ms or seen time regress.

Any fault latches until reset. No further feeding occurs, even if a counter
later changes. This checks task operation: lack of valid ECU measurements must
produce stale/unavailable data, not be confused with a dead acquisition task.
The no-damage display path marks display idle and sleeps for 16 ms while
keeping acquisition, input and UI progress requirements. Queuing a later
transfer starts a fresh completion deadline. The queue generation counter is
observed as well as the pending flag, so a short idle transition missed between
supervisor polls cannot give a new transfer the previous job's deadline.

The transfer timeout cancels only channel reception. A stalled SPI/DMA future
keeps its framebuffer until hardware reset. The watchdog is intended to cover
HAL busy loops, lost timer alarms, a dead executor, and panics that stop useful
progress. No software reset or unsafe lease release is attempted on timeout.

## Register correction and checks

The published Embassy RP 0.10.0 watchdog uses `0x0001fffc` for PSM.WDSEL, the
RP2040 layout. On RP2350, ROSC/XOSC are bits 2/3 and the register extends through
bit 24. The private `RecoveryWatchdog` wrapper immediately replaces that mask
after HAL start with `0x01fffff3`, then checks the readback, enabled/running 1 MHz
tick, watchdog enable and disabled debug-pause bits.
It exposes only feeding. Calling raw HAL start or trigger_reset afterward would
restore the incorrect mask and is prohibited by this wrapper's interface.

Cross-checked against:

1. Raspberry Pi's [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf),
   section 7.4 PSM reset sequencing and WDSEL register layout.
2. The official SDK's [watchdog implementation](https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/rp2_common/hardware_watchdog/watchdog.c)
   and [RP2350 register definitions](https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/rp2350/hardware_regs/include/hardware/regs/psm.h).
3. Exact locked `embassy-rp` 0.10.0 watchdog/clocks source and `rp-pac` 7.0.0
   RP2350 PSM/TICKS accessors. Enabling `unstable-pac` exposes those existing
   typed accessors without changing HAL runtime features or locked versions.

The original mask is inconsistent with RP2350 and the SDK. Since PSM resets are
sequenced, missing CPU bits alone does not prove a particular observed reset
failure. No such physical failure has been reproduced here.

Embassy's [RP2350 lost-alarm report #6528](https://github.com/embassy-rs/embassy/issues/6528)
remains relevant. Also, the [DMA EN-clear fix #6002](https://github.com/embassy-rs/embassy/pull/6002)
is merged upstream but absent from the latest published HAL. This application
does not chain or cancel payload DMA, and makes no general cancellation-safety
claim for that HAL version.

## Verification boundary

Host fault tests exercise independent stalls, healthy counter wrap, idle-display
rules, late supervisor polls, clock regression and latched failure. Strict host
and firmware Clippy plus stock/300 release builds pass. Physical acceptance
requires injecting each stall, measuring the reset delay, checking both cores
and peripherals restart, and verifying cold/USB/SWD/watchdog boot and timer-wrap
soak. The watchdog does not remove those hardware validation requirements.

Recovery still depends on the shared reference clock; this cannot guarantee recovery from every hardware failure.
