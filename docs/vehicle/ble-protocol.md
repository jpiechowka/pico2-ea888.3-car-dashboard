# Bounded BLE adapter protocol prototype

The shared `dashboard_core::ble` module is a hardware-independent, allocation-free
ELM ASCII transaction owner. Real adapters use `Mode::AdapterOnly`, which can
initialize and identify the adapter but cannot construct a diagnostic transmission.
No exact DNUC ECU source is qualified by this prototype. Existing widgets remain
unavailable until independent source and vehicle qualification; no disconnect or
failed request substitutes Demo data.

`Mode::SyntheticPeer` is only for the explicitly selected test profile and a peer
that exposes the firmware's unique test service. Selecting an MC+ or CX profile
must never select this mode. It enables typed synthetic Mode01 fixtures for
00/20/40/05/0F/42/5C and read-only Mode22 DID echo/width fixtures. Their completions
remain labelled `SyntheticPeer`; DID bytes have no production measurement decoder.
The module exposes no arbitrary AT string, service byte, write, session change,
unlock, actuator command, ECU discovery sweep or manufacturer batch request.

## API and ownership

One owner retains a `Session` across connections. Call `connect(epoch, now_ms)`
only after establishing a fresh connection, discarding old RX/TX work and attaching
the notification source to that epoch. Epochs must strictly increase; command
serials never reset across connections. Neither counter wraps.

Use `next_initialization()` and `Command::Initialize`, or `Command::Identity`.
`begin(command, now_ms, budget_ms)` returns at most 32 owned ASCII bytes and an opaque
token. Retain partially written bytes until the whole command has completed, then
call `sent(token, now_ms)`. A complete GATT write is not ECU receipt. Early notifications
must remain in the owner's bounded queue until `sent`; feeding them early closes
the session. A partial write, cancellation, disconnect or flash park calls
`invalidate()`, cancels outstanding work and requires a new connection epoch.

`notify(epoch, sequence, bytes, now_ms)` accepts at most 244 bytes per call. Sequence
starts at one and counts actual notifications at ingress, before any queue can
drop them. A gap or duplicate closes the epoch. If the BLE stack reports lag
without a usable sequence, invalidate directly. Locally assigned consecutive
numbers after an unnoticed queue drop cannot detect that drop. Old-epoch callbacks
and old TX tokens cannot move the new owner's clock or state.

`tick(now_ms)` must run independently of receive activity. The inclusive deadline
is fixed at command creation, including write time, and bounded to 5000 ms.
Timeout, malformed/foreign bytes, overflow, notification loss, partial TX and
unexpected input invalidate the session. Clock regression and identity exhaustion
are terminal for that owner. Recovery requires recreating the surrounding owner
only after its clock/identity lifecycle is safe. A later prompt cannot resurrect
a timed-out transaction.

Only a validated response followed by a prompt produces `Completion`. It carries
command/token, mode, command-start time and actual software prompt-receipt time.
The owner must not replace that time with a later publication/render timestamp.
Receipt time is not ECU measurement time, CAN arrival time or radio arrival time.

## Exact ASCII and CAN format

The initialization sequence is `ATE0`, `ATL0`, `ATS1`, `ATH1`, `ATD1`, `ATM0`,
`ATSP6`, `ATCEA`, `ATCAF0`, `ATCFC1`, `ATSH7E0`, `ATCRA7E8`. These commands choose
spaces, headers, visible DLC, normal addressing, 11-bit 500 kbit/s CAN and the
synthetic engine route without sending a diagnostic query. Each step requires
`OK` followed by a prompt. `ATI` returns a bounded identity line. Identity strings
do not prove genuine hardware or supported commands; every step is checked.

