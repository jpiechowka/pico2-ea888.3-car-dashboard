//! RP2040/RP235x full-speed USB device driver with single-buffered DPRAM endpoints.
//!
//! USB clock must already be configured. VBUS is forced present rather than sensed;
//! bus polling emits synthetic PowerDetected initially and after disable, with no PowerRemoved. Endpoint copies
//! use compiler fences and hardware AVAILABLE ownership; they are not peer acknowledgments.
//! The [driver contract](https://docs.rs/embassy-usb-driver/0.2.2/embassy_usb_driver/)
//! defines setup/data/status ordering and cancellation. Driver drop does not disable USB.
//! Abort ordering follows the [RP2350 register contract](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/usb.h)
//! and [TinyUSB RP controller implementation](https://github.com/hathach/tinyusb/blob/master/src/portable/raspberrypi/rp2040/dcd_rp2040.c).
//! Fake-register tests cover software ownership, not physical enumeration or abort latency.
use core::future::poll_fn;
use core::marker::PhantomData;
use core::slice;
use core::sync::atomic::{AtomicU32, Ordering, compiler_fence};
use core::task::Poll;

use embassy_hal_internal::PeripheralType;
use embassy_sync::waitqueue::AtomicWaker;
use embassy_usb_driver as driver;
use embassy_usb_driver::{
    Direction,
    EndpointAddress,
    EndpointAllocError,
    EndpointError,
    EndpointInfo,
    EndpointType,
    Event,
    Unsupported,
};

use crate::interrupt::typelevel::{Binding, Interrupt};
use crate::{Peri, RegExt, interrupt, pac, peripherals};

/// Binds the singleton to USB registers and its dedicated packet RAM.
trait SealedInstance {
    /// Returns the USB controller register bank.
    fn regs() -> crate::pac::usb::Usb;
    /// Returns the USB packet-RAM control register bank.
    fn dpram() -> crate::pac::usb_dpram::UsbDpram;
    /// Returns whether this silicon supports reliable endpoint abort (RP2040 B2 or RP235x).
    fn abort_supported() -> bool;
    /// Supplies fake completion for requested `_mask`; absent from production builds.
    #[cfg(test)]
    fn abort_requested(_mask: u32) {}
}

/// USB peripheral instance.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType + 'static {
    /// Interrupt for this peripheral.
    type Interrupt: interrupt::typelevel::Interrupt;
}

impl crate::usb::SealedInstance for peripherals::USB {
    /// Returns the USB controller register bank.
    fn regs() -> pac::usb::Usb { pac::USB }

    /// Returns the USB packet-RAM control register bank.
    fn dpram() -> crate::pac::usb_dpram::UsbDpram { pac::USB_DPRAM }

    /// Returns false on RP2040 B0/B1, whose E2 erratum prevents reliable buffer revocation.
    fn abort_supported() -> bool {
        #[cfg(feature = "rp2040")]
        {
            pac::SYSINFO.chip_id().read().revision() >= 2
        }
        #[cfg(feature = "_rp235x")]
        {
            true
        }
    }
}

impl crate::usb::Instance for peripherals::USB {
    /// Shared USB controller IRQ.
    type Interrupt = crate::interrupt::typelevel::USBCTRL_IRQ;
}

/// Endpoint indices 0..15 per direction, including control endpoint 0.
const EP_COUNT: usize = 16;
/// Total USB DPRAM capacity in bytes, including control metadata.
const EP_MEMORY_SIZE: usize = 4096;
/// Base of hardware-owned USB DPRAM; accesses require buffer ownership.
const EP_MEMORY: *mut u8 = pac::USB_DPRAM.as_ptr() as *mut u8;

/// Single bus-event task waiter.
static BUS_WAKER: AtomicWaker = AtomicWaker::new();
/// One device-to-host waiter per endpoint index.
static EP_IN_WAKERS: [AtomicWaker; EP_COUNT] = [const { AtomicWaker::new() }; EP_COUNT];
/// One host-to-device waiter per endpoint index.
static EP_OUT_WAKERS: [AtomicWaker; EP_COUNT] = [const { AtomicWaker::new() }; EP_COUNT];

/// Per-direction endpoint identities; MAX is exhausted until exclusive driver reconstruction.
static EP_IN_GENERATIONS: [AtomicU32; EP_COUNT] = [const { AtomicU32::new(0) }; EP_COUNT];
/// OUT identities, independent of IN so changing one direction cannot cancel the other.
static EP_OUT_GENERATIONS: [AtomicU32; EP_COUNT] = [const { AtomicU32::new(0) }; EP_COUNT];

/// Invalidates `generation` without wrapping; `_cs` serializes load/store on ARMv6-M too.
fn invalidate(
    generation: &AtomicU32,
    _cs: critical_section::CriticalSection<'_>,
) {
    generation.store(generation.load(Ordering::Relaxed).saturating_add(1), Ordering::Release);
}

/// Maximum register reads per abort; a work bound, not a measured wall-clock deadline.
const ABORT_POLL_LIMIT: usize = 65_536;

/// Revokes `index`/`direction` under `_cs`; returns true only when its buffer can be reused.
/// Data enable is cleared first. Unsupported abort or timeout preserves buffer contents,
/// retains ABORT and marks that direction exhausted until exclusive driver reconstruction.
/// At most ABORT_POLL_LIMIT reads occur; callers must not treat timeout as completion.
fn quiesce_endpoint<T: Instance>(
    index: usize,
    direction: Direction,
    _cs: critical_section::CriticalSection<'_>,
) -> bool {
    let (generation, ctrl, mask) = match direction {
        Direction::In => {
            if index != 0 {
                T::dpram().ep_in_control(index - 1).modify(|w| w.set_enable(false));
            }
            (
                &EP_IN_GENERATIONS[index],
                T::dpram().ep_in_buffer_control(index),
                1u32 << (index * 2),
            )
        }
        Direction::Out => {
            if index != 0 {
                T::dpram().ep_out_control(index - 1).modify(|w| w.set_enable(false));
            }
            (
                &EP_OUT_GENERATIONS[index],
                T::dpram().ep_out_buffer_control(index),
                2u32 << (index * 2),
            )
        }
    };
    if generation.load(Ordering::Acquire) == u32::MAX {
        return false;
    }
    if !ctrl.read().available(0) {
        return true;
    }
    let regs = T::regs();
    // DONE is write-one-to-clear; discard any acknowledgement from an older abort.
    regs.ep_abort_done().write(|w| w.0 = mask);
    regs.ep_abort().modify(|w| w.0 |= mask);
    #[cfg(test)]
    T::abort_requested(mask);
    if T::abort_supported() {
        for _ in 0..ABORT_POLL_LIMIT {
            if regs.ep_abort_done().read().0 & mask != 0 {
                ctrl.write(|_| {});
                regs.buff_status().write(|w| w.0 = mask);
                regs.ep_abort_done().write(|w| w.0 = mask);
                regs.ep_abort().modify(|w| w.0 &= !mask);
                return true;
            }
        }
    }
    generation.store(u32::MAX, Ordering::Release);
    match direction {
        Direction::In => EP_IN_WAKERS[index].wake(),
        Direction::Out => EP_OUT_WAKERS[index].wake(),
    }
    false
}

/// Invalidates every endpoint, disables data endpoints and clears buffer ownership.
/// Caller holds `cs`; hardware reset or completed abort must already stop old traffic.
/// Quarantined directions keep their old buffer control and exhausted identities.
fn reset_endpoints<T: Instance>(cs: critical_section::CriticalSection<'_>) {
    for i in 0..EP_COUNT {
        invalidate(&EP_IN_GENERATIONS[i], cs);
        invalidate(&EP_OUT_GENERATIONS[i], cs);
        if i != 0 {
            T::dpram().ep_in_control(i - 1).modify(|w| w.set_enable(false));
            T::dpram().ep_out_control(i - 1).modify(|w| w.set_enable(false));
        }
        if EP_IN_GENERATIONS[i].load(Ordering::Acquire) != u32::MAX {
            T::dpram().ep_in_buffer_control(i).write(|_| {});
        }
        if EP_OUT_GENERATIONS[i].load(Ordering::Acquire) != u32::MAX {
            T::dpram().ep_out_buffer_control(i).write(|_| {});
        }
        EP_IN_WAKERS[i].wake();
        EP_OUT_WAKERS[i].wake();
    }
}

