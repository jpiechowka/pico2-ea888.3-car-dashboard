//! Exercise the exact software functions used by the vendored IRQ driver.
#[path = "../../firmware/vendor/embassy-rp/src/uart/buffered_io.rs"]
mod io;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use core::task::{Context, Poll, Waker};
use std::sync::Arc;
use std::task::Wake;

use embassy_hal_internal::atomic_ring_buffer::RingBuffer;
use embassy_sync::waitqueue::AtomicWaker;

#[derive(Default)]
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }

    fn wake_by_ref(self: &Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }
}
fn ring(bytes: &mut [u8]) -> RingBuffer {
    let ring = RingBuffer::new();
    // Each test keeps storage alive and one reader/writer until the scope ends.
    unsafe { ring.init(bytes.as_mut_ptr(), bytes.len()) };
    ring
}

#[test]
fn irq_claim_allows_same_core_halves_and_rejects_other_cores_without_mutation() {
    for core in 0..=1 {
        let owner = AtomicU8::new(0);
        for _ in 0..3 {
            critical_section::with(|cs| io::claim_irq_core(&owner, core, cs));
            assert_eq!(owner.load(Ordering::Acquire), core + 1);
        }
        for wrong_core in [1 - core, 2, u8::MAX] {
            assert!(
                std::panic::catch_unwind(|| {
                    critical_section::with(|cs| io::claim_irq_core(&owner, wrong_core, cs));
                })
                .is_err()
            );
            assert_eq!(owner.load(Ordering::Acquire), core + 1);
        }
        // HAL releases only after both rings and its local IRQ are disabled.
        owner.store(0, Ordering::Release);
        critical_section::with(|cs| io::claim_irq_core(&owner, 1 - core, cs));
        assert_eq!(owner.load(Ordering::Acquire), 2 - core);
    }
}

#[test]
fn rx_pending_registers_before_rearming_and_no_wakeup_is_lost() {
    let mut bytes = [0; 16];
    let ring = ring(&mut bytes);
    let mut reader = unsafe { ring.reader() };
    let mut writer = unsafe { ring.writer() };
    let errors = AtomicU8::new(0);
    let waker = AtomicWaker::new();
    let count = Arc::new(WakeCount::default());
    let wake = Waker::from(count.clone());
    let cx = Context::from_waker(&wake);
    let mut out = [0; 4];
    assert_eq!(
        io::poll_read(&mut reader, &errors, &waker, &cx, &mut out, || {
            assert!(writer.push_one(42));
            waker.wake();
        }),
        Poll::Pending
    );
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert_eq!(io::try_read(&mut reader, &errors, &mut out, || {}), Poll::Ready(Ok(1)));
    assert_eq!(out[0], 42);
}

#[test]
fn every_fault_position_and_ring_wrap_blocks_prefix_and_post_fault_bytes_until_reset() {
    for offset in 0..32 {
        for fault_at in 0..=12 {
            let mut bytes = [0; 32];
            let ring = ring(&mut bytes);
            let mut reader = unsafe { ring.reader() };
            let mut writer = unsafe { ring.writer() };
            for _ in 0..offset {
                assert!(writer.push_one(0));
            }
            reader.pop_done(offset);
            let errors = AtomicU8::new(0);
            let mut at = 0;
            while errors.load(Ordering::Acquire) == 0 {
                io::fill_rx(&mut writer, &errors, 32, || {
                    let result = if at == fault_at { Err(8) } else { Ok(at as u8) };
                    at += 1;
                    Some(result)
                });
            }
            assert_eq!(at, fault_at + 1);
            assert_eq!(
                io::fill_rx(&mut writer, &errors, 32, || panic!("must not consume after fault")),
                0
            );
            for _ in 0..3 {
                let mut out = [0xA5; 32];
                assert_eq!(
                    io::try_read(&mut reader, &errors, &mut out, || panic!("must not rearm")),
                    Poll::Ready(Err(8))
                );
                assert_eq!(out, [0xA5; 32]);
            }
            // Driver reset drains both ring spans and the hardware FIFO while
            // IRQs are masked. Clearing an error alone is never a boundary.
            while !ring.is_empty() {
                let count = reader.pop_slice().len();
                reader.pop_done(count);
            }
            errors.store(0, Ordering::Release);
            assert!(writer.push_one(99));
            let mut out = [0; 2];
            assert_eq!(io::try_read(&mut reader, &errors, &mut out, || {}), Poll::Ready(Ok(1)));
            assert_eq!(out[0], 99);
        }
    }
}

#[test]
fn fault_during_rearm_discards_the_returned_chunk_and_remains_sticky() {
    let mut bytes = [0; 8];
    let ring = ring(&mut bytes);
    let mut reader = unsafe { ring.reader() };
    let mut writer = unsafe { ring.writer() };
    assert!(writer.push_one(1));
    let errors = AtomicU8::new(0);
    let mut out = [0; 8];
    assert_eq!(
        io::try_read(&mut reader, &errors, &mut out, || io::latch_error(&errors, 1)),
        Poll::Ready(Err(1))
    );
    io::latch_error(&errors, 8);
    assert_eq!(errors.load(Ordering::Acquire), 9);
    assert_eq!(
        io::try_read(&mut reader, &errors, &mut out, || panic!()),
        Poll::Ready(Err(9))
    );
}

