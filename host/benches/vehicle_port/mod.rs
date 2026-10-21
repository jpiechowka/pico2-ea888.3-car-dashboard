//! CPU fixture: immediate synthetic adapter replies, with no wire wait time.
use std::task::{Context, Poll};

use dashboard_host_validation::diagnostics::service::{IoError, Port};
use heapless::Deque;

/// Immediate command-response adapter model with bounded queued RX bytes and no hardware I/O.
#[derive(Default)]
pub struct ScriptedPort(
    /// Up to 64 unread fixture response bytes, consumed FIFO.
    Deque<u8, 64>,
);
impl ScriptedPort {
    /// Returns a port with no pending response bytes.
    pub fn new() -> Self { Self(Deque::new()) }

    /// Appends `bytes` to the RX fixture queue; capacity exhaustion panics after any accepted prefix.
    fn reply(
        &mut self,
        bytes: &[u8],
    ) {
        for &byte in bytes {
            self.0.push_back(byte).unwrap();
        }
    }
}
impl Port for ScriptedPort {
    /// Copies queued response bytes into `bytes` and returns Ready(Ok(count)), or Pending when empty.
    /// Does not register a waker; this immediate fixture relies on its driver's explicit polling loop.
    fn poll_read(
        &mut self,
        _: &mut Context<'_>,
        bytes: &mut [u8],
    ) -> Poll<Result<usize, IoError>> {
        if self.0.is_empty() {
            return Poll::Pending;
        }
        let n = bytes.len().min(self.0.len());
        for byte in &mut bytes[..n] {
            *byte = self.0.pop_front().unwrap();
        }
        Poll::Ready(Ok(n))
    }

    /// Matches complete command `bytes`, queues its fixed response and returns Ready(Ok(bytes.len())).
    /// Panics for unsupported/partial commands or response queue overflow; no transport backpressure is modeled.
    fn poll_write(
        &mut self,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        match bytes {
            b"+++" => self.reply(b"ENTER INTO SETTING MODE\r\n"),
            b"AT\r\n" => self.reply(b"OK\r\n"),
            b"AT+Q\r\n" => self.reply(b"OK\r\nENTER DATA MODE\r\n"),
            [0, 0, 7, 0xE0, 0, 0, 2, 1, 0, 0, 0, 0, 0, 0] => self.reply(&[0, 0, 7, 0xE8, 6, 0x41, 0, 8, 0, 0, 0, 0]),
            [0, 0, 7, 0xE0, 0, 0, 2, 1, 5, 0, 0, 0, 0, 0] => self.reply(&[0, 0, 7, 0xE8, 3, 0x41, 5, 135, 0, 0, 0, 0]),
            _ => panic!("unexpected scripted write"),
        }
        Poll::Ready(Ok(bytes.len()))
    }

    /// Returns Ready(Ok(())) immediately because the fixture has no pending hardware transmitter.
    fn poll_flush(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        Poll::Ready(Ok(()))
    }

    /// Returns false; the fixture has no physical FIFO/shifter activity.
    fn busy(&self) -> bool { false }

    /// Returns None; UART faults are outside this CPU benchmark fixture.
    fn rx_error(&self) -> Option<IoError> { None }

    /// Clears all queued synthetic response bytes and returns Ok.
    fn discard_rx(&mut self) -> Result<(), IoError> {
        self.0.clear();
        Ok(())
    }
}
