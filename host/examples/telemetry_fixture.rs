//! Actual production aggregation and encoding, decoded by the Python contract.
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::display::telemetry::{FrameTrace, RenderTrace};
use dashboard_host_validation::display::window::{FrameWindow, WINDOW_US};
use dashboard_host_validation::input_trace::InputCounters;
use dashboard_host_validation::telemetry_protocol;
fn main() {
    let mut window = FrameWindow::new(0);
    let frame = FrameTrace::finish(
        FrameToken {
            buffer: BufferId::A,
            sequence: 1,
        },
        RenderTrace::instant(10),
        20,
        30,
        153600,
    )
    .unwrap();
    window.record(frame).unwrap();
    for at in [WINDOW_US, 2 * WINDOW_US] {
        telemetry_protocol::frames(window.report(at, InputCounters::ZERO).unwrap().unwrap(), |line| {
            println!("{line}")
        })
        .unwrap();
    }
    let mut session = dashboard_host_validation::diagnostics::session::Session::new(0x7E8, 1).unwrap();
    let tx = session.poll(0).unwrap().unwrap();
    session.tx_completed(tx.token, 1).unwrap();
    let response =
        dashboard_host_validation::diagnostics::CanFrame::new(0x7E8, &[6, 0x41, 0, 0x08, 0x02, 0, 1, 0]).unwrap();
    session.receive(&response, 1, 3).unwrap();
    telemetry_protocol::diagnostics(1, 4, session.telemetry(), |line| println!("{line}")).unwrap();
}
