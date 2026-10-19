//! Core0 retains CYW bus ownership even if the host or an inspection fails.
use core::future::Future;
use core::ops::ControlFlow;
use core::sync::atomic::Ordering;

use bt_hci::cmd::info::ReadBdAddr;
use bt_hci::param::FilterDuplicates;
use dashboard_core::discovery::{
    Action,
    Characteristic,
    Event,
    FailureReason,
    FailureStage,
    InfoValue,
    InspectionFailure,
    InspectionItem,
    Payload,
    Peer,
    ProbeError,
    RadioPhase,
    ReadResult,
    Request,
    Service,
    Uuid as ProbeUuid,
    characteristic_from_wire,
    may_read,
};
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_rp::Peri;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH1, PIN_23, PIN_24, PIN_25, PIN_29, PIO0};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use heapless::Vec;
use static_cell::StaticCell;
use trouble_host::prelude::*;

use crate::bridge;
use crate::errors::{device_info_att_result, failure_reason};
type RadioController = ExternalController<cyw43::bluetooth::BtDriver<'static>, 5>;
type Pool = DefaultPacketPool;
type RadioStack<'a> = Stack<'a, RadioController, Pool>;
type Client<'a> = GattClient<'a, RadioController, Pool, 24>;
pub struct Resources {
    pub pio: Peri<'static, PIO0>,
    pub dma: Peri<'static, DMA_CH1>,
    pub power: Peri<'static, PIN_23>,
    pub data: Peri<'static, PIN_24>,
    pub cs: Peri<'static, PIN_25>,
    pub clock: Peri<'static, PIN_29>,
}
use crate::reports::Reports;
async fn state(
    revision: u32,
    phase: RadioPhase,
    error: ProbeError,
) {
    let stage = match phase {
        RadioPhase::Starting => "starting",
        RadioPhase::Scanning => "scanning",
        RadioPhase::Paused => "paused",
        RadioPhase::Stopping => "stopping",
        RadioPhase::Connecting => "connecting",
        RadioPhase::Discovering => "discovering",
        RadioPhase::Disconnecting => "disconnecting",
        RadioPhase::Fault => "fault",
    };
    if error == ProbeError::None {
        defmt::info!(
            "probe.radio phase={} revision={} at_ms={}",
            stage,
            revision,
            Instant::now().as_millis()
        );
    } else {
        defmt::warn!(
            "probe.radio phase={} revision={} error={} at_ms={}",
            stage,
            revision,
            error_label(error),
            Instant::now().as_millis()
        );
    }
    bridge::emit_event(Event::State { revision, phase, error }).await;
}
async fn changed(revision: u32) {
    while bridge::request().revision == revision {
        Timer::after_millis(10).await;
    }
}
async fn fault(error: ProbeError) -> ! {
    bridge::RADIO_FATAL.store(true, Ordering::Release);
    bridge::RADIO_READY.store(false, Ordering::Release);
    state(bridge::request().revision, RadioPhase::Fault, error).await;
    core::future::pending().await
}
pub async fn run(resources: Resources) -> ! {
    let power = Output::new(resources.power, Level::Low);
    let cs = Output::new(resources.cs, Level::High);
    let mut pio = embassy_rp::pio::Pio::new(resources.pio, crate::Irqs);
    let Some(divider) = dashboard_core::ble::runtime::pio_divider(embassy_rp::clocks::clk_sys_freq()) else {
        fault(ProbeError::Transport).await;
    };
    let spi = cyw43_pio::PioSpi::new(
        &mut pio.common,
        pio.sm0,
        divider.into(),
        pio.irq0,
        cs,
        resources.data,
        resources.clock,
        embassy_rp::dma::Channel::new(resources.dma, crate::Irqs),
    );
    static RADIO: StaticCell<cyw43::State> = StaticCell::new();
    // Neither initialization nor the DMA-owning runner may be canceled.
    let (_wifi, driver, _control, runner) = cyw43::new_with_bluetooth(
        RADIO.init_with(cyw43::State::new),
        power,
        spi,
        cyw43::aligned_bytes!("../../vendor/cyw43-firmware/43439A0.bin"),
        cyw43::aligned_bytes!("../../vendor/cyw43-firmware/43439A0_btfw.bin"),
        cyw43::aligned_bytes!("../../vendor/cyw43-firmware/nvram_rp2040.bin"),
    )
    .await;
    join(
        runner.run_with_boundary(async || {
            bridge::RADIO_PROGRESS.fetch_add(1, Ordering::Relaxed);
        }),
        host(driver),
    )
    .await;
    unreachable!()
}
async fn host(driver: cyw43::bluetooth::BtDriver<'static>) -> ! {
    static RESOURCES: StaticCell<HostResources<Pool, 1, 2>> = StaticCell::new();
    let stack = trouble_host::new(
        ExternalController::<_, 5>::new(driver),
        RESOURCES.init_with(HostResources::new),
    )
    .build();
    let reports = Reports::new();
    let mut runner = stack.runner();
    match select(runner.run_with_handler(&reports), operations(&stack, &reports)).await {
        Either::First(_) => {
            defmt::warn!("Probe HCI runner stopped");
            fault(ProbeError::Transport).await
        }
        Either::Second(never) => never,
    }
}
async fn cleanup(
    stack: &RadioStack<'_>,
    revision: u32,
) {
    state(revision, RadioPhase::Disconnecting, ProbeError::None).await;
    if with_timeout(Duration::from_secs(5), async {
        while !stack.connections_idle() {
            Timer::after_millis(10).await;
        }
    })
    .await
    .is_err()
    {
        fault(ProbeError::Cleanup).await;
    }
}
async fn operations(
    stack: &RadioStack<'_>,
    reports: &Reports,
) -> ! {
    // command() waits for initialized state and proves HCI response flow.
    match with_timeout(Duration::from_secs(10), stack.command(ReadBdAddr::new())).await {
        Ok(Ok(_)) => bridge::RADIO_READY.store(true, Ordering::Release),
        _ => fault(ProbeError::Transport).await,
    }
    let mut central = stack.central();
    loop {
        let request = bridge::request();
        match request.action {
            Action::Scan(false) => {
                state(request.revision, RadioPhase::Paused, ProbeError::None).await;
                changed(request.revision).await;
            }
            Action::Scan(true) => {
                let config = ScanConfig {
                    active: true,
                    phys: PhySet::M1,
                    interval: Duration::from_millis(100),
                    window: Duration::from_millis(50),
                    timeout: Duration::from_secs(0),
                    filter_duplicates: FilterDuplicates::Disabled,
                    filter_accept_list: &[],
                };
                let mut scanner = Scanner::new(&mut central);
                // A canceled start cannot expose a stop-ack handle. Fail closed
                // if start fails; a successful start always receives stop().await.
                let session = match with_timeout(Duration::from_secs(3), scanner.scan(&config)).await {
                    Ok(Ok(session)) => session,
                    _ => fault(ProbeError::Scan).await,
                };
                reports.set_revision(Some(request.revision));
                state(request.revision, RadioPhase::Scanning, ProbeError::None).await;
                changed(request.revision).await;
                reports.set_revision(None);
                state(bridge::request().revision, RadioPhase::Stopping, ProbeError::None).await;
                if with_timeout(Duration::from_secs(5), session.stop()).await.is_err() {
                    fault(ProbeError::Cleanup).await;
                }
            }
            Action::Inspect(peer) => {
                inspect_peer(stack, &mut central, request, peer).await;
                // Successful, canceled and failed inspections never auto-retry.
                changed(request.revision).await;
            }
        }
    }
}
async fn failed(
    revision: u32,
    failure: InspectionFailure,
) {
    defmt::warn!(
        "probe.failure revision={} stage={} reason={} code={:?} range={:?} last_accepted={:?} elapsed_ms={:?}",
        revision,
        failure.stage.label(),
        failure.reason.label(),
        failure.reason.code(),
        failure.range,
        failure.last_accepted,
        failure.elapsed_ms
    );
    bridge::emit_event(Event::Failure { revision, failure }).await;
}
async fn inspect_peer(
    stack: &RadioStack<'_>,
    central: &mut Central<'_, RadioController, Pool>,
    request: Request,
    peer: Peer,
) {
    let mut address = peer.address;
    address.reverse();
    let peers = [Address::new(
        if peer.random {
            AddrKind::RANDOM
        } else {
            AddrKind::PUBLIC
        },
        BdAddr::new(address),
    )];
    let config = ConnectConfig {
        scan_config: ScanConfig {
            filter_accept_list: &peers,
            interval: Duration::from_millis(100),
            window: Duration::from_millis(50),
            ..Default::default()
        },
        connect_params: RequestedConnParams {
            min_connection_interval: Duration::from_millis(15),
            max_connection_interval: Duration::from_millis(30),
            max_latency: 0,
            supervision_timeout: Duration::from_secs(4),
            min_event_length: Duration::from_secs(0),
            max_event_length: Duration::from_secs(0),
        },
    };
    state(request.revision, RadioPhase::Connecting, ProbeError::None).await;
    let connect_started = Instant::now();
    let connection = match select(
        changed(request.revision),
        with_timeout(Duration::from_secs(8), central.connect(&config)),
    )
    .await
    {
        Either::First(()) => {
            defmt::info!("probe.radio stage=connect-canceled revision={}", request.revision);
            cleanup(stack, request.revision).await;
            return;
        }
        Either::Second(Err(_)) => {
            let mut failure = InspectionFailure::new(FailureStage::Connect, FailureReason::Timeout);
            failure.elapsed_ms = Some(elapsed_ms(connect_started));
            failed(request.revision, failure).await;
            cleanup(stack, request.revision).await;
            state(request.revision, RadioPhase::Paused, ProbeError::Timeout).await;
            return;
        }
        Either::Second(Ok(Err(error))) => {
            let mut failure = InspectionFailure::new(FailureStage::Connect, failure_reason(error));
            failure.elapsed_ms = Some(elapsed_ms(connect_started));
            failed(request.revision, failure).await;
            cleanup(stack, request.revision).await;
            state(request.revision, RadioPhase::Paused, ProbeError::Connect).await;
            return;
        }
        Either::Second(Ok(Ok(connection))) => connection,
    };
    if connection.peer_address() != peers[0] {
        connection.disconnect();
        failed(
            request.revision,
            InspectionFailure::new(FailureStage::Connect, FailureReason::PeerMismatch),
        )
        .await;
        cleanup(stack, request.revision).await;
        state(request.revision, RadioPhase::Paused, ProbeError::InvalidPeer).await;
        return;
    }
    state(request.revision, RadioPhase::Discovering, ProbeError::None).await;
    let inspection_started = Instant::now();
    let result = select(
        changed(request.revision),
        with_timeout(Duration::from_secs(30), inspect(stack, &connection, request.revision)),
    )
    .await;
    // Client/request futures are gone. Late ATT responses cannot become a new request.
    connection.disconnect();
    match &result {
        Either::Second(Err(_)) => {
            let mut failure = InspectionFailure::new(FailureStage::Inspection, FailureReason::Timeout);
            failure.elapsed_ms = Some(elapsed_ms(inspection_started));
            failed(request.revision, failure).await;
        }
        Either::Second(Ok(Err(failure))) => failed(request.revision, *failure).await,
        _ => {}
    }
    cleanup(stack, request.revision).await;
    match result {
        Either::First(()) => {
            defmt::info!("probe.radio stage=inspect-canceled revision={}", request.revision);
        }
        Either::Second(Err(_)) => state(request.revision, RadioPhase::Paused, ProbeError::Timeout).await,
        Either::Second(Ok(Err(failure))) => state(request.revision, RadioPhase::Paused, failure.error()).await,
        Either::Second(Ok(Ok(limited))) => {
            bridge::emit_event(Event::Complete {
                revision: request.revision,
                limited,
            })
            .await;
            state(
                request.revision,
                RadioPhase::Paused,
                if limited {
                    ProbeError::Capacity
                } else {
                    ProbeError::None
                },
            )
            .await;
        }
    }
}
fn error_label(error: ProbeError) -> &'static str {
    match error {
        ProbeError::None => "none",
        ProbeError::Scan => "scan",
        ProbeError::Connect => "connect",
        ProbeError::Timeout => "timeout",
        ProbeError::Discovery => "discovery",
        ProbeError::Capacity => "capacity",
        ProbeError::Disconnected => "disconnected",
        ProbeError::Cleanup => "cleanup",
        ProbeError::Transport => "transport",
        ProbeError::InvalidPeer => "invalid-peer",
    }
}
fn elapsed_ms(started: Instant) -> u32 {
    Instant::now()
        .duration_since(started)
        .as_millis()
        .min(u64::from(u32::MAX)) as u32
}
async fn att<T, E>(
    stage: FailureStage,
    future: impl Future<Output = Result<T, BleHostError<E>>>,
) -> Result<T, InspectionFailure> {
    let started = Instant::now();
    let result = match with_timeout(Duration::from_secs(3), future).await {
        Ok(result) => result.map_err(failure_reason),
        Err(_) => Err(FailureReason::Timeout),
    };
    result.map_err(|reason| {
        let mut failure = InspectionFailure::new(stage, reason);
        failure.elapsed_ms = Some(elapsed_ms(started));
        failure
    })
}

