# Diagnostic transport contracts

This stage adds shared pure modules exercised on the host and compiled for
RP2350. They are not yet connected to UART. No captured DNUC ECU reply or physical
CAN behavior has been validated.

## Wire and ownership boundaries

`CanFrame` represents a standard data frame with a known DLC of zero through
eight. Its fields are private and construction checks both ID and length.
Longan transmission requires exactly eight data bytes and emits TX14 with a
big-endian ID and zero EXT/RTR flags.

`Rx12` separately produces `RawRecord`. It deliberately provides no implicit
conversion to `CanFrame`: the receive wire carries neither DLC nor EXT/RTR, and
the published firmware serializes an unspecified tail after a short CAN frame.
All byte splits preserve a record only after a confirmed configuration boundary.
A known error discards the partial record, changes epoch and refuses further
bytes until another qualified transition. Invalid standard IDs also close the
stream. Neither silence nor a plausible ID can reestablish alignment. Undetected
insertion/deletion/substitution and whole-record loss remain protocol limitations.

The caller must qualify the exact bridge firmware's setting/data-mode banners,
consume the entire `ENTER DATA MODE` transition and retain following bytes from
that RX chunk. The future hardware owner must separately qualify standard DLC8
traffic or retain values as unavailable. A configuration ACK cannot prove filter
register readback; this firmware exposes no such readback command.

## ISO-TP and read services

The normal-addressed classical CAN receiver is bounded to 256 response bytes.
It validates SF/FF/CF shape against known DLC, checks the selected responder and
epoch, requires explicit flow-control completion, checks the four-bit sequence
including wrap, and aborts on malformed data or time regression. Replies beyond
capacity emit overflow flow control. No partial response is returned as complete.
CAN FD and extended-length forms are deliberately unsupported.

CTS requests a block size of four and STmin of 5 ms. N_Cr is 1000 ms; the
caller-provided total deadline is a separate application policy. These selected
flow-control settings have not been measured on this ECU. The receiver does
not infer actual CAN spacing from UART delivery timestamps. A missing FC completion
cannot admit CF bytes; transport TX completion will still be only UART completion
because this Longan firmware provides no CAN send result.

The request type exposes only OBD Mode01 and UDS ReadDataByIdentifier. Replies
must echo the requested service/PID/DID; negative responses retain their NRC,
and NRC78 is identified as pending. The next owner/scheduler slice must bound
pending extensions and correlate these events with one outstanding request.
No session change, security access, actuator control or write API is exposed.
No manufacturer DID is enabled by these raw read/response facilities.

Generic decoding currently includes PID05 coolant, PID0F intake, PID5C engine oil,
PID42 control-module voltage, and capability pages 00/20/40. Exact data widths are
required. Temperature arithmetic widens before subtracting 40; voltage uses BE16
millivolts without integer truncation. Capabilities belong to one ECU/epoch,
require advertised continuation pages, reject contradictory page rewrites and
map page-edge PIDs through the previous page. A missing response means unknown,
not unsupported. An explicit clear continuation bit excludes later pages.

## Three-check source record

