//! Software panel transaction model for exact payload reconstruction and fault-boundary ownership.
use std::cell::Cell;
use std::future::Future;
use std::task::{Context, Poll, Waker};

use dashboard_config::{Geometry, Orientation};
use dashboard_host_validation::display::damage::DamagePlan;
use dashboard_host_validation::display::transfer::{FlushError, PanelBus, PanelState, flush_oriented_plan, flush_plan};
use dashboard_host_validation::display::{FrameBuffer, WIDTH};

/// Immediate panel-bus model; exercises protocol ordering without simulating electrical scanout.
struct ModelBus {
    /// Reconstructed RGB565 panel memory in current logical geometry.
    panel: Box<FrameBuffer>,
    /// Geometry selected by the latest accepted MADCTL byte.
    geometry: Geometry,
    /// Inclusive column window.
    columns: [usize; 2],
    /// Inclusive row window.
    rows: [usize; 2],
    /// Next logical pixel coordinate inside the active RAMWR window.
    cursor: [usize; 2],
    /// Successfully accepted command bytes and copied parameter payloads.
    commands: Vec<(u8, Vec<u8>)>,
    /// Whether pixel selection remains asserted.
    active: bool,
    /// Number of attempted command/parameter/start/payload boundaries.
    operations: usize,
    /// Zero-based boundary at which to inject failure; None disables injection.
    fail_at: Option<usize>,
    /// Pixel bytes successfully copied into panel memory.
    payload_bytes: usize,
}
impl ModelBus {
    /// Returns a zeroed landscape panel with optional zero-based failure boundary `fail_at`.
    fn new(fail_at: Option<usize>) -> Self {
        Self {
            panel: Box::default(),
            geometry: Geometry::Landscape,
            columns: [0; 2],
            rows: [0; 2],
            cursor: [0; 2],
            commands: Vec::new(),
            active: false,
            operations: 0,
            fail_at,
            payload_bytes: 0,
        }
    }

    /// Counts one attempted boundary. Returns Err exactly at the configured index, otherwise Ok.
    fn operation(&mut self) -> Result<(), ()> {
        let failed = self.fail_at == Some(self.operations);
        self.operations += 1;
        if failed { Err(()) } else { Ok(()) }
    }
}
impl PanelBus for ModelBus {
    /// Injected boundary failure with no additional diagnostic payload.
    type Error = ();

    /// Applies `command` with `parameters` after separate command/parameter failure points.
    /// Returns Err before model mutation on injected failure. Panics for active pixel selection,
    /// unsupported commands/MADCTL values, short address parameters or reversed bounds.
    async fn command(
        &mut self,
        command: u8,
        parameters: &[u8],
    ) -> Result<(), ()> {
        assert!(!self.active);
        self.operation()?; // Command byte boundary.
        self.operation()?; // Parameter write boundary.
        if command == 0x36 {
            assert_eq!(parameters.len(), 1);
            self.geometry = match parameters[0] {
                0x60 | 0xA0 => Geometry::Landscape,
                0x00 | 0xC0 => Geometry::Portrait,
                _ => panic!("unexpected MADCTL"),
            };
            self.commands.push((command, parameters.to_vec()));
            return Ok(());
        }
        let bounds = [
            u16::from_be_bytes([parameters[0], parameters[1]]) as usize,
            u16::from_be_bytes([parameters[2], parameters[3]]) as usize,
        ];
        assert!(bounds[0] <= bounds[1]);
        match command {
            0x2A => self.columns = bounds,
            0x2B => self.rows = bounds,
            _ => panic!("unexpected command"),
        }
        self.commands.push((command, parameters.to_vec()));
        Ok(())
    }

    /// Returns Ok after asserting selection and resetting the cursor, or Err before either effect.
    /// Panics if pixel selection was already active.
    async fn begin_pixels(&mut self) -> Result<(), ()> {
        assert!(!self.active);
        self.operation()?;
        self.active = true;
        self.cursor = [self.columns[0], self.rows[0]];
        self.commands.push((0x2C, Vec::new()));
        Ok(())
    }