async fn inspect(
    stack: &RadioStack<'_>,
    connection: &Connection<'_, Pool>,
    revision: u32,
) -> Result<bool, InspectionFailure> {
    let client: Client<'_> = att(FailureStage::Mtu, GattClient::new(stack, connection)).await?;
    let work = async {
        match select(inventory(&client, revision), connection_events(stack, connection)).await {
            Either::First(result) => result,
            Either::Second(error) => Err(error),
        }
    };
    match select(work, client.task()).await {
        Either::First(result) => result,
        Either::Second(result) => {
            Err(InspectionFailure::new(
                FailureStage::ClientTask,
                result.err().map(failure_reason).unwrap_or(FailureReason::Disconnected),
            ))
        }
    }
}
async fn connection_events(
    stack: &RadioStack<'_>,
    connection: &Connection<'_, Pool>,
) -> InspectionFailure {
    loop {
        match connection.next().await {
            ConnectionEvent::Disconnected { reason } => {
                return InspectionFailure::new(
                    FailureStage::ConnectionEvents,
                    FailureReason::DisconnectCode(reason.into_inner()),
                );
            }
            ConnectionEvent::RequestConnectionParams(request) => {
                if let Err(error) = att(FailureStage::ConnectionEvents, request.reject(stack)).await {
                    return error;
                }
            }
            _ => {}
        }
    }
}
async fn inventory(
    client: &Client<'_>,
    revision: u32,
) -> Result<bool, InspectionFailure> {
    let services = att(FailureStage::Services, client.services()).await?;
    for service in &services {
        let range = service.handle_range();
        let uuid = ProbeUuid::from_wire(service.uuid().as_raw())
            .ok_or_else(|| InspectionFailure::new(FailureStage::Services, FailureReason::InvalidValue))?;
        bridge::emit_event(Event::Item {
            revision,
            item: InspectionItem::Service(Service {
                start: *range.start(),
                end: *range.end(),
                uuid,
            }),
        })
        .await;
    }
    let mut total = 0usize;
    let mut limited = false;
    let mut readable: Vec<(u16, u16), 8> = Vec::new();
    for service in &services {
        if total == 64 {
            limited = true;
            break;
        }
        let range = service.handle_range();
        let start = *range.start();
        let end = *range.end();
        let service_uuid = ProbeUuid::from_wire(service.uuid().as_raw())
            .ok_or_else(|| InspectionFailure::new(FailureStage::Characteristics, FailureReason::InvalidValue))?;
        let mut rows: Vec<Characteristic, 64> = Vec::new();
        let mut malformed = None;
        let mut previous_value = start;
        let mut last_accepted = None;
        let started = Instant::now();
        let result = att(
            FailureStage::Characteristics,
            client.read_by_type(start, end, &Uuid::new_short(0x2803), |declaration, bytes| {
                let row = match characteristic_from_wire(start, end, previous_value, declaration, bytes) {
                    Ok(row) => row,
                    Err(reason) => {
                        malformed = Some(reason);
                        return ControlFlow::Break(());
                    }
                };
                if total == 64 {
                    limited = true;
                    return ControlFlow::Break(());
                }
                if rows.push(row).is_err() {
                    limited = true;
                    return ControlFlow::Break(());
                }
                previous_value = row.handle;
                last_accepted = Some((declaration, row.handle));
                total += 1;
                ControlFlow::Continue(())
            }),
        )
        .await;
        let query_elapsed_ms = elapsed_ms(started);
        // Keep valid rows preceding a malformed response or the capacity boundary.
        for row in rows {
            if may_read(service_uuid, row.uuid, row.properties)
                && let Some(uuid) = row.uuid.short()
                && !readable.iter().any(|(seen, _)| *seen == uuid)
                && readable.push((uuid, row.handle)).is_err()
            {
                limited = true;
            }
            bridge::emit_event(Event::Item {
                revision,
                item: InspectionItem::Characteristic(row),
            })
            .await;
        }
        // Attach application query context without guessing the hidden ATT page cursor.
        result.map_err(|mut failure| {
            failure.range = Some((start, end));
            failure.last_accepted = last_accepted;
            failure
        })?;
        if let Some(reason) = malformed {
            let mut failure = InspectionFailure::new(FailureStage::Characteristics, reason);
            failure.range = Some((start, end));
            failure.last_accepted = last_accepted;
            failure.elapsed_ms = Some(query_elapsed_ms);
            return Err(failure);
        }
    }
    defmt::info!(
        "probe.radio inventory revision={} services={} characteristics={} info_fields={} limited={}",
        revision,
        services.len(),
        total,
        readable.len(),
        limited
    );
    for (uuid, handle) in readable {
        let mut bytes = [0u8; 64];
        let started = Instant::now();
        let failure = |reason| {
            let mut failure = InspectionFailure::new(FailureStage::DeviceInfo, reason);
            failure.range = Some((handle, handle));
            failure.elapsed_ms = Some(elapsed_ms(started));
            failure
        };
        let (result, value) = match with_timeout(Duration::from_secs(3), client.read_handle(handle, &mut bytes)).await {
            Err(_) => {
                return Err(failure(FailureReason::Timeout));
            }
            Ok(Ok(length)) => {
                let mut value = Payload::new(&bytes[..length]);
                value.truncated = length == bytes.len();
                (ReadResult::Ok, value)
            }
            Ok(Err(BleHostError::BleHost(Error::Att(code)))) => (device_info_att_result(code), Payload::empty()),
            Ok(Err(error)) => {
                return Err(failure(failure_reason(error)));
            }
        };
        bridge::emit_event(Event::Item {
            revision,
            item: InspectionItem::Value(InfoValue { uuid, value, result }),
        })
        .await;
    }
    Ok(limited)
}