| Contract | References checked |
| --- | --- |
| Longan TX14/RX12, commands and missing metadata | [Official documentation](https://docs.longan-labs.cc/1030001/), [official Arduino library](https://github.com/Longan-Labs/Serial_CAN_Arduino/blob/ffeaaa20b10d84771800b9407395f000dc8d4f1a/src/Serial_CAN_Module.cpp), [official bridge firmware](https://github.com/Longan-Labs/Serial_CAN_Bus/blob/c11ae43a81e9f9fc87fae56f70c6a14ccff53ce9/Serial_CAN_Firmware/cmdMode.cpp) |
| ISO-TP types, length, sequence and flow control | [Linux implementation](https://github.com/torvalds/linux/blob/a5ebb76233b79db01e82e061173cb73c6d2b5c6b/net/can/isotp.c), [Zephyr implementation](https://github.com/zephyrproject-rtos/zephyr/blob/920074c07cf4b503f70670cf424e4add4325d05a/subsys/canbus/isotp/isotp.c), [ISO15765-2:2016 original mirror](https://raw.githubusercontent.com/devcoons/iso15765-canbus/master/doc/ISO-15765-2-2016.pdf); also [ELM327 datasheet](https://www.elmelectronics.com/wp-content/uploads/2017/01/ELM327DS.pdf) |
| PID05/0F/42/5C and supported bits | [python-OBD](https://github.com/brendan-w/python-OBD/tree/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd), [ELMduino](https://github.com/PowerBroker2/ELMduino/blob/e9d6b55fd63f4afd7789d0981861e9990cdfcd8e/src/ELMduino.cpp), [ArduinoOBD](https://github.com/stanleyhuangyc/ArduinoOBD/blob/ed4dcb923153c2dcab2b88cab818c29c7f0820eb/libraries/OBD/OBD.cpp) |

These implementations corroborate the four basic formulas; their parser policies
are not copied. ELMduino's page-boundary helper and ArduinoOBD's integer voltage
normalization have limitations avoided here. Three checks do not mean every
upstream library agrees on every other PID.

MAP and wideband lambda remain unqualified. PID4F can override their scaling and
must be established per ECU/epoch. Current [CSS manufacturer DBC v4.3](https://canlogger.csselectronics.com/files/dbc-files/obd-dbc-files-v4.3.zip)
uses AB/32768 for default lambda; older SAE tables use 2*AB/65535. Some general OBD
libraries expose only the accompanying voltage/current or misroute wideband
through a narrowband decoder. These disagreements are preserved as evidence
limits. Neither PID44 commanded lambda nor catalyst PID3C is substituted for a
measured lambda or a qualified physical EGT sensor.

## Verification

Host fixtures cover every payload length 1..256, sequence wrap/block boundaries,
all record splits, fault at every byte, malformed/truncated/oversized frames,
wrong source/epoch, clock regression, deadline boundaries and missing FC. All
256 temperature inputs and 65536 voltage inputs are checked, with all 32 bits
of each capability page and source/epoch isolation. These are deterministic
software contracts, not vehicle replay captures. Live scheduling, UART recovery,
bridge qualification and physical validation remain separate gates.

An independent review reproduced an initially too-short N_Cr policy; the corrected
fixture now accepts a frame at 150 and 1000 ms and rejects 1001 ms while keeping
the total deadline at 5000 ms. No remaining blocking defect was identified in the
reviewed subset. Padding bytes are permissive, and invalid frames abort the
request; this is not a full ISO profile conformance claim.

125 runtime tests plus two compile-fail ownership checks pass in both outline
modes, with strict host/firmware lints and the stock target release/ELF gate.
The pure modules are not yet instantiated on the target, so linked static RAM
remains 374352 B with 149936 B headroom. The 30-sample host baseline in
`host/baselines/stage06_protocol` preserves source hashes and raw measurements:

| Fixture | Local median |
| --- | ---: |
| One synchronized RX12 record | 27.3 ns |
| Known-DLC ISO-TP single frame plus OBD voltage decode | 33.6 ns |
| 128-byte ISO-TP reassembly with block flow-control acknowledgements | 366.5 ns |

These are new parser CPU baselines; no UART waits, CAN traffic or physical
latency is measured. Adjacent host runs varied by several percent even in the
unchanged RX12 path, so no gain is attributed to the deadline correction.

## Single-owner transaction client

`client.rs` admits one outstanding request per configured ECU/transport epoch.
Separate private request-generation and transmission-serial tokens gate every
request and flow-control completion. UART completion must include hardware BUSY
clear; it does not prove CAN acknowledgement. A missing completion closes the
client until a newly qualified transport epoch, preventing interleaved records.

Application budgets are provisional: 100 ms per UART transmission, 500 ms first
response, 1000 ms after NRC78, at most three NRC78 replies, and an immutable
5000 ms transaction limit. ISO-TP retains its separate 1000 ms N_Cr limit.
Matching service/identifier echoes and exact decoder widths remain mandatory.
Foreign replies cannot renew a deadline or abort matching segmented reception.
Negative/malformed/oversized replies never become samples. An identical late
wire reply has no transaction identifier and cannot be proven new from its
contents; tokens protect local ownership, not wire-level freshness.

Review reproduced two corrected edge cases: overflow flow-control completion
after the first-response deadline, and a foreign single frame aborting a
matching segmented response. Regression tests cover both, three pending replies,
total/N_Cr deadlines, epoch/clock failures, and reuse of old request/FC tokens
within and across request generations. Nine client tests bring the suite to
134 runtime tests plus two compile-fail checks in both outline modes. Strict
lints and the stock target release/ELF gate pass; target RAM is unchanged because
the client is not instantiated until acquisition integration.

`host/baselines/stage06_client` preserves the two new 30-sample CPU fixtures,
source hashes and command: single-frame transaction plus OBD decode approximately
72 ns, and 128-byte transaction with all FC acknowledgements approximately
630 ns. These measure more work than the lower-level parser fixtures and are
not an optimization comparison or a physical latency claim.

## Capability-scoped polling and publication

`Session` binds one physical responder 7E8..7EF to its corresponding request
address (response ID minus eight). Seven cyclic slots discover 00/20/40 and poll
05/0F/42/5C. Later capability pages require an advertised continuation bit;
unsupported status requires a valid bitmap. A timeout or negative response
cannot establish unsupported status. Cyclic selection lets already-discovered
channels run while another capability page fails.

Provisional periods are 1000 ms for coolant/intake, 500 ms for ECU voltage and
1500 ms for engine oil, measured from successful receipt. Successful transactions
have a 20 ms gap. Failed requests back off 250..8000 ms and impose a 1000 ms
global quiet interval. These reduce load and late-reply overlap; they do not
solve the wire protocol's missing transaction identifiers. No channel becomes
valid until its exact decoded reply is accepted by the typed acquisition model.

Timeout/malformed refreshes suppress the displayed value and preserve last-good
history/extrema. Negative replies leave the channel unknown. Known wire loss,
abandoned transmission or clock regression invalidates every current channel
and blocks polling until a newer qualified transport epoch. Recovery discards
capabilities, preserves request generations and rediscovers support. Service
progress during ECU sleep is separate from receiving measurements.

Timestamps record software receipt, not ECU measurement time or CAN bus arrival.
The existing UI staleness limits remain independent of scheduling. DSG/EGT,
boost and measured lambda stay unknown because no qualified decoder is enabled.

Eight session replay tests cover discovery, fair polling, absent pages, malformed
widths, last-good statistics, repeated silence/backoff, negatives, wrong ECU and
epoch, stale TX completion and clock failure. The clock-failure fixture first
reproduced stale valid status, then passed after centralized fatal invalidation.
142 runtime tests plus two compile-fail checks pass in both outline modes, with
strict lints and the stock target release/ELF gate. Independent review identified
no remaining actionable defect in this slice. UART integration is still separate.

The new `host/baselines/stage06_session` fixture includes a complete coolant
transaction, typed statistics and a snapshot, after capability discovery outside
the measured loop: local median 420 ns over 30 samples. Raw measurements, hashes
and command are preserved. This is a host CPU baseline, without transport waits.
## Adapter bootstrap and UART write ownership

The shared `bootstrap` module implements one bounded mode transition for a
preprovisioned Longan firmware: 50 ms of command guard quiet (maximum 1 s),
separate `+++`, the complete setting banner, an `AT` round trip, then `AT+Q`
and the complete `OK\r\nENTER DATA MODE\r\n` response. Quiet never establishes
record alignment. Expected replies are exact and incremental; unexpected bytes
fail closed. Bytes following the final banner go straight to RX12. Commands
have unique lifetime tokens, 100 ms TX and 500 ms total reply budgets. Partial
reply progress does not extend a deadline. Stale acknowledgements cannot move
the active clock or phase. Clock regression and identity exhaustion are terminal.

The shared `serial_tx::Pump` retains every partial write offset and copied byte
across Pending polls. It allows only one short record, then requires software
flush and a separate observed `BUSY == false` before reporting its token. Abort
stops the uncopied remainder and drains the queued prefix within the original
100 ms deadline. Invalid progress, a regressed clock or a stalled write
quarantines further writes until the owner is recreated after reset. Empty-ring
flush alone is never CAN acknowledgement or even final UART stop-bit completion.

Eight host handshake tests, five pump tests and an internal identity-exhaustion
test cover strict replies, every final-banner split, binary carry-over, partial
banners across loss, stale/duplicate TX callbacks, quiet flooding, exact deadline
edges, every record size and partial-copy width, abort, BUSY, impossible write
counts and near-maximum timestamps. These are shared software tests. They cannot
validate the installed bridge firmware, electrical link, IRQ timing or ECU.

Commands match the [vendor protocol documentation](https://docs.longan-labs.cc/1030001/)
and [exact public firmware](https://github.com/Longan-Labs/Serial_CAN_Bus/blob/c11ae43a81e9f9fc87fae56f70c6a14ccff53ce9/Serial_CAN_Firmware/cmdMode.cpp).
The runtime recovery handshake does not write baud, CAN rate or filters to
EEPROM. Those must be provisioned and physically verified separately; repeated
recovery must not wear EEPROM or assume a successful AT response proves hardware
filter configuration. Default baud is 9600, while the planned explicit deployment
profile uses preprovisioned 115200. No automatic baud scan is implied.

## Shared vehicle service

`Vehicle<Port>` owns bootstrap, RX12 phase, the write pump, transaction scheduling
and acquisition history. One pass reads at most 32 bytes and performs at most
four TX phase transitions. The real hardware wrapper must provide bounded,
cancel-safe buffered operations. Software flush and observed UART BUSY clear
precede transaction acknowledgement; early replies remain queued until then.

Default `Profile::UNQUALIFIED` allows only the handshake and a 16-record raw
trace. It never polls CAN or publishes vehicle values. An explicitly selected
`PaddedStandardDataDlc8` contract requires external qualification of the missing
DLC/flags and selected responder filters. The type cannot prove those facts.
Unexpected responders under that contract invalidate the transport.

The review reproduced a causal bug: the final 21-byte banner plus an old RX12
reply straddled the read budget, so its last byte could complete under a new
request. Admission now requires an observed empty host read and a complete
record boundary. Every partial-record split is covered by replay tests. Active
transaction deadlines advance even with receive backlog. Incomplete records
have a fixed, inclusive 100 ms first-byte budget; expiry invalidates the epoch
and requires a new handshake, without shifting byte phase. This is a host policy,
not an adapter timing guarantee. Unobserved adapter/FIFO data and identical late
ECU replies remain indistinguishable without wire transaction IDs/timestamps.

RX loss invalidates parser/session before bounded discard. Recovery drains any
queued TX prefix and waits 250..30000 ms between failed attempts, with no EEPROM
writes. A stalled TX, invalid IO count, regressed clock or exhausted identity
quarantines the owner until reset. ECU silence merely times out a query; it does
not restart the adapter. Watchdog progress must track service execution rather
than ECU activity. Raw trace overwrites are counted separately from byte loss.

Fourteen service replay tests and one terminal-error precedence unit test cover
these policies. The total suite has 179 runtime tests and two compile-fail checks
in both outline modes; strict lints and stock release/ELF checks pass. The new
30-sample `host/baselines/stage06_service` fixture measures two service passes,
a synthetic coolant transaction and a snapshot after discovery. Its median is
about 569 ns, with raw samples and source hashes retained. It excludes UART
wire/IRQ time and is not a comparison to the smaller session-only fixture.

## Core 1 runtime and qualification gate

Build `--features longan-rx12` to instantiate the bridge owner. It is mutually
exclusive with `demo`; the stock build leaves vehicle data unavailable. Only
uninitialized UART0/GP0 TX/GP1 RX tokens cross into core 1. `HardwarePort::new`
asserts core 1 before constructing the reviewed, non-Send/non-Sync buffered HAL
and enabling its IRQ there. Static buffers are 1024 RX bytes and 64 TX bytes.
The deployment profile is preprovisioned 115200 8N1 and `UNQUALIFIED`, so enabling
this feature does not enable CAN requests or manufacture missing RX12 metadata.

The acquisition task runs one bounded service pass followed by a 1 ms timer.
There is no overdue tick catch-up loop. It publishes coherent changed snapshots
at most every 10 ms and retains every accepted sample in owner-local history
before coalescing. Link state/counters are published separately; the 16-record
raw trace remains in owner storage. Statistics resets remain acknowledged by
generation. Service watchdog progress advances even when the ECU is asleep,
the adapter is disconnected or the port has been quarantined.
Quarantine deliberately requires a user/power reset; it does not trip the
task-progress watchdog or repeatedly emit recovery commands. That watchdog is
reserved for stalled essential execution. Detailed link-state presentation is
added with the following UI/recovery stage.

Each IO method polls exactly one buffered HAL future, then drops it. The exact
vendored implementation copies no bytes on Pending and retains no DMA/user
slice. This does not generalize to arbitrary implementations of the async IO
traits. Blocking TX, legacy NB TX, `send_break`, `read_exact` and `write_all` are
excluded. The wrapper separates software flush from physical UART BUSY clear.

The [async IO 0.7.0 release notes](https://github.com/rust-embedded/embedded-hal/blob/master/embedded-io-async/CHANGELOG.md),
[current trait documentation](https://docs.rs/embedded-io-async/0.7.0/embedded_io_async/),
locked trait source and actual HAL read/write/flush paths were checked. Adding
the direct optional trait dependency changes no resolved crate version.

Local strict Clippy passes stock, Demo and Longan configurations, and combined
Demo/Longan selection is rejected. Optimized linked static RAM/headroom is
374392/149896 bytes stock, 377048/147240 bytes Longan, and 383832/140456 bytes
Longan with both traces at 300 MHz. Host replays cover the shared owner; target
builds only check compilation/layout. Physical IRQ masking, FIFO overrun,
baud/CAN configuration, exact ECU response identity and actual FPS remain open.
In particular, the 1024-byte ring does not relax the roughly 347 us remaining
FIFO budget at the HAL's 28/32-byte RX threshold and 115200 8N1.
