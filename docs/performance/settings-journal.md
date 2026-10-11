# Settings journal and save policy

This stage adds shared `settings/mod.rs` and `settings/policy.rs`. The host uses
the production code against a NOR model. The subsequent runtime integration is
documented in [flash persistence](../hardware/flash-persistence.md); these tests
do not demonstrate persistence on the board.

## Permanent envelope, schema 1

Journal offsets are relative to the reserved 8192-byte region. Each 4096-byte
sector holds eight 512-byte slots. A slot has distinct data and commit pages;
each 256-byte page is programmed once after erase. No in-place commit-bit update.

| Data page bytes | Meaning |
|---|---|
| 0..4 | `EA88` permanent envelope magic |
| 4..6 | Little-endian payload schema, currently 1 |
| 6..8 | Little-endian payload length, currently 4 |
| 8..16 | Nonzero little-endian u64 generation |
| 16..20 | Brightness 0..100, restore brightness 1..100, boost unit 0/1, FPS mode 0..3 |
| 20..252 | Erased padding for schema 1 |
| 252..256 | CRC-32/ISO-HDLC of all bytes 0..252 |

| Commit page bytes | Meaning |
|---|---|
| 0..4 | `CMT1` permanent envelope magic |
| 4..12 | Same generation |
| 12..16 | Data page checksum |
| 16..20 | CRC-32/ISO-HDLC of commit bytes 0..16 |
| 20..256 | Erased padding |

Future payload schemas must retain this outer envelope/checksum placement.
Integrity is checked before interpreting the payload. An intact unknown schema
or length blocks saves, including a longer future payload; it cannot be treated
as an empty slot and overwritten by a downgrade. CRC detects corruption under
the tested fault model; it is neither authentication nor collision-free proof.

Scanning reads all 16 pairs. A corrupt/torn slot remains consumed. Duplicate valid
generations are ambiguous and block writes even if payloads match. Generation
zero is invalid; u64 exhaustion rejects changed saves. With no valid records,
the caller selects safe defaults (100% brightness, BAR, FPS off), while receiving
the invalid-record count. Read errors propagate and cannot masquerade as blank.

A save returns early without writes if the latest record already matches. It
uses an erased slot, or erases the sector opposite the newest valid record when
both banks are full. It blank-checks every erased page and the destination pair,
checks data after programming, then checks commit and both pages again. Only
matching validated readback reports success. A failed call can still have committed
the new record before its error; boot scanning, not the prior return value,
determines the durable result. No internal retry/feed loop is present.

## Coalescing and failure handling

The single-owner policy waits two seconds after the last actual preference change,
limits attempt starts to one per 30 seconds, and allows three attempts per changed
desired value. Unchanged calls neither extend debounce nor renew retries. No
periodic writes occur while clean. Tickets contain an immutable settings snapshot
and are move-only; callers must return each to its issuing instance.

New desired values arriving during a transaction remain dirty after the older
snapshot is acknowledged. Failed saves retain uncertainty because a commit may
have preceded a readback error: reverting to the previously acknowledged value
still needs a verified write. Clock regression or attempt-identity exhaustion
disables persistence. Unsupported/ambiguous boot storage can construct an explicitly
disabled policy while keeping live UI preferences usable.

## Host verification and first CPU baseline

Fifteen new test functions cover the schema/controller contracts, including:

- Every byte cut in both page programs and all 4097 erase-prefix cut points at
  a rollover with two populated banks.
- All 4096 single-bit corruptions of a complete record pair, 1024 deterministic
  arbitrary torn-bit patterns, and failures at every read boundary in a save.
- Silent erase/program failures, read errors, foreign commits, all unit/FPS bytes,
  brightness ranges, unknown longer schemas, zero/MAX/duplicate generations,
  45-save rollover, unchanged saves, debounce, retries, reverts and stale tickets.

Both outline modes pass 219 runtime tests and two compile-fail checks. Strict
host/all-target and Longan firmware Clippy pass; existing upstream warnings remain.
Those measurements describe the pure-module prerequisite commit.

Source-frozen Criterion `stage07_journal`, 30 samples per fixture on the existing
Windows host:

| Fixture | Host median |
|---|---:|
| Scan two full sectors | 14.500 us |
| Complete rollover transaction against NOR memory model | 17.885 us |

The transaction uses a separately cloned, identically seeded 16-record image per
iteration. Memory-model erase/program are not flash timings. These are first
correctness baselines, not measured speedups or device latency bounds. Raw samples,
compiler/command and pre-run source archive are in `host/baselines/stage07_journal`.

See [hardware integration requirements](../hardware/flash-persistence.md) for
the integrated DMA/I2C/UART, supervisor and XIP contract and remaining physical
power-cut gates.

## Maintenance model and integrated checkpoint

Source-frozen `stage07_maintenance` repeats the same journal fixtures and adds
one complete healthy supervisor grant, two-owner park, write admission/check,
release, two-owner resume and health completion. It measures shared CPU model
work only: no RTOS scheduling, spinlocks, hardware IO, ROM erase or watchdog.

| Fixture | Host median, 30 samples |
|---|---:|
| Scan two full sectors | 14.062 us |
| Complete rollover transaction against NOR memory model | 17.844 us |
| Full maintenance model transition | 35.093 ns |

The journal changes are within ordinary run-to-run variation; no isolated gain
is claimed. The maintenance fixture is its first baseline. Raw samples, source
archive, command and compiler identity are in `host/baselines/stage07_maintenance`.
