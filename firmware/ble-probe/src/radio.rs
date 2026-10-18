//! Core0 retains CYW bus ownership even if the host or an inspection fails.
use core::cell::Cell;
use core::future::Future;
use core::ops::ControlFlow;
use core::sync::atomic::Ordering;

use bt_hci::cmd::info::ReadBdAddr;
use bt_hci::param::{FilterDuplicates, LeAdvEventKind};
use dashboard_core::discovery::{
    Action,
    Advertisement,
    Characteristic,
    Event,
    InfoValue,
    InspectionItem,
    Payload,
    Peer,
    ProbeError,
    RadioPhase,
    ReadResult,
    ReportKind,
    Request,
    Service,
    Uuid as ProbeUuid,
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
struct Reports {
    revision: Cell<Option<u32>>,
}
impl EventHandler for Reports {
    fn on_adv_reports(
        &self,
        reports: LeAdvReportsIter<'_>,
    ) {
        let Some(revision) = self.revision.get() else { return };
        for report in reports {
            let Ok(report) = report else {
                let _ = bridge::RADIO_MALFORMED.try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                    Some(count.saturating_add(1))
                });
                continue;
            };
            if report.addr_kind != AddrKind::PUBLIC && report.addr_kind != AddrKind::RANDOM {
                continue;
            }
            let kind = match report.event_kind {
                LeAdvEventKind::AdvInd => ReportKind::Connectable,
                LeAdvEventKind::AdvDirectInd => ReportKind::Directed,
                LeAdvEventKind::ScanRsp => ReportKind::ScanResponse,
                _ => continue,
            };
            let mut address = [0; 6];
            address.copy_from_slice(report.addr.raw());
            address.reverse();
            let peer = Peer {
                address,
                random: report.addr_kind == AddrKind::RANDOM,
            };
            bridge::emit_report(Event::Report {
                revision,
                report: Advertisement::new(peer, kind, report.rssi, report.data, Instant::now().as_millis()),
            });
        }
    }
}
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
    let reports = Reports {
        revision: Cell::new(None),
    };
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
                reports.revision.set(Some(request.revision));
                state(request.revision, RadioPhase::Scanning, ProbeError::None).await;
                changed(request.revision).await;
                reports.revision.set(None);
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
fn map_error<E>(error: BleHostError<E>) -> ProbeError {
    match error {
        BleHostError::Controller(_) => ProbeError::Transport,
        BleHostError::BleHost(Error::Timeout) => ProbeError::Timeout,
        BleHostError::BleHost(Error::Disconnected) => ProbeError::Disconnected,
        BleHostError::BleHost(Error::InsufficientSpace | Error::OutOfMemory) => ProbeError::Capacity,
        _ => ProbeError::Discovery,
    }
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
            cleanup(stack, request.revision).await;
            state(request.revision, RadioPhase::Paused, ProbeError::Timeout).await;
            return;
        }
        Either::Second(Ok(Err(_))) => {
            cleanup(stack, request.revision).await;
            state(request.revision, RadioPhase::Paused, ProbeError::Connect).await;
            return;
        }
        Either::Second(Ok(Ok(connection))) => connection,
    };
    if connection.peer_address() != peers[0] {
        connection.disconnect();
        cleanup(stack, request.revision).await;
        state(request.revision, RadioPhase::Paused, ProbeError::InvalidPeer).await;
        return;
    }
    state(request.revision, RadioPhase::Discovering, ProbeError::None).await;
    let result = select(
        changed(request.revision),
        with_timeout(Duration::from_secs(30), inspect(stack, &connection, request.revision)),
    )
    .await;
    // Client/request futures are gone. Late ATT responses cannot become a new request.
    connection.disconnect();
    cleanup(stack, request.revision).await;
    match result {
        Either::First(()) => {
            defmt::info!("probe.radio stage=inspect-canceled revision={}", request.revision);
        }
        Either::Second(Err(_)) => state(request.revision, RadioPhase::Paused, ProbeError::Timeout).await,
        Either::Second(Ok(Err(error))) => state(request.revision, RadioPhase::Paused, error).await,
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
async fn att<T, E>(
    revision: u32,
    stage: &'static str,
    future: impl Future<Output = Result<T, BleHostError<E>>>,
) -> Result<T, ProbeError> {
    let started = Instant::now();
    let result = match with_timeout(Duration::from_secs(3), future).await {
        Ok(result) => result.map_err(map_error),
        Err(_) => Err(ProbeError::Timeout),
    };
    if let Err(error) = &result {
        defmt::warn!(
            "probe.radio stage={} revision={} error={} elapsed_ms={}",
            stage,
            revision,
            error_label(*error),
            Instant::now().duration_since(started).as_millis()
        );
    }
    result
}

async fn inspect(
    stack: &RadioStack<'_>,
    connection: &Connection<'_, Pool>,
    revision: u32,
) -> Result<bool, ProbeError> {
    let client: Client<'_> = att(revision, "mtu", GattClient::new(stack, connection)).await?;
    let work = async {
        match select(
            inventory(&client, revision),
            connection_events(stack, connection, revision),
        )
        .await
        {
            Either::First(result) => result,
            Either::Second(error) => Err(error),
        }
    };
    match select(work, client.task()).await {
        Either::First(result) => result,
        Either::Second(result) => Err(result.err().map(map_error).unwrap_or(ProbeError::Disconnected)),
    }
}
async fn connection_events(
    stack: &RadioStack<'_>,
    connection: &Connection<'_, Pool>,
    revision: u32,
) -> ProbeError {
    loop {
        match connection.next().await {
            ConnectionEvent::Disconnected { .. } => return ProbeError::Disconnected,
            ConnectionEvent::RequestConnectionParams(request) => {
                if let Err(error) = att(revision, "connection-parameters", request.reject(stack)).await {
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
) -> Result<bool, ProbeError> {
    let services = att(revision, "services", client.services()).await?;
    for service in &services {
        let range = service.handle_range();
        let uuid = ProbeUuid::from_wire(service.uuid().as_raw()).ok_or(ProbeError::Discovery)?;
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
        let service_uuid = ProbeUuid::from_wire(service.uuid().as_raw()).ok_or(ProbeError::Discovery)?;
        let mut rows: Vec<Characteristic, 64> = Vec::new();
        let mut malformed = false;
        let mut previous_value = start;
        let result = att(
            revision,
            "characteristics",
            client.read_by_type(start, end, &Uuid::new_short(0x2803), |declaration, bytes| {
                if !matches!(bytes.len(), 5 | 19) || declaration <= previous_value {
                    malformed = true;
                    return ControlFlow::Break(());
                }
                let handle = u16::from_le_bytes([bytes[1], bytes[2]]);
                if handle <= declaration || handle > end {
                    malformed = true;
                    return ControlFlow::Break(());
                }
                let Some(uuid) = ProbeUuid::from_wire(&bytes[3..]) else {
                    malformed = true;
                    return ControlFlow::Break(());
                };
                if total == 64 {
                    limited = true;
                    return ControlFlow::Break(());
                }
                previous_value = handle;
                let row = Characteristic {
                    service_start: start,
                    declaration,
                    handle,
                    properties: bytes[0],
                    uuid,
                };
                if rows.push(row).is_err() {
                    limited = true;
                    return ControlFlow::Break(());
                }
                total += 1;
                ControlFlow::Continue(())
            }),
        )
        .await;
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
        result?;
        if malformed {
            return Err(ProbeError::Discovery);
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
        let (result, value) = match with_timeout(Duration::from_secs(3), client.read_handle(handle, &mut bytes)).await {
            Err(_) => {
                defmt::warn!("probe.radio stage=device-info revision={} error=timeout", revision);
                return Err(ProbeError::Timeout);
            }
            Ok(Ok(length)) => {
                let mut value = Payload::new(&bytes[..length]);
                value.truncated = length == bytes.len();
                (ReadResult::Ok, value)
            }
            Ok(Err(BleHostError::BleHost(Error::Att(code)))) => {
                let protected = matches!(
                    code,
                    AttErrorCode::INSUFFICIENT_AUTHENTICATION
                        | AttErrorCode::INSUFFICIENT_ENCRYPTION
                        | AttErrorCode::INSUFFICIENT_ENCRYPTION_KEY_SIZE
                        | AttErrorCode::INSUFFICIENT_AUTHORISATION
                );
                (
                    if protected {
                        ReadResult::Protected
                    } else {
                        ReadResult::Unavailable
                    },
                    Payload::empty(),
                )
            }
            Ok(Err(error)) => {
                let error = map_error(error);
                defmt::warn!(
                    "probe.radio stage=device-info revision={} error={}",
                    revision,
                    error_label(error)
                );
                return Err(error);
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
