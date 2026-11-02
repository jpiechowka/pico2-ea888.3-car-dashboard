//! Actual SPI/SDIO WLAN buffer ownership and framing regressions.
extern crate std;

use core::cell::{Cell, RefCell};
use core::convert::Infallible;
use core::future::{Future, poll_fn};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::vec::Vec;

use crate::runner::SealedBus;
use crate::spi::{SpiBus, SpiBusCyw43};

/// Unused power output; WLAN framing must not toggle it.
struct Power;
impl embedded_hal_1::digital::ErrorType for Power {
    /// This no-I/O fixture cannot fail.
    type Error = Infallible;
}
impl embedded_hal_1::digital::OutputPin for Power {
    /// Never returns: panics if framing unexpectedly changes power.
    fn set_low(&mut self) -> Result<(), Infallible> {
        panic!("unexpected power operation")
    }
    /// Never returns: panics if framing unexpectedly changes power.
    fn set_high(&mut self) -> Result<(), Infallible> {
        panic!("unexpected power operation")
    }
}

/// Scripted transaction readiness and observations shared with a borrowed transport.
#[derive(Default)]
struct Trace {
    /// Whole command/payload snapshot from first transport poll.
    words: RefCell<Vec<u32>>,
    /// Count of actual transport polls.
    polls: Cell<usize>,
    /// Stable borrowed transfer address, zero before first poll.
    pointer: Cell<usize>,
    /// Whether the transport returns its fixed status.
    ready: Cell<bool>,
    /// Number of started transport future drops.
    drops: Cell<usize>,
    /// Fresh status-register read count after the cache is empty.
    reads: Cell<usize>,
}
/// Records a single started transaction future release.
struct Release<'a>(
    /// Counter retained until this started command future is released.
    &'a Cell<usize>,
);
impl Drop for Release<'_> {
    /// Increments the retained counter exactly once.
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
/// Fake SPI retaining the exact caller slice through Pending.
struct Spi<'a>(
    /// Observation state outliving every borrowed SPI command.
    &'a Trace,
);
impl SpiBusCyw43 for Spi<'_> {
    /// Records `write`, checks its stable address/bytes and returns fixed status when ready.
    async fn cmd_write(&mut self, write: &[u32]) -> u32 {
        let _release = Release(&self.0.drops);
        poll_fn(|_| {
            if self.0.polls.get() == 0 {
                self.0.pointer.set(write.as_ptr() as usize);
                self.0.words.borrow_mut().extend_from_slice(write);
            }
            assert_eq!(write.as_ptr() as usize, self.0.pointer.get());
            assert_eq!(write, &*self.0.words.borrow());
            self.0.polls.set(self.0.polls.get() + 1);
            if self.0.ready.get() {
                Poll::Ready(0x76543210)
            } else {
                Poll::Pending
            }
        })
        .await
    }
    /// Fills `read` with 0xBAD0 and counts fresh I/O; returns zero as the unused status.
    async fn cmd_read(&mut self, _: u32, read: &mut [u32]) -> u32 {
        self.0.reads.set(self.0.reads.get() + 1);
        read.fill(0xBAD0);
        0
    }
}

/// A production WLAN write borrows scratch instead of embedding another maximum-size payload.
#[test]
fn wlan_write_future_does_not_copy_payload_storage() {
    let trace = Trace::default();
    let mut bus = SpiBus::new(Power, Spi(&trace));
    let mut words = crate::runner::WlanBuffer::new();
    let work = bus.wlan_write(&mut words, 4);
    assert!(core::mem::size_of_val(&work) < 512, "WLAN future embeds payload copy");
}

/// Returns scratch with `len` patterned logical bytes, nonzero padding and a prefix sentinel.
/// Panics above 2048 bytes; zero is permitted for test preparation only.
fn patterned(len: usize) -> crate::runner::WlanBuffer {
    let mut buffer = crate::runner::WlanBuffer::new();
    buffer.words.fill(0x5A5A5A5A);
    buffer.words[0] = 0xDEADBEEF;
    for (index, byte) in crate::util::slice8_mut(buffer.payload_mut())[..len]
        .iter_mut()
        .enumerate()
    {
        *byte = index as u8;
    }
    buffer
}

