//! CPU fixture: immediate synthetic adapter replies, with no wire wait time.
use std::task::{Context, Poll};

use dashboard_host_validation::diagnostics::service::{IoError, Port};
use heapless::Deque;

#[derive(Default)]
pub struct ScriptedPort(Deque<u8, 64>);
impl ScriptedPort {
    pub fn new() -> Self { Self(Deque::new()) }

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

    fn poll_flush(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        Poll::Ready(Ok(()))
    }

    fn busy(&self) -> bool { false }

    fn rx_error(&self) -> Option<IoError> { None }

    fn discard_rx(&mut self) -> Result<(), IoError> {
        self.0.clear();
        Ok(())
    }
}
