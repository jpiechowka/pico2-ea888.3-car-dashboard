//! Optional browser context sink. Admission never waits for terminal output.
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use dashboard_helper_protocol::diagnostics::*;
use dashboard_helper_protocol::{Error, decode};

use crate::jobs::logs::StreamFramer;
const MAILBOX: usize = 32;
struct Line {
    severity: Severity,
    text: String,
}
pub struct Relay {
    sender: SyncSender<Line>,
    gate: Mutex<RateGate>,
    origin: Instant,
    failures: Arc<AtomicU64>,
}
impl Default for Relay {
    fn default() -> Self { Self::new() }
}
impl Relay {
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

    pub fn failed_writes(&self) -> u64 { self.failures.load(Ordering::Relaxed) }

    /// Acknowledges bounded mailbox admission, not terminal display or job evidence.
    pub fn admit(
        &self,
        bytes: &[u8],
    ) -> Result<Response, Error> {
        self.admit_at(
            bytes,
            self.origin.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        )
    }

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
mod tests {
    use dashboard_helper_protocol::{BoundedVec, Text, encode};

    use super::*;
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