/// SPI retains the full caller frame through Pending and emits exact command/padding/status semantics.
#[test]
fn spi_wlan_wire_and_pending_owner_match_original_transaction() {
    for (len, command) in [
        (1, 0xE0000004),
        (3, 0xE0000004),
        (4, 0xE0000004),
        (2047, 0xE0000000),
        (2048, 0xE0000000),
    ] {
        let trace = Trace::default();
        let mut bus = SpiBus::new(Power, Spi(&trace));
        let mut buffer = patterned(len);
        let address = buffer.words.as_ptr() as usize;
        let payload = buffer.words[1..].to_vec();
        let mut expected = std::vec![command];
        expected.extend_from_slice(&payload[..len.div_ceil(4)]);
        {
            let mut write = pin!(bus.wlan_write(&mut buffer, len));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(write.as_mut().poll(&mut cx).is_pending());
            assert_eq!(trace.pointer.get(), address);
            assert_eq!(&*trace.words.borrow(), &expected);
            assert!(write.as_mut().poll(&mut cx).is_pending());
            assert_eq!(trace.drops.get(), 0);
            trace.ready.set(true);
            assert!(write.as_mut().poll(&mut cx).is_ready());
        }
        assert_eq!(trace.polls.get(), 3);
        assert_eq!(trace.drops.get(), 1);
        assert_eq!(&buffer.words[1..], &payload);
        assert_eq!(
            embassy_futures::block_on(bus.read32(crate::consts::FUNC_BUS, crate::consts::SPI_STATUS_REGISTER)),
            0x76543210
        );
        assert_eq!(trace.reads.get(), 0);
    }
}

/// Cancellation releases the sole borrowed frame without publishing a fabricated completion status.
#[test]
fn spi_wlan_cancellation_keeps_storage_and_completion_separate() {
    for poll in [false, true] {
        let trace = Trace::default();
        let mut bus = SpiBus::new(Power, Spi(&trace));
        let mut buffer = patterned(3);
        let payload = buffer.words[1..].to_vec();
        {
            let mut write = pin!(bus.wlan_write(&mut buffer, 3));
            if poll {
                assert!(
                    write
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_pending()
                );
            }
        }
        assert_eq!(trace.polls.get(), usize::from(poll));
        assert_eq!(trace.drops.get(), usize::from(poll));
        assert_eq!(&buffer.words[1..], &payload);
        if !poll {
            assert_eq!(buffer.words[0], 0xDEADBEEF);
        }
        assert_eq!(
            embassy_futures::block_on(bus.read32(crate::consts::FUNC_BUS, crate::consts::SPI_STATUS_REGISTER)),
            0xBAD0
        );
        assert_eq!(trace.reads.get(), 1);
    }
}

/// Invalid private payload lengths panic before changing caller bytes or polling the SPI transport.
#[test]
fn spi_wlan_invalid_length_is_inert() {
    for len in [0, 2049, usize::MAX] {
        let trace = Trace::default();
        let mut bus = SpiBus::new(Power, Spi(&trace));
        let mut buffer = patterned(4);
        let before = buffer.words;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            embassy_futures::block_on(bus.wlan_write(&mut buffer, len));
        }));
        assert!(result.is_err());
        assert_eq!(trace.polls.get(), 0);
        assert_eq!(buffer.words, before);
    }
}