    /// Attempts one payload boundary, then calls `callback` once when present, even on failure.
    /// Returns Err without copying `bytes` on injection; otherwise copies big-endian RGB565 bytes.
    /// Panics for inactive selection, odd byte length, window overflow or invalid framebuffer indexing.
    async fn pixels<F: FnOnce()>(
        &mut self,
        bytes: &[u8],
        callback: Option<F>,
    ) -> Result<(), ()> {
        assert!(self.active);
        let result = self.operation();
        if let Some(callback) = callback {
            callback();
        }
        result?;
        assert_eq!(bytes.len() % 2, 0);
        for pixel in bytes.as_chunks::<2>().0 {
            let [x, y] = self.cursor;
            assert!(x <= self.columns[1] && y <= self.rows[1]);
            let at = (y * usize::from(self.geometry.width()) + x) * 2;
            self.panel[at..at + 2].copy_from_slice(pixel);
            self.cursor = if x == self.columns[1] {
                [self.columns[0], y + 1]
            } else {
                [x + 1, y]
            };
        }
        self.payload_bytes += bytes.len();
        Ok(())
    }

    /// Deasserts pixel selection; panics if no transfer was active.
    fn end_pixels(&mut self) {
        assert!(self.active);
        self.active = false;
    }
}
/// Polls immediate `future` once and returns its result; panics if the model unexpectedly suspends.
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("model bus unexpectedly pending"),
    }
}
/// Returns a black landscape fixture with two A55A pixels, including the bottom-right corner.
fn changed_frame() -> Box<FrameBuffer> {
    let mut frame = Box::<FrameBuffer>::default();
    for (x, y) in [(17, 29), (319, 239)] {
        let at = (y * WIDTH + x) * 2;
        frame[at..at + 2].copy_from_slice(&[0xA5, 0x5A]);
    }
    frame
}

/// Partial/full plans reconstruct identical pixels with one start callback; empty plans perform no I/O.
#[test]
fn shared_transaction_path_reconstructs_partial_full_and_empty_updates() {
    let frame = changed_frame();
    for plan in [DamagePlan::between(&FrameBuffer::new(), &frame), DamagePlan::full()] {
        let mut bus = ModelBus::new(None);
        let started = Cell::new(0);
        run(flush_plan(&mut bus, &frame, &plan, || started.set(started.get() + 1))).unwrap();
        assert!(bus.panel == frame);
        assert!(!bus.active);
        assert_eq!(started.get(), 1);
        assert_eq!(bus.payload_bytes, plan.bytes());
        for commands in bus.commands.as_chunks::<3>().0 {
            assert_eq!(commands.iter().map(|c| c.0).collect::<Vec<_>>(), vec![0x2A, 0x2B, 0x2C]);
        }
    }
    let mut bus = ModelBus::new(None);
    let empty = DamagePlan::between(&frame, &frame);
    run(flush_plan(&mut bus, &frame, &empty, || panic!("empty payload started"))).unwrap();
    assert_eq!(bus.operations, 0);
}

/// Every injected command or row failure stops further writes and leaves selection deasserted.
#[test]
fn every_command_and_row_failure_stops_writes_and_deasserts_selection() {
    let frame = changed_frame();
    let plan = DamagePlan::between(&FrameBuffer::new(), &frame);
    let mut success = ModelBus::new(None);
    run(flush_plan(&mut success, &frame, &plan, || {})).unwrap();
    for fail_at in 0..success.operations {
        let mut bus = ModelBus::new(Some(fail_at));
        let started = Cell::new(0);
        assert!(run(flush_plan(&mut bus, &frame, &plan, || started.set(started.get() + 1))).is_err());
        assert!(!bus.active, "operation {fail_at}");
        assert_eq!(bus.operations, fail_at + 1);
        assert!(started.get() <= 1);
    }
}