/// Returns success only while data endpoint `index`/`direction` retains `generation` and enable.
/// Reset, disable and exhausted identities return Disabled even after a subsequent re-enable.
fn endpoint_valid<T: Instance>(
    index: usize,
    direction: Direction,
    generation: u32,
) -> Result<(), EndpointError> {
    let (current, enabled) = match direction {
        Direction::In => {
            (
                &EP_IN_GENERATIONS[index],
                T::dpram().ep_in_control(index - 1).read().enable(),
            )
        }
        Direction::Out => {
            (
                &EP_OUT_GENERATIONS[index],
                T::dpram().ep_out_control(index - 1).read().enable(),
            )
        }
    };
    if generation == u32::MAX
        || current.load(Ordering::Acquire) != generation
        || !enabled
        || !T::regs().main_ctrl().read().controller_en()
        || T::regs().sie_status().read().bus_reset()
    {
        Err(EndpointError::Disabled)
    } else {
        Ok(())
    }
}

/// Exclusive software view of an allocated DPRAM range while hardware is not using it.
struct EndpointBuffer<T: Instance> {
    /// Byte offset from USB DPRAM base.
    addr: u16,
    /// Reserved byte capacity, rounded to 64 bytes for data endpoints; zero packet size reserves zero.
    len: u16,
    /// Peripheral/direction lifetime ownership marker.
    _phantom: PhantomData<T>,
}

impl<T: Instance> EndpointBuffer<T> {
    /// Returns a DPRAM view at byte offset `addr` with capacity `len`; does not access RAM.
    const fn new(
        addr: u16,
        len: u16,
    ) -> Self {
        Self {
            addr,
            len,
            _phantom: PhantomData,
        }
    }

    /// Copies exactly `buf.len()` bytes from software-owned DPRAM into `buf`; panics beyond capacity.
    fn read(
        &mut self,
        buf: &mut [u8],
    ) {
        assert!(buf.len() <= self.len as usize);
        compiler_fence(Ordering::SeqCst);
        let mem = unsafe { slice::from_raw_parts((T::dpram().as_ptr() as *mut u8).add(self.addr as _), buf.len()) };
        buf.copy_from_slice(mem);
        compiler_fence(Ordering::SeqCst);
    }

