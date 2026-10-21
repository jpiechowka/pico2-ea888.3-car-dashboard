//! Fixed zero-on-allocation packet storage; the default pool is global and uses a critical-section mutex.
//! No priority, reservation or waiting policy is implemented: exhaustion returns None.
use core::cell::{RefCell, UnsafeCell};

use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, RawMutex};
use embassy_sync::blocking_mutex::Mutex;

use crate::{config, Packet, PacketPool};

/// One independently mutable MTU-byte allocation, excluded from bookkeeping RefCell borrows.
struct PacketBuf<const MTU: usize> {
    /// Raw byte access is permitted only while this slot is exclusively claimed in State.
    buf: UnsafeCell<[u8; MTU]>,
}

impl<const MTU: usize> PacketBuf<MTU> {
    /// Returns zero-filled backing bytes; allocation status lives separately in State.
    pub(crate) const fn new() -> Self {
        Self {
            buf: UnsafeCell::new([0; MTU]),
        }
    }
}

/// Allocation flags only; mutable borrows never cover bytes leased to live packets.
struct State<const N: usize> {
    /// True permits allocation; indices are searched in ascending order.
    free: [bool; N],
}

impl<const N: usize> State<N> {
    /// Returns N entirely available slot flags.
    pub(crate) const fn new() -> Self {
        Self { free: [true; N] }
    }

    /// Claims and returns the first free index, or None without mutation on exhaustion.
    fn alloc(&mut self) -> Option<usize> {
        let index = self.free.iter().position(|free| *free)?;
        self.free[index] = false;
        Some(index)
    }

    /// Releases claimed same-pool `index`; caller must end every byte view and prevent duplicate release.
    /// An out-of-range index panics.
    fn free(&mut self, index: usize) {
        self.free[index] = true;
    }

    /// Returns the number of free flags at this locked snapshot.
    fn available(&self) -> usize {
        self.free.iter().filter(|free| **free).count()
    }
}

/// Fixed N-slot allocator with MTU-byte storage and exclusion supplied by M.
/// Only the global DefaultPacketPool implements PacketPool; local values expose no public allocation API.
pub struct StaticPacketPool<M: RawMutex, const MTU: usize, const N: usize> {
    /// Allocation flags only; RefCell rejects reentrant bookkeeping mutation within the lock.
    state: Mutex<M, RefCell<State<N>>>,
    /// Stationary backing cells, never borrowed through a mutable State reference while leased.
    packets: [PacketBuf<MTU>; N],
}

// SAFETY: A Sync raw mutex excludes concurrent allocation-flag access. Only a claimed slot's
// unique PacketRef may access its UnsafeCell bytes; release follows the last byte borrow.
// No public API can allocate from a movable local pool: DefaultPacketPool uses a static pool.
// Internal raw allocation callers must uphold the documented stationary/affine lease contract.
unsafe impl<M: RawMutex + Sync, const MTU: usize, const N: usize> Sync for StaticPacketPool<M, MTU, N> {}

impl<M: RawMutex, const MTU: usize, const N: usize> Default for StaticPacketPool<M, MTU, N> {
    /// Returns a new independent, entirely free pool value.
    fn default() -> Self {
        Self::new()
    }
}

impl<M: RawMutex, const MTU: usize, const N: usize> StaticPacketPool<M, MTU, N> {
    /// Returns N zero-filled slots; this allocator has no quality-of-service policy.
    const fn new() -> Self {
        Self {
            state: Mutex::new(RefCell::new(State::new())),
            packets: [const { PacketBuf::new() }; N],
        }
    }

    /// Returns a zeroed slot under M exclusion, or None without waiting; keep the pool stationary until release.
    fn alloc(&self) -> Option<PacketRef<MTU>> {
        self.state.lock(|state| {
            let index = state.borrow_mut().alloc()?;
            let buf = self.packets[index].buf.get().cast::<u8>();
            // SAFETY: The flag was free and is now claimed under M. No live packet can
            // refer to this slot; only this allocation writes its complete initialized region.
            unsafe { core::ptr::write_bytes(buf, 0, MTU) };
            Some(PacketRef { idx: index, buf })
        })
    }

