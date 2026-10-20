//! Internal buffered UART software operations, shared with host regression tests.
//! Reader/writer ownership is supplied by the enclosing single-owner driver.
#![allow(missing_docs)]
use core::sync::atomic::{AtomicU8, Ordering};
use core::task::{Context, Poll};

use embassy_hal_internal::atomic_ring_buffer::{Reader, RingBuffer, Writer};
use embassy_sync::waitqueue::AtomicWaker;

/// Claim a UART IRQ on exactly one core while either buffered half is alive.
/// The caller holds the same critical section across ring initialization and
/// interrupt enable; Drop releases the claim only after disabling the last IRQ.
pub fn claim_irq_core(
    owner: &AtomicU8,
    core: u8,
    _cs: critical_section::CriticalSection<'_>,
) {
    assert!(core <= 1, "invalid UART IRQ core");
    let claim = core + 1;
    let previous = owner.load(Ordering::Relaxed);
    assert!(
        previous == 0 || previous == claim,
        "UART IRQ already owned by another core"
    );
    owner.store(claim, Ordering::Release);
}

pub fn latch_error(
    errors: &AtomicU8,
    bits: u8,
) {
    critical_section::with(|_| {
        errors.store(errors.load(Ordering::Relaxed) | bits, Ordering::Release);
    });
}

pub fn consume_rx(
    reader: &mut Reader<'_>,
    amount: usize,
) {
    assert!(amount <= reader.pop_slice().len(), "consume exceeds contiguous RX data");
    reader.pop_done(amount);
}

pub fn try_read(
    reader: &mut Reader<'_>,
    errors: &AtomicU8,
    out: &mut [u8],
    rearm: impl FnOnce(),
) -> Poll<Result<usize, u8>> {
    if out.is_empty() {
        return Poll::Ready(Ok(0));
    }
    let error = errors.load(Ordering::Acquire);
    if error != 0 {
        return Poll::Ready(Err(error));
    }
    let n = reader.pop(|data| {
        let n = data.len().min(out.len());
        out[..n].copy_from_slice(&data[..n]);
        n
    });
    // Rearm even on Pending. The caller atomically rechecks the sticky fault
    // before enabling interrupts, so a concurrently latched fault stays closed.
    rearm();
    let error = errors.load(Ordering::Acquire);
    if error != 0 {
        Poll::Ready(Err(error))
    } else if n == 0 {
        Poll::Pending
    } else {
        Poll::Ready(Ok(n))
    }
}

pub fn poll_read(
    reader: &mut Reader<'_>,
    errors: &AtomicU8,
    waker: &AtomicWaker,
    cx: &Context<'_>,
    out: &mut [u8],
    rearm: impl FnOnce(),
) -> Poll<Result<usize, u8>> {
    waker.register(cx.waker());
    try_read(reader, errors, out, rearm)
}

pub fn poll_write(
    writer: &mut Writer<'_>,
    waker: &AtomicWaker,
    cx: &Context<'_>,
    input: &[u8],
    pend: impl FnOnce(),
) -> Poll<usize> {
    if input.is_empty() {
        return Poll::Ready(0);
    }
    waker.register(cx.waker());
    try_write(writer, input, pend)
}

pub fn try_write(
    writer: &mut Writer<'_>,
    input: &[u8],
    pend: impl FnOnce(),
) -> Poll<usize> {
    if input.is_empty() {
        return Poll::Ready(0);
    }
    let n = writer.push(|data| {
        let n = data.len().min(input.len());
        data[..n].copy_from_slice(&input[..n]);
        n
    });
    pend(); // A full ring needs a manual drain kick too.
    if n == 0 { Poll::Pending } else { Poll::Ready(n) }
}

pub fn poll_flush(
    ring: &RingBuffer,
    waker: &AtomicWaker,
    cx: &Context<'_>,
    pend: impl FnOnce(),
) -> Poll<()> {
    waker.register(cx.waker());
    if ring.is_empty() {
        Poll::Ready(())
    } else {
        pend();
        Poll::Pending
    }
}

pub fn drain_tx(
    reader: &mut Reader<'_>,
    budget: usize,
    mut put: impl FnMut(u8) -> bool,
) -> usize {
    let mut total = 0;
    while total < budget {
        let mut written = 0;
        for &byte in reader.pop_slice().iter().take(budget - total) {
            if !put(byte) {
                break;
            }
            written += 1;
        }
        if written == 0 {
            break;
        }
        reader.pop_done(written);
        total += written;
    }
    total
}

pub fn fill_rx(
    writer: &mut Writer<'_>,
    errors: &AtomicU8,
    budget: usize,
    mut get: impl FnMut() -> Option<Result<u8, u8>>,
) -> usize {
    if errors.load(Ordering::Acquire) != 0 {
        return 0;
    }
    let mut count = 0;
    for slot in writer.push_slice().iter_mut().take(budget) {
        match get() {
            None => break,
            Some(Err(error)) => {
                latch_error(errors, error);
                break;
            }
            Some(Ok(byte)) => {
                *slot = byte;
                count += 1;
            }
        }
    }
    if count != 0 {
        writer.push_done(count);
    }
    count
}

#[cfg(test)]
mod tests {
    #[test]
    fn nonblocking_write_preserves_full_ring_and_recovers_after_drain() {
        use super::*;
        let mut bytes = [0u8; 2];
        let ring = RingBuffer::new();
        unsafe { ring.init(bytes.as_mut_ptr(), bytes.len()) };
        let mut writer = unsafe { ring.writer() };
        let mut kicks = 0;
        assert_eq!(try_write(&mut writer, &[1, 2, 3], || kicks += 1), Poll::Ready(2));
        assert_eq!(try_write(&mut writer, &[3], || kicks += 1), Poll::Pending);
        assert_eq!(try_write(&mut writer, &[], || kicks += 1), Poll::Ready(0));
        assert_eq!(kicks, 2);
        let mut reader = unsafe { ring.reader() };
        assert_eq!(reader.pop_slice(), &[1, 2]);
        reader.pop_done(2);
        assert_eq!(try_write(&mut writer, &[3], || kicks += 1), Poll::Ready(1));
        assert_eq!(reader.pop_slice(), &[3]);
        reader.pop_done(1);
        unsafe { ring.deinit() };
    }
}