/// Each orientation change writes MADCTL before a full window/payload, even when all pixels are black.
#[test]
fn black_rotation_writes_madctl_then_complete_window_and_payload() {
    let frame = Box::<FrameBuffer>::default();
    let mut bus = ModelBus::new(None);
    let mut state = PanelState::initialized(Orientation::Landscape);
    for (orientation, madctl) in [
        (Orientation::Portrait, 0x00),
        (Orientation::LandscapeInverted, 0xA0),
        (Orientation::PortraitInverted, 0xC0),
        (Orientation::Landscape, 0x60),
    ] {
        let before = bus.commands.len();
        let bytes_before = bus.payload_bytes;
        let plan = DamagePlan::full_in(orientation.geometry());
        let started = Cell::new(0);
        run(flush_oriented_plan(
            &mut bus,
            &mut state,
            &frame,
            &plan,
            orientation,
            || started.set(started.get() + 1),
        ))
        .unwrap();
        assert_eq!(started.get(), 1);
        assert_eq!(bus.payload_bytes - bytes_before, 153600);
        assert_eq!(
            bus.commands[before..].iter().map(|c| c.0).collect::<Vec<_>>(),
            vec![0x36, 0x2A, 0x2B, 0x2C]
        );
        assert_eq!(bus.commands[before].1, vec![madctl]);
        assert_eq!(bus.columns, [0, usize::from(orientation.geometry().width()) - 1]);
        assert_eq!(bus.rows, [0, usize::from(orientation.geometry().height()) - 1]);
        assert_eq!(state.programmed_orientation(), Some(orientation));
        assert_eq!(state.completed_orientation(), Some(orientation));
    }
}

/// Orientation changes reject empty, partial or mismatched geometry plans without any bus operation.
#[test]
fn orientation_checked_seam_rejects_empty_partial_and_wrong_geometry_without_io() {
    let frame = Box::<FrameBuffer>::default();
    let mut changed = Box::<FrameBuffer>::default();
    changed[153599] = 1;
    for plan in [
        DamagePlan::between_in(Geometry::Portrait, &frame, &frame),
        DamagePlan::between_in(Geometry::Portrait, &frame, &changed),
        DamagePlan::full(),
    ] {
        let mut bus = ModelBus::new(None);
        let mut state = PanelState::initialized(Orientation::Landscape);
        assert!(
            run(flush_oriented_plan(
                &mut bus,
                &mut state,
                &frame,
                &plan,
                Orientation::Portrait,
                || panic!("invalid start")
            ))
            .is_err()
        );
        assert_eq!(bus.operations, 0);
        assert_eq!(state.programmed_orientation(), Some(Orientation::Landscape));
    }
}

/// Every rotation fault invalidates completed content; uncertain MADCTL and payload state require full rewrite.
#[test]
fn each_rotation_fault_invalidates_completed_content_and_requires_full_rewrite() {
    let frame = Box::<FrameBuffer>::default();
    let plan = DamagePlan::full_in(Geometry::Portrait);
    for fail_at in 0..8 {
        let mut bus = ModelBus::new(Some(fail_at));
        let mut state = PanelState::initialized(Orientation::Landscape);
        let starts = Cell::new(0);
        assert_eq!(
            run(flush_oriented_plan(
                &mut bus,
                &mut state,
                &frame,
                &plan,
                Orientation::Portrait,
                || starts.set(starts.get() + 1)
            )),
            Err(FlushError::Bus(()))
        );
        assert_eq!(state.completed_orientation(), None);
        assert_eq!(
            state.programmed_orientation(),
            if fail_at < 2 { None } else { Some(Orientation::Portrait) }
        );
        assert_eq!(starts.get(), usize::from(fail_at == 7));
        assert!(!bus.active);
        let empty = DamagePlan::between_in(Geometry::Portrait, &frame, &frame);
        let operations = bus.operations;
        assert_eq!(
            run(flush_oriented_plan(
                &mut bus,
                &mut state,
                &frame,
                &empty,
                Orientation::Portrait,
                || {}
            )),
            Err(FlushError::FullRewriteRequired)
        );
        assert_eq!(bus.operations, operations);
    }
}

