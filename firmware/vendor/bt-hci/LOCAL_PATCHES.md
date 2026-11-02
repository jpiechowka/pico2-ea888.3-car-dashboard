# Reviewed HCI controller changes, 2 November 2026

Base is complete published `bt-hci 0.10.1`, upstream commit
`72e72f2515ef0372d6eea431ce49b258fbfa96a5`. Original files and both licenses
remain inventoried separately in `../bt-hci-provenance.json`; no release upgrade.

## Command slots, cancellation and wakeups

The original slot-release path never signaled its registered waker. Actual
concurrent command futures reproduce the hang even with available controller
credits. [Upstream issue66](https://github.com/embassy-rs/bt-hci/issues/66) also
records the overwritten-multiple-waiter problem. Current main and release0.11.0
still contained both mechanisms when checked. Replace the single waker with the
already-used futures-intrusive crate's LocalManualResetEvent. Each caller owns
its intrusive node; a release pulses all registered waiters after ending the
slot-table borrow. Per-waiter completion survives reset of the event. Slot check
and first registration execute in one poll on this non-Sync controller.

The controller allowance is an executor-local Cell, replaced by each accepted
report, including decreases and zero. Credit and slot admission occur in one
poll, with no reserved credit retained while waiting. Both async and blocking
receive apply status-free opcode-zero CommandComplete reports before returning
the event. Credit updates pulse the same intrusive event after publication.
This follows [Core 6.2 Vol 4 Part E sections 4.4, 7.7.14 and 7.7.15](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host-controller-interface/host-controller-interface-functional-specification.html).
The previous additive semaphore update incorrectly retained a higher allowance;
a fair semaphore cannot safely revoke already-notified reservations.
Pending slots retain a pinned caller buffer. A response removes that pointer,
records Responded and publishes exactly one result; duplicate unread responses
cannot overwrite it or repeat Reset's broadcast. Such a duplicate is malformed
controller input and is ignored in its entirety, including its credit field;
legitimate standalone allowance changes use opcode zero and are always applied. A canceled Pending command
becomes Abandoned with opcode and response-capacity only. Its late completion
is consumed before reuse, without accessing the old caller's memory.

Cancellation after transport polling cannot prove whether a write was submitted.
An abandoned command can therefore remain quarantined indefinitely. Successful
Reset clears abandoned work and fails other pending commands; failed Reset does
not. Reset can use a free slot with zero command credits. If every slot is
quarantined, recovery requires resetting the transport/device and reconstructing
the controller; the patch does not steal an unresolved opcode to manufacture a
Reset slot. Applications must retain their bounded recovery/watchdog policy.

## Receive bounds

Async and blocking receive validate matched live or abandoned return capacity
before copying, signaling or publishing credits. Oversized returns and malformed
nonempty Reset returns fail with ReadHciError::InvalidValue. The host must terminate
and recover after this boundary error. Unknown opcodes do not complete another
command; typed decoding retains exact success-length/value checks and legal
short failure statuses. The prior failure was a bounds-checked panic, not an
established memory-corruption primitive or peer-controlled HCI attack.

Eighteen native boundary tests accompany the unchanged upstream suite. Baseline
failures demonstrated missed wake, oversized panic, duplicate-result overwrite,
canceled-opcode reuse, credit leakage, failure to lower allowance, and ignored
opcode-zero Complete credits. The additional credit tests cover both receive
APIs and both event layouts, revocation after wake, and notified-waiter cancellation. Further regressions cover multiple
waiters, cancellation during transport write, Reset success/failure/duplicates,
blocking receive, zero-byte async returns and malformed abandoned replies.
Strict Clippy required removing one unused higher-ranked lifetime and converting
two unchanged upstream test futures to async fn; no warning allowances were added.
No HCI slot count, radio queue, memory limit, transport command or feature was
removed. Intrusive waiter futures require linked ARM memory/stack review.

## Primitive, packet and retained declaration boundaries

Fixed-size array admission now validates each element (including boolean/enum
representations) before constructing references. Truncated and zero-sized dynamic
arrays are bounded. Async byte slices use the same payload count as blocking
writers. Packet/event parsers retain exact tails and correct SCO/ISO length fields;
size maxima and writers reject unrepresentable lengths before publishing a wrapped
header. Tests retain deliberate stream-prefix effects where transport I/O itself
fails; no atomic-write guarantee is invented.

Inquiry, periodic-advertising and channel-sounding records follow the specified
interleaved layout and validate enum fields before exposing typed records. Retained
LE declarations correct RF opcodes/PHY kinds, ISO widths, CIG/BIG command layouts
and bounded variable-count owned returns. ExternalController no longer requires
those return types to be fixed-size when SyncCmd already supplies exact decoding.
See [the complete declaration review](../../../docs/dependencies/bt-hci-declarations-2026-11-02.md)
for normative wire fixtures, public API migrations and retained limitations.
Root packet/primitive and independent declaration regressions supplement the
controller tests above. These changes validate host software boundaries, not
physical ISO/RF behavior or peer control of malformed controller envelopes.

ReadStoredLinkKey's retained classic-Bluetooth return now contains two u16 counts
without a second Status field. ExternalController already handles that status;
the old three-byte return rejected the legal four-byte count body. Two failing
baseline regressions cover exact typed decoding and actual controller completion,
then zero/full-range counts and short HCI failure. Core6.2 Vol4E7.3.8 defines the
status byte followed by two two-byte counts. No selected LE image calls this API.