    /// Returns live same-pool `p_ref` under M exclusion; caller relinquishes all access to that slot.
    fn free(&self, p_ref: &PacketRef<MTU>) {
        self.state.lock(|state| {
            let mut state = state.borrow_mut();
            state.free(p_ref.idx);
        });
    }

    /// Returns the free-slot count under M exclusion; subsequent allocations may change it.
    fn available(&self) -> usize {
        self.state.lock(|state| state.borrow().available())
    }
}

/// Internal affine slot identity and pointer; only DefaultPacket supplies byte views and automatic release.
/// Does not carry a lifetime or release itself; private callers must keep the source pool stationary and alive.
#[repr(C)]
pub struct PacketRef<const MTU: usize> {
    /// Slot index within the originating pool; never a byte offset.
    idx: usize,
    /// Pointer to the first of MTU initialized slot bytes.
    buf: *mut u8,
}

/// Critical-section-protected pool type using build-time MTU and slot-count constants.
pub type DefaultPacketPool = StaticPacketPool<
    CriticalSectionRawMutex,
    { config::DEFAULT_PACKET_POOL_MTU },
    { config::DEFAULT_PACKET_POOL_SIZE },
>;

/// Stable-address storage shared by every DefaultPacketPool allocation.
static DEFAULT_POOL: StaticPacketPool<
    CriticalSectionRawMutex,
    { config::DEFAULT_PACKET_POOL_MTU },
    { config::DEFAULT_PACKET_POOL_SIZE },
> = StaticPacketPool::new();

impl PacketPool for DefaultPacketPool {
    /// Affine packet owner that returns its slot on drop.
    type Packet = DefaultPacket;
    /// Full byte capacity of each allocation, including any protocol headers.
    const MTU: usize = { config::DEFAULT_PACKET_POOL_MTU };
    /// Returns total slots, not the number currently free.
    fn capacity() -> usize {
        config::DEFAULT_PACKET_POOL_SIZE
    }

    /// Returns a zeroed global packet or None immediately on exhaustion.
    fn allocate() -> Option<DefaultPacket> {
        DEFAULT_POOL.alloc().map(|p| DefaultPacket {
            p_ref: p,
            pool: &DEFAULT_POOL,
        })
    }
}

/// Exclusive global slot owner; byte views cover the entire configured MTU, not a logical packet length.
pub struct DefaultPacket {
    /// Unique live slot in pool, inaccessible to external construction or cloning.
    p_ref: PacketRef<{ config::DEFAULT_PACKET_POOL_MTU }>,
    /// Stable backing allocator that outlives every packet and receives the slot on drop.
    pool: &'static DefaultPacketPool,
}

impl Packet for DefaultPacket {}
impl AsRef<[u8]> for DefaultPacket {
    /// Returns all initialized slot bytes, borrowing this owner so drop cannot free them.
    fn as_ref(&self) -> &[u8] {
        // SAFETY: This affine owner retains the claimed static slot; &self excludes
        // mutation through this packet for the returned borrow. Other slots are disjoint.
        unsafe { core::slice::from_raw_parts(self.p_ref.buf, config::DEFAULT_PACKET_POOL_MTU) }
    }
}

impl AsMut<[u8]> for DefaultPacket {
    /// Returns exclusive access to all slot bytes for this borrow.
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: This affine owner is the sole claimant; &mut self excludes every other
        // byte view of this slot. Pool bookkeeping borrows no backing bytes.
        unsafe { core::slice::from_raw_parts_mut(self.p_ref.buf, config::DEFAULT_PACKET_POOL_MTU) }
    }
}

impl Drop for DefaultPacket {
    /// Releases exactly this slot; bytes are erased only on its next allocation.
    fn drop(&mut self) {
        self.pool.free(&self.p_ref);
    }
}

#[cfg(test)]
/// Exhaustion and slot-reuse checks independent of the shared global pool.
mod tests {
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;

    use super::*;