/// A matching completed portrait base permits a 512-byte partial update without another MADCTL write.
#[test]
fn same_orientation_retains_partial_damage_without_madctl() {
    let previous = Box::<FrameBuffer>::default();
    let mut frame = Box::<FrameBuffer>::default();
    frame[153599] = 0x9A;
    let mut bus = ModelBus::new(None);
    let mut state = PanelState::initialized(Orientation::Landscape);
    run(flush_oriented_plan(
        &mut bus,
        &mut state,
        &previous,
        &DamagePlan::full_in(Geometry::Portrait),
        Orientation::Portrait,
        || {},
    ))
    .unwrap();
    bus.commands.clear();
    let bytes_before = bus.payload_bytes;
    let plan = DamagePlan::between_in(Geometry::Portrait, &previous, &frame);
    run(flush_oriented_plan(
        &mut bus,
        &mut state,
        &frame,
        &plan,
        Orientation::Portrait,
        || {},
    ))
    .unwrap();
    assert!(bus.panel == frame);
    assert_eq!(bus.payload_bytes - bytes_before, 512);
    assert_eq!(
        bus.commands.iter().map(|c| c.0).collect::<Vec<_>>(),
        vec![0x2A, 0x2B, 0x2C]
    );
}

/// A delayed prior payload retains transfer ownership until return, before the next orientation command.
#[test]
fn next_orientation_command_waits_for_previous_payload_return() {
    use std::cell::RefCell;

    use dashboard_host_validation::display::pipeline::after_first_poll;
    /// Model bus whose payload future is manually released by the test.
    struct DelayedBus<'a> {
        /// Immediate reconstruction and boundary-failure model.
        bus: ModelBus,
        /// Shared readiness gate; false leaves payload pending without copying.
        ready: &'a Cell<bool>,
        /// Ordered poll/command/return observations.
        events: &'a RefCell<Vec<&'static str>>,
    }
    impl PanelBus for DelayedBus<'_> {
        /// Forwarded model boundary failure.
        type Error = ();

        /// Logs attempted `command`, forwards `parameters`, and returns the model's validation/failure result.
        async fn command(
            &mut self,
            command: u8,
            parameters: &[u8],
        ) -> Result<(), ()> {
            self.events
                .borrow_mut()
                .push(if command == 0x36 { "madctl" } else { "window" });
            self.bus.command(command, parameters).await
        }

        /// Returns the model's selection result, preserving its already-active panic precondition.
        async fn begin_pixels(&mut self) -> Result<(), ()> { self.bus.begin_pixels().await }

        /// Waits for readiness before copying `bytes`; invokes optional `callback` after the first poll.
        /// Returns the model copy result and records successful return. Dropping while pending copies no bytes.
        async fn pixels<F: FnOnce()>(
            &mut self,
            bytes: &[u8],
            callback: Option<F>,
        ) -> Result<(), ()> {
            after_first_poll(
                std::future::poll_fn(|_| {
                    self.events.borrow_mut().push("payload_poll");
                    if self.ready.get() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                }),
                || {
                    if let Some(callback) = callback {
                        callback();
                    }
                },
            )
            .await;
            self.bus.pixels(bytes, None::<fn()>).await?;
            self.events.borrow_mut().push("payload_return");
            Ok(())
        }

        /// Releases modeled selection; panics if it was not active.
        fn end_pixels(&mut self) { self.bus.end_pixels(); }
    }
    let ready = Cell::new(false);
    let events = RefCell::new(Vec::new());
    let starts = Cell::new(0);
    let frame = Box::<FrameBuffer>::default();
    let mut bus = DelayedBus {
        bus: ModelBus::new(None),
        ready: &ready,
        events: &events,
    };
    let mut state = PanelState::initialized(Orientation::Landscape);
    let sequence = async {
        flush_oriented_plan(
            &mut bus,
            &mut state,
            &frame,
            &DamagePlan::full(),
            Orientation::Landscape,
            || starts.set(starts.get() + 1),
        )
        .await
        .unwrap();
        flush_oriented_plan(
            &mut bus,
            &mut state,
            &frame,
            &DamagePlan::full_in(Geometry::Portrait),
            Orientation::Portrait,
            || starts.set(starts.get() + 1),
        )
        .await
        .unwrap();
    };
    let mut sequence = std::pin::pin!(sequence);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(sequence.as_mut().poll(&mut cx), Poll::Pending);
    assert_eq!(starts.get(), 1);
    assert!(!events.borrow().contains(&"madctl"));
    ready.set(true);
    assert_eq!(sequence.as_mut().poll(&mut cx), Poll::Ready(()));
    assert_eq!(starts.get(), 2);
    let events = events.borrow();
    assert!(
        events.iter().position(|event| *event == "payload_return").unwrap()
            < events.iter().position(|event| *event == "madctl").unwrap()
    );
}