/// Recorded SDIO commands, with transport readiness controlling actual suspension.
#[derive(Default)]
struct SdioTrace {
    /// Raw CMD53 arguments and exact logical bytes accepted by the transport.
    calls: RefCell<Vec<(u32, Vec<u8>)>>,
    /// Whether the active command may complete.
    ready: Cell<bool>,
    /// First command's payload address for prefix/alignment checks.
    pointer: Cell<usize>,
    /// Started transport command futures released.
    drops: Cell<usize>,
}
/// Fake SDIO preserving logical byte counts and exposing both transfer modes.
struct Sdio<'a>(
    /// Observation state outliving every borrowed SDIO command.
    &'a SdioTrace,
);
impl Sdio<'_> {
    /// Records `arg`/`bytes` and `address` once; returns unit after readiness without a second command.
    async fn write(&self, arg: u32, bytes: Vec<u8>, address: usize) {
        if self.0.calls.borrow().is_empty() {
            self.0.pointer.set(address);
        }
        self.0.calls.borrow_mut().push((arg, bytes));
        let _release = Release(&self.0.drops);
        poll_fn(|_| {
            if self.0.ready.get() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}
impl crate::sdio::SdioBusCyw43<64> for Sdio<'_> {
    /// Scripted transport cannot fail; only the write paths are supported.
    type Error = Infallible;
    /// Never returns: panics because WLAN framing must not reconfigure the clock.
    fn set_bus_to_high_speed(&mut self, _: u32) -> Result<(), Infallible> {
        panic!("unexpected clock change")
    }
    /// Never returns: panics because WLAN framing must not issue CMD52.
    async fn cmd52(&mut self, _: u32) -> Result<u16, Infallible> {
        panic!("unexpected CMD52")
    }
    /// Never returns: panics because this test exercises transmit only.
    async fn cmd53_block_read(
        &mut self,
        _: u32,
        _: &mut [aligned::Aligned<aligned::A4, [u8; 64]>],
    ) -> Result<(), Infallible> {
        panic!("unexpected read")
    }
    /// Never returns: panics because this test exercises transmit only.
    async fn cmd53_byte_read(&mut self, _: u32, _: &mut aligned::Aligned<aligned::A4, [u8]>) -> Result<(), Infallible> {
        panic!("unexpected read")
    }
    /// Records every byte in `blocks` for `arg`; returns Ok after scripted readiness.
    async fn cmd53_block_write(
        &mut self,
        arg: u32,
        blocks: &[aligned::Aligned<aligned::A4, [u8; 64]>],
    ) -> Result<(), Infallible> {
        let bytes = blocks.iter().flat_map(|block| block.iter()).copied().collect();
        self.write(arg, bytes, blocks.as_ptr() as usize).await;
        Ok(())
    }
    /// Records only logical `bytes` for `arg`; returns after scripted readiness.
    async fn cmd53_byte_write(
        &mut self,
        arg: u32,
        bytes: &aligned::Aligned<aligned::A4, [u8]>,
    ) -> Result<(), Infallible> {
        self.write(arg, bytes.to_vec(), bytes.as_ptr() as usize).await;
        Ok(())
    }
}

/// SDIO never sends the prefix or SPI padding and preserves its existing block/tail arguments.
#[test]
fn sdio_wlan_skips_prefix_and_preserves_exact_logical_lengths() {
    for (len, arguments) in [
        (1, std::vec![0xA4000001]),
        (3, std::vec![0xA4000003]),
        (4, std::vec![0xA4000004]),
        (63, std::vec![0xA400003F]),
        (64, std::vec![0xAC000001]),
        (65, std::vec![0xAC000001, 0xA4008001]),
        (2047, std::vec![0xAC00001F, 0xA40F803F]),
        (2048, std::vec![0xAC000020]),
    ] {
        let trace = SdioTrace::default();
        trace.ready.set(true);
        let mut bus = crate::sdio::SdioBus::new(Sdio(&trace));
        let mut buffer = patterned(len);
        let before = buffer.words;
        let address = buffer.words[1..].as_ptr() as usize;
        let expected = buffer.payload(len).to_vec();
        embassy_futures::block_on(bus.wlan_write(&mut buffer, len));
        let calls = trace.calls.borrow();
        assert_eq!(calls.iter().map(|call| call.0).collect::<Vec<_>>(), arguments);
        assert_eq!(
            calls.iter().flat_map(|call| call.1.iter()).copied().collect::<Vec<_>>(),
            expected
        );
        assert_eq!(trace.pointer.get(), address);
        assert_eq!(address % 4, 0);
        assert_eq!(trace.drops.get(), calls.len());
        assert_eq!(buffer.words, before);
    }
}

/// Invalid lengths and canceled block writes cannot submit a later tail or mutate the retained buffer.
#[test]
fn sdio_wlan_invalid_and_canceled_work_preserves_owner() {
    for len in [0, 2049, usize::MAX] {
        let trace = SdioTrace::default();
        let mut bus = crate::sdio::SdioBus::new(Sdio(&trace));
        let mut buffer = patterned(4);
        let before = buffer.words;
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                embassy_futures::block_on(bus.wlan_write(&mut buffer, len));
            }))
            .is_err()
        );
        assert!(trace.calls.borrow().is_empty());
        assert_eq!(buffer.words, before);
    }
    for poll in [false, true] {
        let trace = SdioTrace::default();
        let mut bus = crate::sdio::SdioBus::new(Sdio(&trace));
        let mut buffer = patterned(65);
        let before = buffer.words;
        {
            let mut write = pin!(bus.wlan_write(&mut buffer, 65));
            if poll {
                assert!(
                    write
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_pending()
                );
            }
        }
        assert_eq!(trace.calls.borrow().len(), usize::from(poll));
        assert_eq!(trace.drops.get(), usize::from(poll));
        assert_eq!(buffer.words, before);
    }
}