    #[test]
    /// All eight slots are usable; the ninth allocation returns None without waiting.
    fn test_none_qos() {
        let pool: StaticPacketPool<NoopRawMutex, 27, 8> = StaticPacketPool::new();

        let a1 = pool.alloc();
        assert!(a1.is_some());
        let a2 = pool.alloc();
        assert!(a2.is_some());
        let a3 = pool.alloc();
        assert!(a3.is_some());
        let a4 = pool.alloc();
        assert!(a4.is_some());
        let a5 = pool.alloc();
        assert!(a5.is_some());
        let a6 = pool.alloc();
        assert!(a6.is_some());
        let a7 = pool.alloc();
        assert!(a7.is_some());

        let b1 = pool.alloc();
        assert!(b1.is_some());

        let b2 = pool.alloc();
        assert!(b2.is_none());
    }
    /// Release restores availability; reacquiring the same slot zeroes stale payload bytes.
    #[test]
    fn released_slot_is_zeroed_before_reuse() {
        let pool: StaticPacketPool<NoopRawMutex, 4, 1> = StaticPacketPool::new();
        let first = pool.alloc().unwrap();
        assert_eq!(pool.available(), 0);
        // This test owns the sole live slot and keeps the pool stationary.
        unsafe {
            core::slice::from_raw_parts_mut(first.buf, 4).fill(0xa5);
        }
        pool.free(&first);
        assert_eq!(pool.available(), 1);
        let second = pool.alloc().unwrap();
        assert_eq!(second.idx, first.idx);
        assert_eq!(second.buf, first.buf);
        // No reference from the released allocation is reused.
        assert_eq!(unsafe { core::slice::from_raw_parts(second.buf, 4) }, &[0; 4]);
        pool.free(&second);
        assert_eq!(pool.available(), 1);
    }
    /// Allocating, inspecting availability and freeing another packet preserve a live safe byte view.
    /// Run under Miri to validate raw-pointer provenance, not merely native byte equality.
    #[test]
    fn live_packet_survives_other_pool_operations() {
        let mut first = DefaultPacketPool::allocate().unwrap();
        first.as_mut().fill(0xa5);
        let mut second = DefaultPacketPool::allocate().unwrap();
        second.as_mut().fill(0x5a);
        assert!(first.as_ref().iter().all(|byte| *byte == 0xa5));
        let _ = DEFAULT_POOL.available();
        assert!(first.as_ref().iter().all(|byte| *byte == 0xa5));
        drop(second);
        assert!(first.as_ref().iter().all(|byte| *byte == 0xa5));
        first.as_mut()[0] = 0x42;
        assert_eq!(first.as_ref()[0], 0x42);
    }

    /// A live mutable packet slice remains valid while another slot is allocated, mutated and freed.
    #[test]
    fn live_slice_is_disjoint_from_other_slot_bookkeeping() {
        let mut first = DefaultPacketPool::allocate().unwrap();
        let bytes = first.as_mut();
        bytes.fill(0x31);
        let mut second = DefaultPacketPool::allocate().unwrap();
        second.as_mut().fill(0x72);
        assert!(bytes.iter().all(|byte| *byte == 0x31));
        drop(second);
        let _ = DEFAULT_POOL.available();
        bytes[0] = 0x90;
        assert_eq!(first.as_ref()[0], 0x90);
    }
    /// Zero slots exhaust immediately; zero-byte slots still have distinct allocation ownership.
    #[test]
    fn empty_pool_and_zero_byte_slots_have_bounded_ownership() {
        let empty: StaticPacketPool<NoopRawMutex, 4, 0> = StaticPacketPool::new();
        assert!(empty.alloc().is_none());
        assert_eq!(empty.available(), 0);
        let zero: StaticPacketPool<NoopRawMutex, 0, 2> = StaticPacketPool::new();
        let first = zero.alloc().unwrap();
        let second = zero.alloc().unwrap();
        assert_ne!(first.idx, second.idx);
        assert!(zero.alloc().is_none());
        zero.free(&first);
        zero.free(&second);
        assert_eq!(zero.available(), 2);
    }
}
