//! Core0 retains CYW bus ownership even if the host or an inspection fails.
use core::cell::Cell;
use core::future::Future;
use core::ops::ControlFlow;
use core::pin::{Pin, pin};
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll};

use bt_hci::cmd::info::ReadBdAddr;
use bt_hci::param::FilterDuplicates;
use dashboard_core::discovery::{
    Action,
    ConnectionFailureContext,
    Event,
    FailureReason,
    FailureStage,
    InspectionFailure,
    InspectionItem,
    LinkInfo,
    Peer,
    ProbeError,
    RadioPhase,
    Request,
    Service,
    Uuid as ProbeUuid,
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
use crate::errors::failure_reason;
type RadioController = ExternalController<cyw43::bluetooth::BtDriver<'static>, 5>;
type Pool = DefaultPacketPool;
type RadioStack<'a> = Stack<'a, RadioController, Pool>;
type Client<'a> = GattClient<'a, RadioController, Pool, 24>;
static BUS_IDLE: AtomicBool = AtomicBool::new(false);
pub fn bus_idle() -> bool { BUS_IDLE.load(Ordering::SeqCst) }
pub struct Resources {
    pub pio: Peri<'static, PIO0>,
    pub dma: Peri<'static, DMA_CH1>,
    pub power: Peri<'static, PIN_23>,
    pub data: Peri<'static, PIN_24>,
    pub cs: Peri<'static, PIN_25>,
    pub clock: Peri<'static, PIN_29>,
}
// Keep the radio poll path separate from executor/metrics frames. Borrowing the
// pinned owner avoids constructing or moving its large future at this boundary.
#[inline(never)]
pub fn poll_owner<F: Future>(
    future: Pin<&mut F>,
    cx: &mut Context<'_>,
) -> Poll<F::Output> {
    future.poll(cx)
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
    defmt::info!(
        "Probe BLE PIO divider {}; GSPI {} Hz",
        divider,
        dashboard_core::ble::runtime::radio_bus_hz(embassy_rp::clocks::clk_sys_freq()).unwrap()
    );
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
    let mut maintenance_request = None;
    let bus = pin!(runner.run_with_boundary(async || {
        bridge::RADIO_PROGRESS.fetch_add(1, Ordering::Relaxed);
        BUS_IDLE.store(true, Ordering::SeqCst);
        crate::maintenance::radio_boundary(&mut maintenance_request).await;
        BUS_IDLE.store(false, Ordering::SeqCst);
    }));
    let protocol = pin!(host(driver));
    join(bus, protocol).await;
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
    let active_inspection = Cell::new(None);
    let outcome = {
        let hci = pin!(runner.run_with_handler(&reports));
        let work = pin!(operations(&stack, &reports, &active_inspection));
        select(hci, work).await
    };
    match outcome {
        Either::First(result) => {
            defmt::warn!("Probe HCI runner stopped: {:?}", defmt::Debug2Format(&result));
            if let Some(revision) = active_inspection.get() {
                let reason = result.err().map(failure_reason).unwrap_or(FailureReason::Disconnected);
                failed(revision, InspectionFailure::new(FailureStage::ClientTask, reason)).await;
                // A stopped host cannot acknowledge controller cleanup.
                bridge::emit_event(Event::InspectionEnd {
                    revision,
                    at_ms: Instant::now().as_millis(),
                    cleanup_complete: false,
                    disconnect_reason: None,
                })
                .await;
            }
            fault(ProbeError::Transport).await
        }
        Either::Second(never) => never,
    }
}
async fn cleanup(
    stack: &RadioStack<'_>,
    revision: u32,
    connection: Option<&Connection<'_, Pool>>,
    active_inspection: &Cell<Option<u32>>,
) {
    state(revision, RadioPhase::Disconnecting, ProbeError::None).await;
    let complete = crate::session::finish_cleanup(
        revision,
        async {
            with_timeout(Duration::from_secs(5), async {
                while !stack.connections_idle() {
                    Timer::after_millis(10).await;
                }
            })
            .await
            .is_ok()
        },
        || Instant::now().as_millis(),
        || {
            connection
                .and_then(|connection| connection.disconnect_reason())
                .map(|reason| reason.into_inner())
        },
        bridge::emit_event,
    )
    .await;
    active_inspection.set(None);
    if !complete {
        fault(ProbeError::Cleanup).await;
    }
}
async fn operations(
    stack: &RadioStack<'_>,
    reports: &Reports,
    active_inspection: &Cell<Option<u32>>,
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
                active_inspection.set(Some(request.revision));
                inspect_peer(stack, &mut central, request, peer, active_inspection).await;
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
async fn failure_context(
    connection: &Connection<'_, Pool>,
    revision: u32,
) {
    let context = ConnectionFailureContext {
        local_error: connection
            .local_disconnect_error()
            .map(|error| failure_reason::<()>(BleHostError::BleHost(error))),
        disconnect_reason: connection.disconnect_reason().map(|code| code.into_inner()),
    };
    bridge::emit_event(Event::FailureContext { revision, context }).await;
}
async fn inspect_peer(
    stack: &RadioStack<'_>,
    central: &mut Central<'_, RadioController, Pool>,
    request: Request,
    peer: Peer,
    active_inspection: &Cell<Option<u32>>,
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
    let connected = {
        let connect = pin!(central.connect(&config));
        select(changed(request.revision), with_timeout(Duration::from_secs(8), connect)).await
    };
    // The pinned connect operation has been dropped before cleanup can wait.
    let connection = match connected {
        Either::First(()) => {
            defmt::info!("probe.radio stage=connect-canceled revision={}", request.revision);
            cleanup(stack, request.revision, None, active_inspection).await;
            return;
        }
        Either::Second(Err(_)) => {
            let mut failure = InspectionFailure::new(FailureStage::Connect, FailureReason::Timeout);
            failure.elapsed_ms = Some(elapsed_ms(connect_started));
            failed(request.revision, failure).await;
            cleanup(stack, request.revision, None, active_inspection).await;
            state(request.revision, RadioPhase::Paused, ProbeError::Timeout).await;
            return;
        }
        Either::Second(Ok(Err(error))) => {
            let mut failure = InspectionFailure::new(FailureStage::Connect, failure_reason(error));
            failure.elapsed_ms = Some(elapsed_ms(connect_started));
            failed(request.revision, failure).await;
            cleanup(stack, request.revision, None, active_inspection).await;
            state(request.revision, RadioPhase::Paused, ProbeError::Connect).await;
            return;
        }
        Either::Second(Ok(Ok(connection))) => connection,
    };
    // Freeze link timing before any bounded event-channel publication can wait.
    let connected_at_ms = Instant::now().as_millis();
    let connect_elapsed_ms = elapsed_ms(connect_started);
    if connection.peer_address() != peers[0] {
        connection.disconnect();
        failed(
            request.revision,
            InspectionFailure::new(FailureStage::Connect, FailureReason::PeerMismatch),
        )
        .await;
        cleanup(stack, request.revision, Some(&connection), active_inspection).await;
        state(request.revision, RadioPhase::Paused, ProbeError::InvalidPeer).await;
        return;
    }
    state(request.revision, RadioPhase::Discovering, ProbeError::None).await;
    link_info(
        &connection,
        request.revision,
        connected_at_ms,
        connect_elapsed_ms,
        false,
    )
    .await;
    let inspection_started = Instant::now();
    let mtu_ready = Cell::new(false);
    let result = {
        let work = pin!(inspect(
            stack,
            &connection,
            request.revision,
            connected_at_ms,
            connect_elapsed_ms,
            &mtu_ready,
        ));
        select(changed(request.revision), with_timeout(Duration::from_secs(30), work)).await
    };
    // Client/request futures are gone. Late ATT responses cannot become a new request.
    connection.disconnect();
    // Final observed parameters, including updates delivered during inventory.
    // This is a local snapshot, not another controller or ATT request.
    link_info(
        &connection,
        request.revision,
        connected_at_ms,
        connect_elapsed_ms,
        mtu_ready.get(),
    )
    .await;
    match &result {
        Either::Second(Err(_)) => {
            let mut failure = InspectionFailure::new(FailureStage::Inspection, FailureReason::Timeout);
            failure.elapsed_ms = Some(elapsed_ms(inspection_started));
            failed(request.revision, failure).await;
        }
        Either::Second(Ok(Err(failure))) => failed(request.revision, *failure).await,
        _ => {}
    }
    let has_failure = matches!(&result, Either::Second(Err(_)) | Either::Second(Ok(Err(_))));
    if has_failure {
        // Retain local ingress evidence even if terminal cleanup later stalls.
        failure_context(&connection, request.revision).await;
    }
    cleanup(stack, request.revision, Some(&connection), active_inspection).await;

    if has_failure {
        // HCI terminal reason may arrive after the operation's first failure.
        failure_context(&connection, request.revision).await;
    }
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
    connected_at_ms: u64,
    connect_elapsed_ms: u32,
    mtu_ready: &Cell<bool>,
) -> Result<bool, InspectionFailure> {
    crate::session::inspect_with_events(
        pin!(att(FailureStage::Mtu, pin!(Client::new(stack, connection)))),
        |client| {
            async move {
                mtu_ready.set(true);
                link_info(connection, revision, connected_at_ms, connect_elapsed_ms, true).await;
                let outcome = {
                    let work = pin!(crate::inventory::inventory(&client, revision, &bridge::emit_event));
                    let task = pin!(client.task());
                    select(work, task).await
                };
                match outcome {
                    Either::First(result) => result,
                    Either::Second(result) => {
                        Err(InspectionFailure::new(
                            FailureStage::ClientTask,
                            result.err().map(failure_reason).unwrap_or(FailureReason::Disconnected),
                        ))
                    }
                }
            }
        },
        pin!(connection_events(stack, connection)),
    )
    .await
}
async fn link_info(
    connection: &Connection<'_, Pool>,
    revision: u32,
    connected_at_ms: u64,
    connect_elapsed_ms: u32,
    mtu_ready: bool,
) {
    let params = connection.params();
    bridge::emit_event(Event::Item {
        revision,
        item: InspectionItem::Link(LinkInfo {
            connected_at_ms,
            connect_elapsed_ms,
            interval_us: params.conn_interval.as_micros().min(u64::from(u32::MAX)) as u32,
            latency: params.peripheral_latency,
            supervision_timeout_us: params.supervision_timeout.as_micros().min(u64::from(u32::MAX)) as u32,
            att_mtu: mtu_ready.then(|| connection.att_mtu()),
        }),
    })
    .await;
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

impl crate::inventory::ServiceRecord for ServiceHandle {
    fn snapshot(&self) -> Result<Service, FailureReason> {
        let range = self.handle_range();
        let uuid = ProbeUuid::from_wire(self.uuid().as_raw()).ok_or(FailureReason::InvalidValue)?;
        Ok(Service {
            start: *range.start(),
            end: *range.end(),
            uuid,
        })
    }
}
impl crate::inventory::InventoryClient for Client<'_> {
    type Service = ServiceHandle;

    async fn services(
        &self,
        output: &mut Vec<ServiceHandle, 24>,
    ) -> Result<(), FailureReason> {
        match Client::services(self).await {
            Ok(services) => {
                *output = services;
                Ok(())
            }
            Err(error) => Err(failure_reason(error)),
        }
    }

    async fn read_by_type(
        &self,
        start: u16,
        end: u16,
        uuid: &Uuid,
        callback: impl FnMut(u16, &[u8]) -> ControlFlow<()>,
    ) -> Result<Option<()>, FailureReason> {
        Client::read_by_type(self, start, end, uuid, callback)
            .await
            .map_err(failure_reason)
    }

    async fn find_information(
        &self,
        start: u16,
        end: u16,
        callback: impl FnMut(u16, Uuid) -> ControlFlow<()>,
    ) -> Result<Option<()>, FailureReason> {
        Client::find_information(self, start, end, callback)
            .await
            .map_err(failure_reason)
    }

    async fn read_handle(
        &self,
        handle: u16,
        bytes: &mut [u8],
    ) -> Result<usize, FailureReason> {
        Client::read_handle(self, handle, bytes).await.map_err(failure_reason)
    }
}
