use std::cell::Cell;
use std::future::Future;
use std::task::{Context, Poll, Waker};

use dashboard_host_validation::display::damage::DamagePlan;
use dashboard_host_validation::display::transfer::{PanelBus, flush_plan};
use dashboard_host_validation::display::{FrameBuffer, WIDTH};

struct ModelBus {
    panel: Box<FrameBuffer>,
    columns: [usize; 2],
    rows: [usize; 2],
    cursor: [usize; 2],
    commands: Vec<(u8, Vec<u8>)>,
    active: bool,
    operations: usize,
    fail_at: Option<usize>,
    payload_bytes: usize,
}
impl ModelBus {
    fn new(fail_at: Option<usize>) -> Self {
        Self {
            panel: Box::default(),
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

    fn operation(&mut self) -> Result<(), ()> {
        let failed = self.fail_at == Some(self.operations);
        self.operations += 1;
        if failed { Err(()) } else { Ok(()) }
    }
}
impl PanelBus for ModelBus {
    type Error = ();

    async fn command(
        &mut self,
        command: u8,
        parameters: &[u8],
    ) -> Result<(), ()> {
        assert!(!self.active);
        self.operation()?;
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

    async fn begin_pixels(&mut self) -> Result<(), ()> {
        assert!(!self.active);
        self.operation()?;
        self.active = true;
        self.cursor = [self.columns[0], self.rows[0]];
        self.commands.push((0x2C, Vec::new()));
        Ok(())
    }

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
            let at = (y * WIDTH + x) * 2;
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

    fn end_pixels(&mut self) {
        assert!(self.active);
        self.active = false;
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("model bus unexpectedly pending"),
    }
}
fn changed_frame() -> Box<FrameBuffer> {
    let mut frame = Box::<FrameBuffer>::default();
    for (x, y) in [(17, 29), (319, 239)] {
        let at = (y * WIDTH + x) * 2;
        frame[at..at + 2].copy_from_slice(&[0xA5, 0x5A]);
    }
    frame
}

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