    /// Copies `buf` into software-owned DPRAM with compiler fences; panics beyond capacity.
    fn write(
        &mut self,
        buf: &[u8],
    ) {
        assert!(buf.len() <= self.len as usize);
        compiler_fence(Ordering::SeqCst);
        let mem = unsafe { slice::from_raw_parts_mut((T::dpram().as_ptr() as *mut u8).add(self.addr as _), buf.len()) };
        mem.copy_from_slice(buf);
        compiler_fence(Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Allocation metadata retained for reset and OUT rearming.
struct EndpointData {
    /// Transfer type, meaningful only while `used` is true.
    ep_type: EndpointType, // only valid if used
    /// Configured maximum USB packet payload bytes.
    max_packet_size: u16,
    /// Whether this endpoint index has been allocated.
    used: bool,
}

impl EndpointData {
    /// Returns an unused endpoint slot with zero packet capacity.
    const fn new() -> Self {
        Self {
            ep_type: EndpointType::Bulk,
            max_packet_size: 0,
            used: false,
        }
    }
}

/// RP2040/RP235x endpoint allocator owning the USB peripheral lifetime.
pub struct Driver<'d, T: Instance> {
    /// Retains exclusive peripheral lifetime ownership.
    phantom: PhantomData<&'d mut T>,
    /// Independent IN endpoint allocations.
    ep_in: [EndpointData; EP_COUNT],
    /// OUT endpoint allocations and maximum packet lengths.
    ep_out: [EndpointData; EP_COUNT],
    /// First unallocated DPRAM byte offset, starting after endpoint 0 storage.
    ep_mem_free: u16, // first free address in EP mem, in bytes.
}

impl<'d, T: Instance> Driver<'d, T> {
    /// Returns an allocator owning `_usb` with `_irq` handler proof. Enables its IRQ,
    /// resets controller/control RAM, selects internal PHY and forces VBUS detected.
    /// Requires configured USB clock and exclusive controller ownership; no physical VBUS sensing.
    pub fn new(
        _usb: Peri<'d, T>,
        _irq: impl Binding<T::Interrupt, InterruptHandler<T>>,
    ) -> Self {
        critical_section::with(|_| {
            for generation in EP_IN_GENERATIONS.iter().chain(EP_OUT_GENERATIONS.iter()) {
                generation.store(0, Ordering::Release);
            }
        });
        T::Interrupt::unpend();
        unsafe { T::Interrupt::enable() };

        let regs = T::regs();
        unsafe {
            // zero fill regs
            let p = regs.as_ptr() as *mut u32;
            for i in 0..0x9C / 4 {
                p.add(i).write_volatile(0)
            }

            // zero fill epmem
            let p = EP_MEMORY as *mut u32;
            for i in 0..0x100 / 4 {
                p.add(i).write_volatile(0)
            }
        }

        regs.usb_muxing().write(|w| {
            w.set_to_phy(true);
            w.set_softcon(true);
        });
        regs.usb_pwr().write(|w| {
            w.set_vbus_detect(true);
            w.set_vbus_detect_override_en(true);
        });
        regs.main_ctrl().write(|w| {
            w.set_controller_en(true);
        });

        // Initialize the bus so that it signals that power is available
        BUS_WAKER.wake();

        Self {
            phantom: PhantomData,
            ep_in: [EndpointData::new(); EP_COUNT],
            ep_out: [EndpointData::new(); EP_COUNT],
            ep_mem_free: 0x180, // data buffer region
        }
    }

    /// Allocates `ep_type` using optional index `ep_addr`, payload bound `max_packet_size`
    /// and descriptor polling `interval_ms`. Returns an endpoint or allocation error for
    /// reserved/used index, payload over 64 (non-iso) or 1023 (iso), or exhausted DPRAM.
    /// Direction comes from this method; the address direction is not validated.
    fn alloc_endpoint<D: Dir>(
        &mut self,
        ep_type: EndpointType,
        ep_addr: Option<EndpointAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<Endpoint<'d, T, D>, driver::EndpointAllocError> {
        trace!(
            "allocating type={:?} mps={:?} interval_ms={}, dir={:?}",
            ep_type,
            max_packet_size,
            interval_ms,
            D::dir()
        );

        let alloc = match D::dir() {
            Direction::Out => &mut self.ep_out,
            Direction::In => &mut self.ep_in,
        };

        let index = if let Some(addr) = ep_addr {
            // Use the specified endpoint address
            let requested_index = addr.index();
            if requested_index == 0 || requested_index >= EP_COUNT {
                return Err(EndpointAllocError);
            }
            if alloc[requested_index].used {
                return Err(EndpointAllocError);
            }
            Some((requested_index, &mut alloc[requested_index]))
        } else {
            // Find any available endpoint
            alloc.iter_mut().enumerate().find(|(i, ep)| {
                if *i == 0 {
                    return false; // reserved for control pipe
                }
                !ep.used
            })
        };

        let (index, ep) = index.ok_or(EndpointAllocError)?;
        assert!(!ep.used);

        // as per datasheet, the maximum buffer size is 64, except for isochronous
        // endpoints, which are allowed to be up to 1023 bytes.
        if (ep_type != EndpointType::Isochronous && max_packet_size > 64) || max_packet_size > 1023 {
            warn!("max_packet_size too high: {}", max_packet_size);
            return Err(EndpointAllocError);
        }

        // ep mem addrs must be 64-byte aligned, so there's no point in trying
        // to allocate smaller chunks to save memory.
        let len = (max_packet_size + 63) / 64 * 64;

        let addr = self.ep_mem_free;
        if addr + len > EP_MEMORY_SIZE as u16 {
            warn!("Endpoint memory full");
            return Err(EndpointAllocError);
        }
        self.ep_mem_free += len;

        let buf = EndpointBuffer {
            addr,
            len,
            _phantom: PhantomData,
        };

        trace!("  index={} addr={} len={}", index, buf.addr, buf.len);

        ep.ep_type = ep_type;
        ep.used = true;
        ep.max_packet_size = max_packet_size;

        let ep_type_reg = match ep_type {
            EndpointType::Bulk => pac::usb_dpram::vals::EpControlEndpointType::BULK,
            EndpointType::Control => pac::usb_dpram::vals::EpControlEndpointType::CONTROL,
            EndpointType::Interrupt => pac::usb_dpram::vals::EpControlEndpointType::INTERRUPT,
            EndpointType::Isochronous => pac::usb_dpram::vals::EpControlEndpointType::ISOCHRONOUS,
        };

        match D::dir() {
            Direction::Out => {
                T::dpram().ep_out_control(index - 1).write(|w| {
                    w.set_enable(false);
                    w.set_buffer_address(addr);
                    w.set_interrupt_per_buff(true);
                    w.set_endpoint_type(ep_type_reg);
                })
            }
            Direction::In => {
                T::dpram().ep_in_control(index - 1).write(|w| {
                    w.set_enable(false);
                    w.set_buffer_address(addr);
                    w.set_interrupt_per_buff(true);
                    w.set_endpoint_type(ep_type_reg);
                })
            }
        }

        Ok(Endpoint {
            _phantom: PhantomData,
            info: EndpointInfo {
                addr: EndpointAddress::from_parts(index, D::dir()),
                ep_type,
                max_packet_size,
                interval_ms,
            },
            buf,
        })
    }
}

/// USB interrupt handler.
pub struct InterruptHandler<T: Instance> {
    /// Compile-time USB peripheral identity.
    _uart: PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for InterruptHandler<T> {
    /// Masks handled bus/setup events and wakes bus/endpoint waiters; acknowledges buffer status.
    ///
    /// # Safety
    /// Run only for T's USB interrupt with initialized exclusive controller state.
    unsafe fn on_interrupt() {
        let regs = T::regs();
        // let x = regs.istr().read().0;
        // trace!("USB IRQ: {:08x}", x);

        let ints = regs.ints().read();

        if ints.bus_reset() {
            regs.inte().write_clear(|w| w.set_bus_reset(true));
            critical_section::with(|cs| reset_endpoints::<T>(cs));
            BUS_WAKER.wake();
        }
        if ints.dev_resume_from_host() {
            regs.inte().write_clear(|w| w.set_dev_resume_from_host(true));
            BUS_WAKER.wake();
        }
        if ints.dev_suspend() {
            regs.inte().write_clear(|w| w.set_dev_suspend(true));
            BUS_WAKER.wake();
        }
        if ints.setup_req() {
            regs.inte().write_clear(|w| w.set_setup_req(true));
            EP_IN_WAKERS[0].wake();
            EP_OUT_WAKERS[0].wake();
        }

        if ints.buff_status() {
            let s = regs.buff_status().read();
            regs.buff_status().write_value(s);

            for i in 0..EP_COUNT {
                if s.ep_in(i) {
                    EP_IN_WAKERS[i].wake();
                }
                if s.ep_out(i) {
                    EP_OUT_WAKERS[i].wake();
                }
            }
        }
    }
}

impl<'d, T: Instance> driver::Driver<'d> for Driver<'d, T> {
    /// Whole-controller event and endpoint-control owner.
    type Bus = Bus<'d, T>;
    /// Combined IN/OUT endpoint 0 control-transfer owner.
    type ControlPipe = ControlPipe<'d, T>;
    /// Device-to-host single-buffered endpoint.
    type EndpointIn = Endpoint<'d, T, In>;
    /// Host-to-device single-buffered endpoint.
    type EndpointOut = Endpoint<'d, T, Out>;

    /// Allocates `ep_type` using optional index `ep_addr`, payload bound `max_packet_size`
    /// and descriptor polling `interval_ms`. Returns an endpoint or allocation error for
    /// reserved/used index, payload over 64 (non-iso) or 1023 (iso), or exhausted DPRAM.
    /// Direction comes from this method; the address direction is not validated.
    fn alloc_endpoint_in(
        &mut self,
        ep_type: EndpointType,
        ep_addr: Option<EndpointAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<Self::EndpointIn, driver::EndpointAllocError> {
        self.alloc_endpoint(ep_type, ep_addr, max_packet_size, interval_ms)
    }

    /// Allocates `ep_type` using optional index `ep_addr`, payload bound `max_packet_size`
    /// and descriptor polling `interval_ms`. Returns an endpoint or allocation error for
    /// reserved/used index, payload over 64 (non-iso) or 1023 (iso), or exhausted DPRAM.
    /// Direction comes from this method; the address direction is not validated.
    fn alloc_endpoint_out(
        &mut self,
        ep_type: EndpointType,
        ep_addr: Option<EndpointAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<Self::EndpointOut, driver::EndpointAllocError> {
        self.alloc_endpoint(ep_type, ep_addr, max_packet_size, interval_ms)
    }

    /// Enables USB event IRQs/pull-up and returns bus/control owners using `control_max_packet_size`.
    /// Panics before start writes unless the limit is 8,16,32 or 64 bytes.
    /// Individual non-control endpoints remain disabled until the stack enables them.
    fn start(
        self,
        control_max_packet_size: u16,
    ) -> (Self::Bus, Self::ControlPipe) {
        assert!(matches!(control_max_packet_size, 8 | 16 | 32 | 64));
        let regs = T::regs();
        regs.inte().write(|w| {
            w.set_bus_reset(true);
            w.set_buff_status(true);
            w.set_dev_resume_from_host(true);
            w.set_dev_suspend(true);
            w.set_setup_req(true);
        });
        regs.int_ep_ctrl().write(|w| {
            w.set_int_ep_active(0xFFFE); // all EPs
        });
        regs.sie_ctrl().write(|w| {
            w.set_ep0_int_1buf(true);
            w.set_pullup_en(true);
        });

        trace!("enabled");

        (
            Bus {
                phantom: PhantomData,
                inited: false,
                ep_out: self.ep_out,
            },
            ControlPipe {
                _phantom: PhantomData,
                max_packet_size: control_max_packet_size,
                generation: EP_IN_GENERATIONS[0].load(Ordering::Acquire),
            },
        )
    }
}

/// Type representing the RP USB bus.
pub struct Bus<'d, T: Instance> {
    /// Retains exclusive peripheral lifetime ownership.
    phantom: PhantomData<&'d mut T>,
    /// OUT endpoint allocations and maximum packet lengths.
    ep_out: [EndpointData; EP_COUNT],
    /// Whether initial synthetic PowerDetected was returned.
    inited: bool,
}

impl<'d, T: Instance> driver::Bus for Bus<'d, T> {
    /// Returns the next bus event: synthetic power once, then resume/reset/suspend in that order.
    /// Reset clears address and disables data endpoints, waking their waiters; cancellation
    /// while pending leaves interrupt/waker registration installed.
    async fn poll(&mut self) -> Event {
        poll_fn(move |cx| {
            BUS_WAKER.register(cx.waker());

            // TODO: implement VBUS detection.
            if !self.inited {
                self.inited = true;
                return Poll::Ready(Event::PowerDetected);
            }

            let regs = T::regs();
            let siestatus = regs.sie_status().read();
            let intrstatus = regs.intr().read();

            if siestatus.resume() || intrstatus.dev_resume_from_host() {
                regs.sie_status().write(|w| w.set_resume(true));
                return Poll::Ready(Event::Resume);
            }

            if siestatus.bus_reset() {
                regs.sie_status().write(|w| {
                    w.set_bus_reset(true);
                    w.set_setup_rec(true);
                });
                regs.buff_status().write(|w| w.0 = 0xFFFF_FFFF);
                regs.addr_endp().write(|w| w.set_address(0));

                critical_section::with(|cs| reset_endpoints::<T>(cs));
                return Poll::Ready(Event::Reset);
            }

            if siestatus.suspended() && intrstatus.dev_suspend() {
                regs.sie_status().write(|w| w.set_suspended(true));
                return Poll::Ready(Event::Suspend);
            }

            // no pending event. Reenable all irqs.
            regs.inte().write_set(|w| {
                w.set_bus_reset(true);
                w.set_dev_resume_from_host(true);
                w.set_dev_suspend(true);
            });
            Poll::Pending
        })
        .await
    }

    /// Sets `ep_addr` stall to `stalled` and wakes its waiter; endpoint 0 also arms its stall bit.
    /// Index must identify a valid allocated endpoint; buffer contents/PID are retained.
    fn endpoint_set_stalled(
        &mut self,
        ep_addr: EndpointAddress,
        stalled: bool,
    ) {
        let n = ep_addr.index();

        if n == 0 {
            T::regs().ep_stall_arm().modify(|w| {
                if ep_addr.is_in() {
                    w.set_ep0_in(stalled);
                } else {
                    w.set_ep0_out(stalled);
                }
            });
        }

        let ctrl = if ep_addr.is_in() {
            T::dpram().ep_in_buffer_control(n)
        } else {
            T::dpram().ep_out_buffer_control(n)
        };

        ctrl.modify(|w| w.set_stall(stalled));

        let wakers = if ep_addr.is_in() { &EP_IN_WAKERS } else { &EP_OUT_WAKERS };
        wakers[n].wake();
    }

    /// Returns the stall bit for valid allocated `ep_addr`; does not clear it.
    fn endpoint_is_stalled(
        &mut self,
        ep_addr: EndpointAddress,
    ) -> bool {
        let n = ep_addr.index();

        let ctrl = if ep_addr.is_in() {
            T::dpram().ep_in_buffer_control(n)
        } else {
            T::dpram().ep_out_buffer_control(n)
        };

        ctrl.read().stall()
    }

    /// Sets non-control `ep_addr` to `enabled`, resets its PID/buffer and wakes its waiter.
    /// Endpoint0 is unchanged. Disable discards prior contents without rearming OUT.
    /// Every call invalidates older transfers. Hardware revocation uses at most 65,536 reads;
    /// timeout/unsupported abort/exhaustion quarantines the direction until Driver::new.
    fn endpoint_set_enabled(
        &mut self,
        ep_addr: EndpointAddress,
        enabled: bool,
    ) {
        trace!("set_enabled {:?} {}", ep_addr, enabled);
        if ep_addr.index() == 0 {
            return;
        }

        let n = ep_addr.index();
        critical_section::with(|cs| {
            if !quiesce_endpoint::<T>(n, ep_addr.direction(), cs) {
                return;
            }
            match ep_addr.direction() {
                Direction::In => {
                    invalidate(&EP_IN_GENERATIONS[n], cs);
                    let enabled = enabled && EP_IN_GENERATIONS[n].load(Ordering::Acquire) != u32::MAX;
                    T::dpram().ep_in_control(n - 1).modify(|w| w.set_enable(enabled));
                    T::dpram().ep_in_buffer_control(n).write(|w| w.set_pid(0, true));
                    EP_IN_WAKERS[n].wake();
                }
                Direction::Out => {
                    invalidate(&EP_OUT_GENERATIONS[n], cs);
                    let enabled = enabled && EP_OUT_GENERATIONS[n].load(Ordering::Acquire) != u32::MAX;
                    T::dpram().ep_out_control(n - 1).modify(|w| w.set_enable(enabled));
                    let ctrl = T::dpram().ep_out_buffer_control(n);
                    ctrl.write(|w| w.set_length(0, self.ep_out[n].max_packet_size));
                    if enabled {
                        buffer_control_delay();
                        ctrl.modify(|w| w.set_available(0, true));
                    }
                    EP_OUT_WAKERS[n].wake();
                }
            }
        });
    }

    /// Enables the controller then reconnects its pull-up; USB clock must remain configured.
    /// Data endpoints remain disabled until explicitly enabled by the stack.
    async fn enable(&mut self) {
        T::regs().main_ctrl().modify(|w| w.set_controller_en(true));
        T::regs().sie_ctrl().modify(|w| w.set_pullup_en(true));
    }

    /// Disconnects the pull-up, stops the controller and invalidates/wakes all endpoint waiters.
    /// Does not gate the shared USB clock or reset allocations. At most32 aborts of 65,536
    /// register reads; timeout quarantines that direction. Next poll reports PowerDetected again.
    async fn disable(&mut self) {
        critical_section::with(|cs| {
            T::regs().sie_ctrl().modify(|w| w.set_pullup_en(false));
            // At most 32 * ABORT_POLL_LIMIT register reads before the controller is stopped.
            for i in 0..EP_COUNT {
                quiesce_endpoint::<T>(i, Direction::In, cs);
                quiesce_endpoint::<T>(i, Direction::Out, cs);
            }
            T::regs().main_ctrl().modify(|w| w.set_controller_en(false));
            reset_endpoints::<T>(cs);
            T::regs().addr_endp().write(|w| w.set_address(0));
            T::regs().sie_status().write(|w| {
                w.set_setup_rec(true);
                w.set_bus_reset(true);
            });
            T::regs().buff_status().write(|w| w.0 = u32::MAX);
            self.inited = false;
        });
    }

    /// Returns Unsupported without signaling the host.
    async fn remote_wakeup(&mut self) -> Result<(), Unsupported> { Err(Unsupported) }
}

/// Compile-time endpoint transfer direction.
trait Dir {
    /// Returns this marker's USB transfer direction.
    fn dir() -> Direction;
}

/// Type for In direction.
pub enum In {}
impl Dir for In {
    /// Returns this marker's USB transfer direction.
    fn dir() -> Direction { Direction::In }
}

/// Type for Out direction.
pub enum Out {}
impl Dir for Out {
    /// Returns this marker's USB transfer direction.
    fn dir() -> Direction { Direction::Out }
}

/// Endpoint for RP USB driver.
pub struct Endpoint<'d, T: Instance, D> {
    /// Peripheral/direction lifetime ownership marker.
    _phantom: PhantomData<(&'d mut T, D)>,
    /// Allocated USB endpoint descriptor parameters.
    info: EndpointInfo,
    /// Owned packet-RAM buffer, reused after hardware releases AVAILABLE.
    buf: EndpointBuffer<T>,
}

impl<'d, T: Instance> driver::Endpoint for Endpoint<'d, T, In> {
    /// Returns immutable allocated endpoint address/type/packet/interval metadata.
    fn info(&self) -> &EndpointInfo { &self.info }

    /// Waits for the endpoint enable bit; cancellation leaves only its waker registration.
    async fn wait_enabled(&mut self) {
        trace!("wait_enabled IN WAITING");
        let index = self.info.addr.index();
        poll_fn(|cx| {
            EP_IN_WAKERS[index].register(cx.waker());
            let val = T::dpram().ep_in_control(self.info.addr.index() - 1).read();
            if val.enable() { Poll::Ready(()) } else { Poll::Pending }
        })
        .await;
        trace!("wait_enabled IN OK");
    }
}

impl<'d, T: Instance> driver::Endpoint for Endpoint<'d, T, Out> {
    /// Returns immutable allocated endpoint address/type/packet/interval metadata.
    fn info(&self) -> &EndpointInfo { &self.info }

    /// Waits for the endpoint enable bit; cancellation leaves only its waker registration.
    async fn wait_enabled(&mut self) {
        trace!("wait_enabled OUT WAITING");
        let index = self.info.addr.index();
        poll_fn(|cx| {
            EP_OUT_WAKERS[index].register(cx.waker());
            let val = T::dpram().ep_out_control(self.info.addr.index() - 1).read();
            if val.enable() { Poll::Ready(()) } else { Poll::Pending }
        })
        .await;
        trace!("wait_enabled OUT OK");
    }
}

impl<'d, T: Instance> driver::EndpointOut for Endpoint<'d, T, Out> {
    /// Copies one packet into `buf`, rearms OUT and returns its byte count.
    /// BufferOverflow preserves the packet; reset/disable/re-enable returns Disabled.
    /// Pending cancellation copies nothing; the hardware buffer remains armed.
    async fn read(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, EndpointError> {
        let index = self.info.addr.index();
        let generation = EP_OUT_GENERATIONS[index].load(Ordering::Acquire);
        poll_fn(|cx| {
            EP_OUT_WAKERS[index].register(cx.waker());
            critical_section::with(|_| {
                if let Err(e) = endpoint_valid::<T>(index, Direction::Out, generation) {
                    return Poll::Ready(Err(e));
                }
                let ctrl = T::dpram().ep_out_buffer_control(index);
                let val = ctrl.read();
                if val.available(0) {
                    return Poll::Pending;
                }
                let rx_len = val.length(0) as usize;
                if rx_len > buf.len() || rx_len > self.info.max_packet_size as usize {
                    return Poll::Ready(Err(EndpointError::BufferOverflow));
                }
                self.buf.read(&mut buf[..rx_len]);
                if let Err(e) = endpoint_valid::<T>(index, Direction::Out, generation) {
                    return Poll::Ready(Err(e));
                }
                ctrl.write(|w| {
                    w.set_pid(0, !val.pid(0));
                    w.set_length(0, self.info.max_packet_size);
                });
                buffer_control_delay();
                if let Err(e) = endpoint_valid::<T>(index, Direction::Out, generation) {
                    return Poll::Ready(Err(e));
                }
                ctrl.modify(|w| w.set_available(0, true));
                Poll::Ready(Ok(rx_len))
            })
        })
        .await
    }
}

impl<'d, T: Instance> driver::EndpointIn for Endpoint<'d, T, In> {
    /// Copies `buf` after prior IN release, returning success at hardware admission.
    /// Oversized packets return BufferOverflow; reset/disable/re-enable returns Disabled.
    /// Zero length queues a ZLP; pending cancellation preserves the prior packet.
    async fn write(
        &mut self,
        buf: &[u8],
    ) -> Result<(), EndpointError> {
        if buf.len() > self.info.max_packet_size as usize {
            return Err(EndpointError::BufferOverflow);
        }
        let index = self.info.addr.index();
        let generation = EP_IN_GENERATIONS[index].load(Ordering::Acquire);
        poll_fn(|cx| {
            EP_IN_WAKERS[index].register(cx.waker());
            critical_section::with(|_| {
                if let Err(e) = endpoint_valid::<T>(index, Direction::In, generation) {
                    return Poll::Ready(Err(e));
                }
                let ctrl = T::dpram().ep_in_buffer_control(index);
                let val = ctrl.read();
                if val.available(0) {
                    return Poll::Pending;
                }
                self.buf.write(buf);
                if let Err(e) = endpoint_valid::<T>(index, Direction::In, generation) {
                    return Poll::Ready(Err(e));
                }
                ctrl.write(|w| {
                    w.set_pid(0, !val.pid(0));
                    w.set_length(0, buf.len() as _);
                    w.set_full(0, true);
                });
                buffer_control_delay();
                if let Err(e) = endpoint_valid::<T>(index, Direction::In, generation) {
                    return Poll::Ready(Err(e));
                }
                ctrl.modify(|w| w.set_available(0, true));
                Poll::Ready(Ok(()))
            })
        })
        .await
    }
}

/// Control pipe for RP USB driver.
pub struct ControlPipe<'d, T: Instance> {
    /// Peripheral/direction lifetime ownership marker.
    _phantom: PhantomData<&'d mut T>,
    /// Configured maximum USB packet payload bytes.
    max_packet_size: u16,
    /// Reset/disable identity accepted with the latest SETUP; a new SETUP is separately latched.
    generation: u32,
}

impl<'d, T: Instance> ControlPipe<'d, T> {
    /// Returns success while the accepted SETUP identity is live and no new SETUP/reset is pending.
    fn check_active(&self) -> Result<(), EndpointError> {
        let status = T::regs().sie_status().read();
        if self.generation == u32::MAX
            || self.generation != EP_IN_GENERATIONS[0].load(Ordering::Acquire)
            || EP_OUT_GENERATIONS[0].load(Ordering::Acquire) == u32::MAX
            || status.setup_rec()
            || status.bus_reset()
            || !T::regs().main_ctrl().read().controller_en()
        {
            Err(EndpointError::Disabled)
        } else {
            Ok(())
        }
    }

    /// Sends status and commits optional address `addr` only after its live completion.
    /// Returns Disabled on SETUP/reset/disable; pending cancellation never writes the address.
    async fn accept_status(
        &mut self,
        addr: Option<u8>,
    ) -> Result<(), EndpointError> {
        critical_section::with(|_| {
            self.check_active()?;
            let ctrl = T::dpram().ep_in_buffer_control(0);
            ctrl.write(|w| {
                w.set_pid(0, true);
                w.set_full(0, true);
            });
            buffer_control_delay();
            self.check_active()?;
            ctrl.modify(|w| w.set_available(0, true));
            Ok(())
        })?;
        poll_fn(|cx| {
            EP_IN_WAKERS[0].register(cx.waker());
            critical_section::with(|_| {
                if let Err(e) = self.check_active() {
                    return Poll::Ready(Err(e));
                }
                if T::dpram().ep_in_buffer_control(0).read().available(0) {
                    return Poll::Pending;
                }
                if let Some(addr) = addr {
                    if let Err(e) = self.check_active() {
                        return Poll::Ready(Err(e));
                    }
                    T::regs().addr_endp().write(|w| w.set_address(addr));
                }
                Poll::Ready(Ok(()))
            })
        })
        .await
    }
}

impl<'d, T: Instance> driver::ControlPipe for ControlPipe<'d, T> {
    /// Returns the configured endpoint 0 packet payload limit in bytes.
    fn max_packet_size(&self) -> usize { self.max_packet_size as usize }

    /// Returns eight raw SETUP bytes, acknowledges the request and resets both DATA PIDs.
    /// Pending cancellation leaves IRQ registration installed; exhausted identities remain pending.
    async fn setup(&mut self) -> [u8; 8] {
        T::regs().inte().write_set(|w| w.set_setup_req(true));
        poll_fn(|cx| {
            EP_OUT_WAKERS[0].register(cx.waker());
            critical_section::with(|cs| {
                let regs = T::regs();
                let status = regs.sie_status().read();
                let generation = EP_IN_GENERATIONS[0].load(Ordering::Acquire);
                if !status.setup_rec()
                    || status.bus_reset()
                    || generation == u32::MAX
                    || !regs.main_ctrl().read().controller_en()
                {
                    return Poll::Pending;
                }
                if !quiesce_endpoint::<T>(0, Direction::In, cs) || !quiesce_endpoint::<T>(0, Direction::Out, cs) {
                    return Poll::Pending;
                }
                let mut buf = [0; 8];
                EndpointBuffer::<T>::new(0, 8).read(&mut buf);
                regs.sie_status().write(|w| w.set_setup_rec(true));
                T::dpram().ep_in_buffer_control(0).write(|_| {});
                T::dpram().ep_out_buffer_control(0).write(|_| {});
                self.generation = generation;
                Poll::Ready(buf)
            })
        })
        .await
    }

    /// Arms OUT and returns the bytes copied into `buf`; `first`/`last` do not change hardware sequencing.
    /// Returns BufferOverflow for insufficient space and Disabled on SETUP/reset/disable.
    /// Pending cancellation leaves the packet armed until setup/reset recovery.
    async fn data_out(
        &mut self,
        buf: &mut [u8],
        first: bool,
        last: bool,
    ) -> Result<usize, EndpointError> {
        let _ = (first, last);
        critical_section::with(|_| {
            self.check_active()?;
            let ctrl = T::dpram().ep_out_buffer_control(0);
            let pid = !ctrl.read().pid(0);
            ctrl.write(|w| {
                w.set_length(0, self.max_packet_size);
                w.set_pid(0, pid);
            });
            buffer_control_delay();
            self.check_active()?;
            ctrl.modify(|w| w.set_available(0, true));
            Ok(())
        })?;
        poll_fn(|cx| {
            EP_OUT_WAKERS[0].register(cx.waker());
            critical_section::with(|_| {
                if let Err(e) = self.check_active() {
                    return Poll::Ready(Err(e));
                }
                let val = T::dpram().ep_out_buffer_control(0).read();
                if val.available(0) {
                    return Poll::Pending;
                }
                let len = val.length(0) as usize;
                if len > buf.len() || len > self.max_packet_size as usize {
                    return Poll::Ready(Err(EndpointError::BufferOverflow));
                }
                EndpointBuffer::<T>::new(0x100, 64).read(&mut buf[..len]);
                Poll::Ready(Ok(len))
            })
        })
        .await
    }

    /// Sends `data` within the configured packet limit; `first` is informational, `last` arms OUT status.
    /// Returns BufferOverflow before copying oversized input, or Disabled on SETUP/reset/disable.
    /// Pending cancellation can leave a packet armed until setup/reset recovery.
    async fn data_in(
        &mut self,
        data: &[u8],
        first: bool,
        last: bool,
    ) -> Result<(), EndpointError> {
        let _ = first;
        if data.len() > self.max_packet_size as usize {
            return Err(EndpointError::BufferOverflow);
        }
        critical_section::with(|_| {
            self.check_active()?;
            EndpointBuffer::<T>::new(0x100, 64).write(data);
            let ctrl = T::dpram().ep_in_buffer_control(0);
            let pid = !ctrl.read().pid(0);
            ctrl.write(|w| {
                w.set_length(0, data.len() as _);
                w.set_pid(0, pid);
                w.set_full(0, true);
            });
            buffer_control_delay();
            self.check_active()?;
            ctrl.modify(|w| w.set_available(0, true));
            Ok(())
        })?;
        poll_fn(|cx| {
            EP_IN_WAKERS[0].register(cx.waker());
            critical_section::with(|_| {
                if let Err(e) = self.check_active() {
                    return Poll::Ready(Err(e));
                }
                if T::dpram().ep_in_buffer_control(0).read().available(0) {
                    return Poll::Pending;
                }
                if last {
                    let ctrl = T::dpram().ep_out_buffer_control(0);
                    ctrl.write(|w| w.set_pid(0, true));
                    buffer_control_delay();
                    if let Err(e) = self.check_active() {
                        return Poll::Ready(Err(e));
                    }
                    ctrl.modify(|w| w.set_available(0, true));
                }
                Poll::Ready(Ok(()))
            })
        })
        .await
    }

    /// Sends DATA1 IN status and waits for release or SETUP/reset/disable cancellation.
    async fn accept(&mut self) { let _ = self.accept_status(None).await; }

    /// Stalls both directions only if the accepted SETUP is still active; otherwise does nothing.
    async fn reject(&mut self) {
        critical_section::with(|_| {
            if self.check_active().is_err() {
                return;
            }
            T::regs().ep_stall_arm().write_set(|w| {
                w.set_ep0_in(true);
                w.set_ep0_out(true);
            });
            T::dpram().ep_out_buffer_control(0).write(|w| w.set_stall(true));
            T::dpram().ep_in_buffer_control(0).write(|w| w.set_stall(true));
        });
    }

    /// Acknowledges status and commits `addr` (caller range0..=127) only for that live transfer.
    /// SETUP/reset/disable or future cancellation leaves the address unchanged by this call.
    async fn accept_set_address(
        &mut self,
        addr: u8,
    ) {
        let _ = self.accept_status(Some(addr)).await;
    }
}

/// Waits the required twelve CPU cycles between buffer-control writes; fake-register tests skip time.
fn buffer_control_delay() {
    #[cfg(not(test))]
    cortex_m::asm::delay(12);
}

/// Actual driver futures exercised against RAM-backed PAC registers, without USB hardware.
#[cfg(test)]
mod tests {
    extern crate std;
    use core::cell::UnsafeCell;
    use core::future::Future;
    use core::pin::pin;
    use core::task::{Context, Waker};
    use std::sync::Mutex;

    use driver::{Bus as _, ControlPipe as _, Driver as _, EndpointIn as _, EndpointOut as _};

    use super::*;

    /// Serializes the singleton register banks and global endpoint wakers.
    static LOCK: Mutex<()> = Mutex::new(());
    /// Whether the fake peripheral acknowledges new abort requests.
    static ABORT_ACK: AtomicU32 = AtomicU32::new(1);
    /// Fake silicon capability; zero models the RP2040 E2 affected revisions.
    static ABORT_SUPPORTED: AtomicU32 = AtomicU32::new(1);
    /// Aligned register storage including the PAC atomic alias windows.
    struct Registers(
        /// Serialized fake register words, including alias windows.
        UnsafeCell<[u32; 4096]>,
    );
    // All access is serialized by LOCK, including polls and simulated IRQs.
    unsafe impl Sync for Registers {}
    /// Fake controller and alias storage.
    static REGS: Registers = Registers(UnsafeCell::new([0; 4096]));
    /// Fake DPRAM and alias storage.
    static RAM: Registers = Registers(UnsafeCell::new([0; 4096]));
    /// Test peripheral whose PAC operations access the two serialized arrays.
    #[derive(Clone, Copy)]
    struct FakeUsb;
    impl PeripheralType for FakeUsb {}
    impl SealedInstance for FakeUsb {
        /// Returns the fake controller bank; caller must hold LOCK.
        fn regs() -> pac::usb::Usb { unsafe { pac::usb::Usb::from_ptr(REGS.0.get().cast()) } }

        /// Returns the fake DPRAM bank; caller must hold LOCK.
        fn dpram() -> pac::usb_dpram::UsbDpram { unsafe { pac::usb_dpram::UsbDpram::from_ptr(RAM.0.get().cast()) } }

        /// Returns the test-selected silicon abort capability.
        fn abort_supported() -> bool { ABORT_SUPPORTED.load(Ordering::Relaxed) != 0 }

        /// Simulates acknowledgment for `mask`; writes no endpoint buffer control or payload.
        fn abort_requested(mask: u32) {
            Self::regs().ep_abort_done().write(|w| {
                w.0 = if ABORT_ACK.load(Ordering::Relaxed) != 0 {
                    mask
                } else {
                    0
                }
            });
        }
    }
    impl Instance for FakeUsb {
        /// Type proof only: tests do not enable or access the NVIC.
        type Interrupt = interrupt::typelevel::USBCTRL_IRQ;
    }
    /// Clears RAM under LOCK and returns a driver without touching the physical NVIC.
    fn driver() -> Driver<'static, FakeUsb> {
        unsafe {
            REGS.0.get().write([0; 4096]);
            RAM.0.get().write([0; 4096]);
        }
        ABORT_ACK.store(1, Ordering::Relaxed);
        ABORT_SUPPORTED.store(1, Ordering::Relaxed);
        for generation in EP_IN_GENERATIONS.iter().chain(EP_OUT_GENERATIONS.iter()) {
            generation.store(0, Ordering::Release);
        }
        FakeUsb::regs().main_ctrl().write(|w| w.set_controller_en(true));
        Driver {
            phantom: PhantomData,
            ep_in: [EndpointData::new(); EP_COUNT],
            ep_out: [EndpointData::new(); EP_COUNT],
            ep_mem_free: 0x180,
        }
    }
    /// Configured EP0 sizes are reported exactly and enforced before copying/arming IN.
    #[test]
    fn control_packet_limits() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        for size in [8, 16, 32, 64] {
            let (_, mut control) = driver().start(size);
            assert_eq!(control.max_packet_size(), size as usize);
            let data = [0u8; 65];
            let mut transfer = pin!(control.data_in(&data[..size as usize + 1], true, true));
            assert_eq!(
                transfer.as_mut().poll(&mut cx),
                Poll::Ready(Err(EndpointError::BufferOverflow))
            );
            assert!(!FakeUsb::dpram().ep_in_buffer_control(0).read().available(0));
        }
    }
    /// New SETUP aborts pending data and prevents an old status completion changing address.
    #[test]
    fn setup_aborts_control_and_address() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        let (_, mut control) = driver().start(64);
        {
            let mut transfer = pin!(control.data_in(&[1], true, false));
            assert!(transfer.as_mut().poll(&mut cx).is_pending());
            FakeUsb::regs().sie_status().write(|w| w.set_setup_rec(true));
            assert_eq!(
                transfer.as_mut().poll(&mut cx),
                Poll::Ready(Err(EndpointError::Disabled))
            );
        }
        FakeUsb::regs().sie_status().write(|_| {});
        let mut status = pin!(control.accept_set_address(42));
        assert!(status.as_mut().poll(&mut cx).is_pending());
        FakeUsb::regs().sie_status().write(|w| w.set_setup_rec(true));
        FakeUsb::dpram().ep_in_buffer_control(0).write(|_| {});
        assert!(status.as_mut().poll(&mut cx).is_ready());
        assert_eq!(FakeUsb::regs().addr_endp().read().address(), 0);
    }
    /// Disabled endpoints reject transfers; disable/re-enable cannot complete an older future.
    #[test]
    fn endpoint_disable_and_reenable_abort_old_transfers() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        let mut driver = driver();
        let mut input = driver.alloc_endpoint_in(EndpointType::Bulk, None, 64, 0).unwrap();
        let mut output = driver.alloc_endpoint_out(EndpointType::Bulk, None, 64, 0).unwrap();
        let (mut bus, _) = driver.start(64);
        let mut bytes = [0; 64];
        {
            let mut read = pin!(output.read(&mut bytes));
            assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Err(EndpointError::Disabled)));
        }
        bus.endpoint_set_enabled(input.info.addr, true);
        FakeUsb::dpram()
            .ep_in_buffer_control(1)
            .write(|w| w.set_available(0, true));
        let mut write = pin!(input.write(&[7]));
        assert!(write.as_mut().poll(&mut cx).is_pending());
        bus.endpoint_set_enabled(EndpointAddress::from_parts(1, Direction::In), false);
        bus.endpoint_set_enabled(EndpointAddress::from_parts(1, Direction::In), true);
        assert_eq!(write.as_mut().poll(&mut cx), Poll::Ready(Err(EndpointError::Disabled)));
    }
    /// Both endpoint 0 directions must be awakened for SETUP and reset cancellation.
    #[test]
    fn setup_and_reset_irq_wake_both_control_directions() {
        use std::sync::Arc;
        use std::task::Wake;

        use interrupt::typelevel::Handler;
        /// Counts wakes without polling any unrelated driver state.
        struct WakeCount(
            /// Number of wake callbacks observed by this test.
            AtomicU32,
        );
        impl Wake for WakeCount {
            /// Records one notification for this `self` counter.
            fn wake(self: Arc<Self>) { self.0.fetch_add(1, Ordering::Relaxed); }
        }
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _driver = driver();
        for reset in [false, true] {
            let input = Arc::new(WakeCount(AtomicU32::new(0)));
            let output = Arc::new(WakeCount(AtomicU32::new(0)));
            EP_IN_WAKERS[0].register(&Waker::from(input.clone()));
            EP_OUT_WAKERS[0].register(&Waker::from(output.clone()));
            // PAC INTS is read-only; the fake bank supplies a simulated IRQ cause.
            unsafe {
                FakeUsb::regs()
                    .ints()
                    .as_ptr()
                    .cast::<u32>()
                    .write_volatile(if reset { 1 << 12 } else { 1 << 16 });
            }
            unsafe {
                InterruptHandler::<FakeUsb>::on_interrupt();
            }
            assert_eq!(input.0.load(Ordering::Relaxed), 1);
            assert_eq!(output.0.load(Ordering::Relaxed), 1);
        }
    }

    /// Controller disable cancels old data/status even if re-enabled before their next poll.
    #[test]
    fn bus_disable_reenable_cancels_control_and_data() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        let mut driver = driver();
        let mut output = driver.alloc_endpoint_out(EndpointType::Bulk, None, 8, 0).unwrap();
        let (mut bus, mut control) = driver.start(8);
        let addr = output.info.addr;
        bus.endpoint_set_enabled(addr, true);
        let mut bytes = [0xAA; 8];
        let mut read = pin!(output.read(&mut bytes));
        let mut status = pin!(control.accept_set_address(17));
        assert!(read.as_mut().poll(&mut cx).is_pending());
        assert!(status.as_mut().poll(&mut cx).is_pending());
        assert!(pin!(bus.disable()).as_mut().poll(&mut cx).is_ready());
        assert!(!FakeUsb::regs().sie_ctrl().read().pullup_en());
        assert!(!FakeUsb::regs().main_ctrl().read().controller_en());
        assert!(!FakeUsb::dpram().ep_out_buffer_control(1).read().available(0));
        assert!(pin!(bus.enable()).as_mut().poll(&mut cx).is_ready());
        bus.endpoint_set_enabled(addr, true);
        assert!(FakeUsb::regs().sie_ctrl().read().pullup_en());
        assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Err(EndpointError::Disabled)));
        assert!(status.as_mut().poll(&mut cx).is_ready());
        assert_eq!(FakeUsb::regs().addr_endp().read().address(), 0);
    }

    /// SETUP before arming leaves control buffers untouched; pending OUT also aborts without copying.
    #[test]
    fn control_out_checks_setup_before_arm_and_copy() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        let (_, mut control) = driver().start(8);
        let mut bytes = [0xAA; 8];
        FakeUsb::regs().sie_status().write(|w| w.set_setup_rec(true));
        {
            let mut read = pin!(control.data_out(&mut bytes, true, true));
            assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Err(EndpointError::Disabled)));
            assert!(!FakeUsb::dpram().ep_out_buffer_control(0).read().available(0));
        }
        FakeUsb::regs().sie_status().write(|_| {});
        {
            let mut read = pin!(control.data_out(&mut bytes, true, true));
            assert!(read.as_mut().poll(&mut cx).is_pending());
            FakeUsb::regs().sie_status().write(|w| w.set_setup_rec(true));
            FakeUsb::dpram().ep_out_buffer_control(0).write(|w| w.set_length(0, 1));
            assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Err(EndpointError::Disabled)));
        }
        assert_eq!(bytes, [0xAA; 8]);
    }

    /// A reset IRQ invalidates pending stages even after the reset flag is acknowledged elsewhere.
    #[test]
    fn reset_cannot_turn_stale_status_into_address_commit() {
        use interrupt::typelevel::Handler;
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        let (_, mut control) = driver().start(64);
        let mut status = pin!(control.accept_set_address(71));
        assert!(status.as_mut().poll(&mut cx).is_pending());
        unsafe {
            FakeUsb::regs().ints().as_ptr().cast::<u32>().write_volatile(1 << 12);
        }
        unsafe {
            InterruptHandler::<FakeUsb>::on_interrupt();
        }
        FakeUsb::regs().sie_status().write(|_| {});
        assert!(status.as_mut().poll(&mut cx).is_ready());
        assert_eq!(FakeUsb::regs().addr_endp().read().address(), 0);
    }

    /// Successful packet copies, PID toggles, OUT retry and live status address commits remain usable.
    #[test]
    fn live_packets_and_address_complete() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        let mut driver = driver();
        let mut output = driver.alloc_endpoint_out(EndpointType::Bulk, None, 8, 0).unwrap();
        let (mut bus, mut control) = driver.start(8);
        bus.endpoint_set_enabled(output.info.addr, true);
        EndpointBuffer::<FakeUsb>::new(output.buf.addr, 8).write(&[2, 4, 6]);
        FakeUsb::dpram().ep_out_buffer_control(1).write(|w| w.set_length(0, 3));
        let mut small = [0; 2];
        assert_eq!(
            pin!(output.read(&mut small)).as_mut().poll(&mut cx),
            Poll::Ready(Err(EndpointError::BufferOverflow))
        );
        let mut data = [0; 8];
        assert_eq!(pin!(output.read(&mut data)).as_mut().poll(&mut cx), Poll::Ready(Ok(3)));
        assert_eq!(&data[..3], &[2, 4, 6]);
        assert!(FakeUsb::dpram().ep_out_buffer_control(1).read().available(0));
        assert!(FakeUsb::dpram().ep_out_buffer_control(1).read().pid(0));
        let mut status = pin!(control.accept_set_address(23));
        assert!(status.as_mut().poll(&mut cx).is_pending());
        assert_eq!(FakeUsb::regs().addr_endp().read().address(), 0);
        FakeUsb::dpram().ep_in_buffer_control(0).write(|_| {});
        assert!(status.as_mut().poll(&mut cx).is_ready());
        assert_eq!(FakeUsb::regs().addr_endp().read().address(), 23);
    }

    /// Generation exhaustion cannot alias an old transfer or re-enable a permanently exhausted endpoint.
    #[test]
    fn generation_exhaustion_is_fail_closed() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        let mut driver = driver();
        let mut input = driver.alloc_endpoint_in(EndpointType::Bulk, None, 8, 0).unwrap();
        let (mut bus, _) = driver.start(8);
        EP_IN_GENERATIONS[1].store(u32::MAX - 1, Ordering::Release);
        for _ in 0..2 {
            bus.endpoint_set_enabled(input.info.addr, true);
        }
        assert_eq!(EP_IN_GENERATIONS[1].load(Ordering::Acquire), u32::MAX);
        assert!(!FakeUsb::dpram().ep_in_control(0).read().enable());
        assert_eq!(
            pin!(input.write(&[1])).as_mut().poll(&mut cx),
            Poll::Ready(Err(EndpointError::Disabled))
        );
    }

    /// Unconfirmed hardware abort must preserve the old buffer and permanently quarantine its direction.
    #[test]
    fn abort_timeout_preserves_buffer_and_quarantine() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut driver = driver();
        let output = driver.alloc_endpoint_out(EndpointType::Bulk, None, 8, 0).unwrap();
        let (mut bus, _) = driver.start(8);
        bus.endpoint_set_enabled(output.info.addr, true);
        EndpointBuffer::<FakeUsb>::new(output.buf.addr, 8).write(&[0x55; 8]);
        let before = FakeUsb::dpram().ep_out_buffer_control(1).read().0;
        ABORT_ACK.store(0, Ordering::Relaxed);
        bus.endpoint_set_enabled(output.info.addr, false);
        assert_eq!(EP_OUT_GENERATIONS[1].load(Ordering::Acquire), u32::MAX);
        assert_eq!(FakeUsb::dpram().ep_out_buffer_control(1).read().0, before);
        critical_section::with(|cs| reset_endpoints::<FakeUsb>(cs));
        bus.endpoint_set_enabled(output.info.addr, true);
        assert!(!FakeUsb::dpram().ep_out_control(0).read().enable());
        assert_eq!(FakeUsb::dpram().ep_out_buffer_control(1).read().0, before);
        let mut bytes = [0; 8];
        EndpointBuffer::<FakeUsb>::new(output.buf.addr, 8).read(&mut bytes);
        assert_eq!(bytes, [0x55; 8]);
    }

    /// Acknowledged abort clears ownership before reuse; E2-affected silicon cannot trust DONE.
    #[test]
    fn abort_acknowledgment_and_legacy_silicon() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for supported in [true, false] {
            let mut driver = driver();
            let input = driver.alloc_endpoint_in(EndpointType::Bulk, None, 8, 0).unwrap();
            let (mut bus, _) = driver.start(8);
            bus.endpoint_set_enabled(input.info.addr, true);
            FakeUsb::dpram().ep_in_buffer_control(1).write(|w| {
                w.set_available(0, true);
                w.set_full(0, true);
                w.set_length(0, 5);
            });
            ABORT_SUPPORTED.store(u32::from(supported), Ordering::Relaxed);
            bus.endpoint_set_enabled(input.info.addr, false);
            assert!(!FakeUsb::dpram().ep_in_control(0).read().enable());
            if supported {
                assert_eq!(FakeUsb::regs().ep_abort().read().0 & 4, 0);
                assert!(!FakeUsb::dpram().ep_in_buffer_control(1).read().available(0));
                bus.endpoint_set_enabled(input.info.addr, true);
                assert!(FakeUsb::dpram().ep_in_control(0).read().enable());
            } else {
                assert_eq!(EP_IN_GENERATIONS[1].load(Ordering::Acquire), u32::MAX);
                assert_eq!(FakeUsb::regs().ep_abort().read().0 & 4, 4);
                assert_eq!(FakeUsb::dpram().ep_in_buffer_control(1).read().length(0), 5);
                assert!(FakeUsb::dpram().ep_in_buffer_control(1).read().available(0));
            }
        }
    }

    /// A fresh SETUP revokes previous EP0 hardware buffers before accepting its bytes.
    #[test]
    fn setup_reclaims_old_control_buffers_only_after_abort() {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Context::from_waker(Waker::noop());
        for acknowledge in [true, false] {
            let (_, mut control) = driver().start(8);
            EndpointBuffer::<FakeUsb>::new(0, 8).write(&[0, 5, 42, 0, 0, 0, 0, 0]);
            FakeUsb::dpram().ep_in_buffer_control(0).write(|w| {
                w.set_available(0, true);
                w.set_length(0, 7);
            });
            FakeUsb::regs().sie_status().write(|w| w.set_setup_rec(true));
            ABORT_ACK.store(u32::from(acknowledge), Ordering::Relaxed);
            let mut setup = pin!(control.setup());
            let result = setup.as_mut().poll(&mut cx);
            if acknowledge {
                assert_eq!(result, Poll::Ready([0, 5, 42, 0, 0, 0, 0, 0]));
                assert_eq!(FakeUsb::dpram().ep_in_buffer_control(0).read().0, 0);
            } else {
                assert!(result.is_pending());
                assert_eq!(FakeUsb::dpram().ep_in_buffer_control(0).read().length(0), 7);
                assert_eq!(EP_IN_GENERATIONS[0].load(Ordering::Acquire), u32::MAX);
            }
        }
    }
}