#[test]
fn full_tx_ring_and_pending_flush_pend_after_registering_the_waker() {
    let mut bytes = [0; 8];
    let ring = ring(&mut bytes);
    let mut reader = unsafe { ring.reader() };
    let mut writer = unsafe { ring.writer() };
    for byte in 0..8 {
        assert!(writer.push_one(byte));
    }
    let waker = AtomicWaker::new();
    let count = Arc::new(WakeCount::default());
    let wake = Waker::from(count.clone());
    let cx = Context::from_waker(&wake);
    assert_eq!(
        io::poll_write(&mut writer, &waker, &cx, &[9], || {
            reader.pop_done(8);
            waker.wake();
        }),
        Poll::Pending
    );
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert_eq!(io::poll_write(&mut writer, &waker, &cx, &[9], || {}), Poll::Ready(1));
    assert_eq!(
        io::poll_flush(&ring, &waker, &cx, || {
            reader.pop_done(1);
            waker.wake();
        }),
        Poll::Pending
    );
    assert_eq!(count.0.load(Ordering::SeqCst), 2);
    assert_eq!(io::poll_flush(&ring, &waker, &cx, || panic!()), Poll::Ready(()));
}

#[test]
fn tx_drain_crosses_every_ring_wrap_and_honors_fifo_capacity_and_work_budget() {
    for capacity in 1..=64 {
        for offset in 0..capacity {
            for fifo in [0, 1, 4, 32, 64] {
                let mut bytes = vec![0; capacity];
                let ring = ring(&mut bytes);
                let mut reader = unsafe { ring.reader() };
                let mut writer = unsafe { ring.writer() };
                for _ in 0..offset {
                    assert!(writer.push_one(0));
                }
                reader.pop_done(offset);
                for byte in 0..capacity {
                    assert!(writer.push_one(byte as u8));
                }
                let mut got = Vec::new();
                let count = io::drain_tx(&mut reader, 32, |byte| {
                    if got.len() == fifo {
                        return false;
                    }
                    got.push(byte);
                    true
                });
                let expected = capacity.min(fifo).min(32);
                assert_eq!(count, expected);
                assert_eq!(got, (0..expected as u8).collect::<Vec<_>>());
                assert_eq!(ring.available(), capacity - expected);
                io::drain_tx(&mut reader, capacity, |byte| {
                    got.push(byte);
                    true
                });
                assert_eq!(got, (0..capacity as u8).collect::<Vec<_>>());
                assert!(ring.is_empty());
            }
        }
    }
}

#[test]
fn oversized_consume_cannot_corrupt_the_ring_indices() {
    let mut bytes = [0; 8];
    let ring = ring(&mut bytes);
    let mut reader = unsafe { ring.reader() };
    let mut writer = unsafe { ring.writer() };
    assert!(writer.push_one(42));
    for amount in [2, 8, usize::MAX] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| io::consume_rx(&mut reader, amount)));
        assert!(result.is_err());
        assert_eq!(ring.available(), 1);
        assert_eq!(reader.pop_slice(), &[42]);
    }
    io::consume_rx(&mut reader, 1);
    assert!(ring.is_empty());
}

#[test]
fn concurrent_rx_publication_preserves_order_and_stops_at_the_fault() {
    const TOTAL: usize = 100_000;
    let mut bytes = [0; 31];
    let ring = ring(&mut bytes);
    let mut reader = unsafe { ring.reader() };
    let mut writer = unsafe { ring.writer() };
    let errors = AtomicU8::new(0);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let producer = scope.spawn(|| {
            let mut sequence = 0;
            while !stop.load(Ordering::Acquire) && errors.load(Ordering::Acquire) == 0 {
                io::fill_rx(&mut writer, &errors, 32, || {
                    if sequence == TOTAL {
                        return Some(Err(8));
                    }
                    let byte = sequence as u8;
                    sequence += 1;
                    Some(Ok(byte))
                });
                std::thread::yield_now();
            }
            sequence
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut received = Vec::new();
        let fault = loop {
            let mut out = [0; 7];
            match io::try_read(&mut reader, &errors, &mut out, || {}) {
                Poll::Ready(Ok(n)) => received.extend_from_slice(&out[..n]),
                Poll::Ready(Err(error)) => break Some(error),
                Poll::Pending => std::thread::yield_now(),
            }
            if std::time::Instant::now() > deadline {
                break None;
            }
        };
        stop.store(true, Ordering::Release);
        let produced = producer.join().unwrap();
        assert_eq!(fault, Some(8));
        assert_eq!(produced, TOTAL);
        assert!(received.len() <= TOTAL && received.len() >= TOTAL - 38);
        assert!(received.iter().enumerate().all(|(i, byte)| *byte == i as u8));
    });
    assert_eq!(io::fill_rx(&mut writer, &errors, 32, || panic!()), 0);
}