The configured receive format is explicitly `7E8 8 03 41 05 82 00 00 00 00`:
three hex identifier digits, one DLC digit, then exactly DLC two-digit octets.
Missing headers, missing DLC, extended IDs, RTR, CAN FD and already-reassembled
CAF1 text are rejected. CR/LF lines and `>` prompts are parsed byte by byte across
arbitrary notification boundaries. One exact command echo is consumed, never
treated as a response. Unknown echoes, duplicate responses and any non-CR/LF bytes
after a prompt in the same notification reject the entire completion.

Line and diagnostic payload capacities are each 64 bytes. Whole transactions are
limited to 1024 received bytes. Classical ISO-TP single/first/consecutive frames
require bounded length, selected responder, correct sequence and complete payload.
Adapter-managed flow control is enabled; the host never transmits FC and never
pretends that a local FC callback proves an adapter action. This is why the raw
Longan `IsoTpRx` owner is not reused: it requires explicit host FC transmission.
The existing checked `CanFrame`, read-only `Request`, service/identifier matching
and generic OBD decoders are reused without a second conversion implementation.

Up to three matching NRC78 frames may precede a final response within the original
deadline. No pending response extends it. `NO DATA`, error statuses, a negative
response or a prompt ending a pending-only reply produce a typed non-sample outcome
and invalidate the epoch. The prototype deliberately reconnects after these
outcomes; it does not issue a new identical query over an ambiguous timed-out stream.
Identical unsolicited/late wire replies after successful transactions remain
indistinguishable in ELM text, which has no transaction identifier. Epochs protect
local callbacks; they cannot prove ECU freshness or flush an adapter's hidden FIFO.

## Polling and status

`poll::Scheduler<N>` accepts one to eight cadences, a successful/failed completion
gap and the owning Session's nonzero epoch. It inspects at most N slots and admits
one transaction per call. Round-robin selection among due channels prevents a
continuously due fast channel from starving slow values. Periods start at actual
completion, missed ticks are discarded, and failure never updates the last-success
timestamp. Deadline, generation and latency arithmetic is checked. Stop it on
disconnect or flash park and recreate only under a newer epoch; stale tickets
cannot complete a new epoch's work.

The occupancy estimate `utilization_permille` compares configured periods with an
observed transaction cost plus gap, rounding up. For eight 100 ms channels, an
illustrative 40 ms round trip plus 5 ms gap requires 3600 permille of serialized
capacity; 10 ms and no gap requires 800. These are arithmetic examples, not measured
adapter rates. Display freshness limits remain in the existing sample-quality
contract. Completion timestamps and latency statistics do not make stale data fresh.

`link::LinkSnapshot` supplies bounded shared firmware/Studio state: Disabled,
Starting, Connecting, Securing, Discovering, Identifying, AdapterReady,
SyntheticReady, Backoff, Maintenance and Fault. It carries epoch, completed-command
and notification-loss counters, optional latency and a typed fault. AdapterReady
asserts only successful software adapter bring-up, never vehicle compatibility.

## Source checks and verification scope

The [ELM327 datasheet](https://www.elmelectronics.com/wp-content/uploads/2017/01/ELM327DS.pdf),
especially CAF/CFC, headers/DLC, read services and prompt handling, and the
[OBDLink family programming manual](https://www.scantool.net/scantool/downloads/98/obdlink-frpm.pdf)
were checked on 2026-10-18. Parser fixtures explicitly choose raw CAN text and
adapter-owned FC. This is a source-backed prototype contract, not qualification of
a particular adapter clone, installed firmware, CAN route, padding or ECU.

Focused shared-core tests cover fragmented prompts/replies, loss at every split,
all 61 bounded DID data widths with 17 deterministic fragmentation partitions each,
overflow, malformed/foreign/duplicate frames, early RX, echo handling, NRC/error
states, immutable deadlines, time reversal, stale callbacks, exhausted identities,
fair polling, failed freshness and stale scheduler tickets across owner recreation.
Host correctness, CPU timings, ARM image resources and physical radio/vehicle
validation remain separate evidence. No vehicle requests or flashing are needed
to execute these tests.
