//! Optional browser context sink. Admission never waits for terminal output.
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use dashboard_helper_protocol::diagnostics::*;
use dashboard_helper_protocol::{Error, decode};

use crate::jobs::logs::StreamFramer;
/// Maximum sanitized terminal lines waiting for the detached sink thread.
const MAILBOX: usize = 32;
/// Private terminal message assembled solely from validated diagnostic metadata.
struct Line {
    /// Warn/error classification selecting optional terminal colour.
    severity: Severity,
    /// Sanitized metadata line; excludes browser message/subject payloads.
    text: String,
}
/// Optional nonblocking browser-diagnostic mailbox with independent ten-per-second admission.
pub struct Relay {
    /// Bounded channel; full or disconnected receivers count as explicit drops.
    sender: SyncSender<Line>,
    /// Rate state acquired with try_lock so HTTP admission never waits for another batch.
    gate: Mutex<RateGate>,
    /// Helper-relative monotonic ingress clock origin.
    origin: Instant,
    /// Saturating count of actual stderr write failures, separate from mailbox drops.
    failures: Arc<AtomicU64>,
}
impl Default for Relay {
    /// Returns a new relay and attempts to start its detached terminal sink.
    fn default() -> Self { Self::new() }
}
impl Relay {
    /// Returns an empty relay and starts a detached stderr writer, coloured only for a terminal without NO_COLOR.
    /// Thread creation failure leaves a disconnected mailbox: later admission reports drops. Dropping the relay
    /// closes its sender but does not join the sink or guarantee pending lines reached the terminal.
    pub fn new() -> Self {
        let (sender, receiver) = sync_channel::<Line>(MAILBOX);
        let failures = Arc::new(AtomicU64::new(0));
        let failed = failures.clone();
        let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let _ = std::thread::Builder::new()
            .name("studio-diagnostic-relay".into())
            .spawn(move || {
                while let Ok(line) = receiver.recv() {
                    let mut out = std::io::stderr().lock();
                    let result = if color {
                        writeln!(
                            out,
                            "\x1b[{}m{}\x1b[0m",
                            if line.severity == Severity::Error { 31 } else { 33 },
                            line.text
                        )
                    } else {
                        writeln!(out, "{}", line.text)
                    };
                    if result.is_err() {
                        let _ = failed.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| Some(n.saturating_add(1)));
                    }
                }
            });
        Self {
            sender,
            gate: Mutex::new(RateGate::default()),
            origin: Instant::now(),
            failures,
        }
    }

    /// Returns the relaxed, saturating stderr-write failure count; excludes queue/rate drops and spawn failure.
    pub fn failed_writes(&self) -> u64 { self.failures.load(Ordering::Relaxed) }

    /// Decodes validated diagnostic `bytes`; returns mailbox-admitted/dropped counts using helper elapsed milliseconds.
    /// Returns wire errors or Busy for a contended/poisoned rate lock. Admission does not prove terminal display or job
    /// evidence.
    pub fn admit(
        &self,
        bytes: &[u8],
    ) -> Result<Response, Error> {
        self.admit_at(
            bytes,
            self.origin.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        )
    }

    /// Returns admission/drop counts for wire `bytes` at helper-relative `now` milliseconds, clamped by RateGate.
    /// Rate slots are consumed before mailbox admission, including full/disconnected drops. Decode/lock/framing errors
    /// return Err; a later framing error would retain earlier sends and consumed slots.
    fn admit_at(
        &self,
        bytes: &[u8],
        now: u64,
    ) -> Result<Response, Error> {
        let request: Request = decode(bytes)?;
        let mut gate = self.gate.try_lock().map_err(|_| Error::Busy)?;
        let mut reply = Response {
            version: 1,
            admitted: 0,
            dropped: 0,
        };
        for event in request.events.iter() {
            if !gate.admit(now) {
                reply.dropped += 1;
                continue;
            }
            let raw = format!(
                "[Studio browser {:?}] ingress+{}ms client#{} client+{}ms {:?}/{} workspace={:?} \
                 accepted-revision={}; inspect browser history for local message",
                event.severity,
                now,
                event.sequence.as_str(),
                event.client_elapsed_ms.as_str(),
                event.category,
                event.code.label(),
                event.workspace,
                event.accepted_revision.as_str()
            );
            // Use the same framing/sanitizing boundary as native job output. The DTO
            // contains only closed enums and canonical integers, never user text.
            let mut framer = StreamFramer::new(&[])?;
            let mut text = String::new();
            framer.push(raw.as_bytes(), |line, _| text.push_str(&line))?;
            framer.finish(|line, _| text.push_str(&line))?;
            if self
                .sender
                .try_send(Line {
                    severity: event.severity,
                    text,
                })
                .is_ok()
            {
                reply.admitted += 1;
            } else {
                reply.dropped += 1;
            }
        }
        Ok(reply)
    }
}
#[cfg(test)]
/// Deterministic mailbox/rate and private-payload rejection tests without terminal threads.
mod tests {
    use dashboard_helper_protocol::{BoundedVec, Text, encode};

    use super::*;
    /// Returns four valid metadata events retaining a sequence above JavaScript's exact-integer range; panics on
    /// fixture drift.
    fn bytes() -> Vec<u8> {
        encode(&Request {
            version: 1,
            events: BoundedVec::new(vec![
                Event {
                    severity: Severity::Warn,
                    category: Category::Storage,
                    code: Code::StorageFailed,
                    workspace: Workspace::Diagnostics,
                    sequence: Text::new("9007199254740993").unwrap(),
                    client_elapsed_ms: Text::new("7").unwrap(),
                    accepted_revision: Text::new("4").unwrap()
                };
                4
            ])
            .unwrap(),
        })
        .unwrap()
    }
    #[test]
    /// Full mailboxes report drops, consume rate slots, preserve integer text and reopen exactly at the time boundary.
    fn nonblocking_mailbox_and_rate_drops_are_explicit() {
        let (sender, receiver) = sync_channel(1);
        let relay = Relay {
            sender,
            gate: Mutex::new(RateGate::default()),
            origin: Instant::now(),
            failures: Arc::new(AtomicU64::new(0)),
        };
        assert_eq!(
            relay.admit_at(&bytes(), 0).unwrap(),
            Response {
                version: 1,
                admitted: 1,
                dropped: 3
            }
        );
        let line = receiver.try_recv().unwrap();
        assert!(line.text.contains("client#9007199254740993"));
        assert!(!line.text.contains('\x1b'));
        relay.admit_at(&bytes(), 0).unwrap();
        relay.admit_at(&bytes(), 0).unwrap();
        receiver.try_recv().unwrap();
        assert_eq!(relay.admit_at(&bytes(), 999).unwrap().dropped, 4);
        assert_eq!(relay.admit_at(&bytes(), 1000).unwrap().admitted, 1);
    }
    #[test]
    /// Arbitrary message-shaped JSON is rejected before any terminal line is queued.
    fn malformed_input_never_reaches_sink() {
        let (sender, receiver) = sync_channel(1);
        let relay = Relay {
            sender,
            gate: Mutex::new(RateGate::default()),
            origin: Instant::now(),
            failures: Arc::new(AtomicU64::new(0)),
        };
        assert!(relay.admit(b"{\"message\":\"secret\"}").is_err());
        assert!(receiver.try_recv().is_err());
    }
}