/// Shared and exclusive aligned views retain padded parent provenance while exposing only logical bytes.
#[test]
fn aligned_ranges_preserve_parent_extent_and_logical_bounds() {
    use crate::util::{aligned_range, aligned_range_mut};
    for len in [0, 1, 3, 4, 63, 64, 65, 68] {
        let mut backing = aligned::Aligned::<aligned::A4, _>([0xa5u8; 68]);
        let view = aligned_range(&backing, 0..len);
        assert_eq!(view.len(), len);
        assert_eq!(core::mem::size_of_val(view), len.div_ceil(4) * 4);
        assert!(view.iter().all(|byte| *byte == 0xa5));
        aligned_range_mut(&mut backing, 0..len).fill(0x42);
        assert!(backing[..len].iter().all(|byte| *byte == 0x42));
        assert!(backing[len..].iter().all(|byte| *byte == 0xa5));
    }
    // The parent has real layout padding, not extra logical bytes. Neither view reads it.
    let mut padded = aligned::Aligned::<aligned::A4, _>([0x23u8; 65]);
    let parent = aligned_range(&padded, 0..65);
    assert_eq!(aligned_range(parent, 64..65).len(), 1);
    assert_eq!(aligned_range(parent, 64..64).len(), 0);
    let parent = aligned_range_mut(&mut padded, 0..65);
    aligned_range_mut(parent, 64..65).fill(0x42);
    assert_eq!(padded[64], 0x42);
    assert!(padded[..64].iter().all(|byte| *byte == 0x23));
}

/// Bad alignment/order/extent is rejected before pointer arithmetic or caller mutation.
#[test]
fn aligned_ranges_reject_invalid_bounds_without_mutation() {
    use crate::util::{aligned_range, aligned_range_mut};
    for (start, end) in [
        (1, 4),
        (3, 3),
        (8, 4),
        (0, 69),
        (68, 69),
        (0, usize::MAX),
        (usize::MAX, usize::MAX),
    ] {
        let range = start..end;
        let mut backing = aligned::Aligned::<aligned::A4, _>([0xa5u8; 68]);
        assert!(std::panic::catch_unwind(|| aligned_range(&backing, range.clone())).is_err());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                aligned_range_mut(&mut backing, range.clone()).fill(0x42);
            }))
            .is_err()
        );
        assert!(backing.iter().all(|byte| *byte == 0xa5));
    }
}
