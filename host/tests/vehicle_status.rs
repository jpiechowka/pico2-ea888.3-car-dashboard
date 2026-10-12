use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::diagnostics::service::{LinkSnapshot, LinkState};
use dashboard_host_validation::diagnostics::session::Counters;
use dashboard_host_validation::samples::{Quantity, Sample, Source};
use dashboard_host_validation::vehicle_status::VehicleStatus;

fn link(state: LinkState) -> LinkSnapshot {
    LinkSnapshot {
        state,
        last_fault: None,
        faults: 0,
        handshakes_started: 0,
        raw_records: 0,
        raw_overwrites: 0,
        diagnostics: Counters::default(),
    }
}

#[test]
fn startup_raw_and_recovery_states_never_claim_vehicle_data() {
    let samples = DashboardSnapshot::EMPTY;
    assert_eq!(VehicleStatus::derive(false, None, &samples, 0), VehicleStatus::Disabled);
    assert_eq!(VehicleStatus::derive(true, None, &samples, 0), VehicleStatus::Starting);
    for (state, expected) in [
        (LinkState::Recovering, VehicleStatus::Retrying),
        (LinkState::Quiet, VehicleStatus::Starting),
        (LinkState::Handshake, VehicleStatus::Starting),
        (LinkState::Unqualified, VehicleStatus::Raw),
        (LinkState::Polling, VehicleStatus::Waiting),
        (LinkState::Quarantined, VehicleStatus::ResetRequired),
        (LinkState::Maintenance, VehicleStatus::Maintenance),
    ] {
        assert_eq!(VehicleStatus::derive(true, Some(link(state)), &samples, 0), expected);
    }
    let demo = AcquisitionModel::demo().snapshot();
    assert_eq!(
        VehicleStatus::derive(false, Some(link(LinkState::Quarantined)), &demo, 0),
        VehicleStatus::Demo
    );
}

#[test]
fn actual_fresh_sample_is_required_and_staleness_changes_without_new_publication() {
    let mut model = AcquisitionModel::new();
    model
        .ingest_sample(
            Sample {
                value: Quantity::CoolantC(95.0),
                received_at_ms: 10,
                sequence: 1,
                source: Source::Obd { ecu: 0x7E8, pid: 5 },
            },
            10,
        )
        .unwrap();
    let mut snapshot = model.snapshot();
    let polling = Some(link(LinkState::Polling));
    assert_eq!(
        VehicleStatus::derive(true, polling, &snapshot, 3010),
        VehicleStatus::Data
    );
    assert_eq!(
        VehicleStatus::derive(true, polling, &snapshot, 3011),
        VehicleStatus::Stale
    );
    assert_eq!(
        VehicleStatus::derive(true, polling, &snapshot, 9),
        VehicleStatus::Waiting
    );
    snapshot.metadata[2].source = Source::Unknown;
    assert_eq!(
        VehicleStatus::derive(true, polling, &snapshot, 10),
        VehicleStatus::Waiting
    );
    assert_eq!(
        VehicleStatus::derive(true, Some(link(LinkState::Quarantined)), &snapshot, 10),
        VehicleStatus::ResetRequired
    );
}

#[test]
fn each_link_status_repairs_both_retained_headers_without_sensor_changes() {
    use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::render::FpsMode;
    use dashboard_host_validation::{AlignedFrame, colors};
    let samples = DashboardSnapshot::EMPTY;
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    for status in [
        VehicleStatus::Disabled,
        VehicleStatus::Starting,
        VehicleStatus::Raw,
        VehicleStatus::Waiting,
        VehicleStatus::Data,
        VehicleStatus::Stale,
        VehicleStatus::ResetRequired,
        VehicleStatus::Retrying,
        VehicleStatus::Maintenance,
        VehicleStatus::Starting,
    ] {
        let scene = DashboardScene {
            layout: None,
            measurements: &samples,
            status,
            now_ms: 0,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Instant,
            fps_instant: 40.0,
            fps_average: 40.0,
            backgrounds: [colors::BLACK; 8],
            overlay: None,
        };
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (buffer, frame) in frames.iter_mut().enumerate() {
            let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
            assert!(redraw.header);
            assert!(frame.0 == reference.0, "status {status:?}, buffer {buffer}");
        }
        assert!(!cache.prepare(0, &scene).header);
    }
}
