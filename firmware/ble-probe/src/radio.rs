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
/// External HCI controller with five command slots over the retained CYW BT driver.
pub(crate) type RadioController = ExternalController<cyw43::bluetooth::BtDriver<'static>, 5>;
/// Compile-time bounded Trouble packet pool used by this image.
pub(crate) type Pool = DefaultPacketPool;
/// Borrowed host stack with one admitted connection and bounded channels.
pub(crate) type RadioStack<'a> = Stack<'a, RadioController, Pool>;
/// GATT inventory client with at most 24 cached services.
pub(crate) type Client<'a> = GattClient<'a, RadioController, Pool, 24>;
/// True only inside a completed CYW transaction boundary, including parked time.
static BUS_IDLE: AtomicBool = AtomicBool::new(false);
/// Returns whether the retained bus owner is at its quiescent maintenance hook;
/// callers still require the shared hardware/IRQ checks before flash mutation.
pub fn bus_idle() -> bool { BUS_IDLE.load(Ordering::SeqCst) }
/// Uninitialized core0 tokens for the Pico 2 W internal wireless bus.
pub struct Resources {
    /// PIO0 token; this owner takes SM0 and IRQ0.
    pub pio: Peri<'static, PIO0>,
    /// DMA1 reserved for radio bus transfers, distinct from display DMA0.
    pub dma: Peri<'static, DMA_CH1>,
    /// GP23 wireless enable, initially driven low.
    pub power: Peri<'static, PIN_23>,
    /// GP24 bidirectional wireless data, never a non-W VBUS input.
    pub data: Peri<'static, PIN_24>,
    /// GP25 active-low wireless chip select, initially high.
    pub cs: Peri<'static, PIN_25>,
    /// GP29 wireless clock/shared VSYS pin; no concurrent ADC use.
    pub clock: Peri<'static, PIN_29>,
}
// Keep the radio poll path separate from executor/metrics frames. Borrowing the
// pinned owner avoids constructing or moving its large future at this boundary.
#[inline(never)]
/// Polls already-pinned `future` with `cx` and returns its unchanged Poll.
/// The non-inlined boundary preserves named stack admission without moving or
/// materializing another large radio owner future.
pub fn poll_owner<F: Future>(
    future: Pin<&mut F>,
    cx: &mut Context<'_>,
) -> Poll<F::Output> {
    future.poll(cx)
}
use crate::reports::Reports;
/// Logs and reliably queues `phase`/`error` under `revision`; queue backpressure
/// may suspend publication but does not itself change the hardware operation.
pub(crate) async fn state(
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
/// Waits until the UI operation no longer has `revision`, checking every 10 ms.
/// This is generation cancellation evidence, not a radio cleanup acknowledgement.
pub(crate) async fn changed(revision: u32) {
    while bridge::request().revision == revision {
        Timer::after_millis(10).await;
    }
}
/// Never returns: latches fatal radio state for `error`, publishes it, then waits forever.
/// Clears ready and preserves the outer CYW owner until independent watchdog reset.
pub(crate) async fn fault(error: ProbeError) -> ! {
    latch_fault();
    state(bridge::request().revision, RadioPhase::Fault, error).await;
    core::future::pending().await
}
/// Latches fatal radio visibility before reliable publication can wait for queue capacity.
/// The caller must retain the operation/resource owners and enter the terminal fault path.
pub(crate) fn latch_fault() { bridge::latch_radio_fault(); }
/// Consumes core0 radio `resources`, initializes CYW and joins the permanent bus
/// runner with host work. Never returns/cancels DMA; fatal host outcomes retain
/// this bus owner. Boundary hooks publish progress and park only after completed
/// PIO/DMA transactions. Invalid clock admission waits for watchdog recovery.
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
    /// One-time CYW protocol/ring state, retained with the permanent runner.
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
/// Never returns: builds the bounded no-pairing host around `driver` and runs operation
/// supervision forever. A stopped HCI runner records active inspection failure
/// and incomplete cleanup before fatal recovery; no automatic host restart.
async fn host(driver: cyw43::bluetooth::BtDriver<'static>) -> ! {
    /// Single host resource pool with one connection and two L2CAP channels.
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
        let hci = pin!(crate::session::latch_completion(
            runner.run_with_handler(&reports),
            latch_fault
        ));
        let work = pin!(operations(&stack, &reports, &active_inspection));
        select(hci, work).await
    };
    match outcome {
        Either::First(result) => {
            defmt::warn!("Probe HCI runner stopped: {:?}", result);
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
/// Waits up to five seconds for `stack` connections to become idle, publishes
/// terminal evidence for `revision` from optional `connection`, then clears
/// `active_inspection`. A missed acknowledgement enters nonreturning fatal state;
/// queued disconnect alone is not completion and cleanup never starts another peer.
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
        || connection.map(connection_context),
        bridge::emit_event,
    )
    .await;
    active_inspection.set(None);
    if !complete {
        fault(ProbeError::Cleanup).await;
    }
}
/// Never returns: runs latest explicit UI operations on `stack`, setting `reports` only during
/// an acknowledged scan and tracking `active_inspection` for host-failure evidence.
/// Startup requires ReadBdAddr within ten seconds; scan start/stop and cleanup
/// have deadlines. Inspections run once per generation and never auto-retry.
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
                let Some(lease) = bridge::OPERATION_GATE.connection() else {
                    // A previously admitted journal mutation may span several
                    // page/restoration cycles. Re-read generation before retry.
                    Timer::after_millis(5).await;
                    continue;
                };
                active_inspection.set(Some(request.revision));
                let mut inspection = pin!(crate::inspect_selected_peer(
                    stack,
                    &mut central,
                    request,
                    peer,
                    active_inspection,
                ));
                // Retain the same pinned owner, with a bounded poll-call boundary.
                core::future::poll_fn(|cx| poll_owner(inspection.as_mut(), cx)).await;
                drop(lease);
                // Successful, canceled and failed inspections never auto-retry.
                changed(request.revision).await;
            }
        }
    }
}
/// Logs bounded `failure` context and reliably publishes it under `revision`;
/// retained partial inventory is left intact and no retry/security escalation occurs.
pub(crate) async fn failed(
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
/// Publishes retained local/controller termination evidence from `connection`
/// under `revision` before cleanup, preserving it even if later cleanup stalls.
async fn failure_context(
    connection: &Connection<'_, Pool>,
    revision: u32,
) {
    bridge::emit_event(Event::FailureContext {
        revision,
        context: connection_context(connection),
    })
    .await;
}
/// Returns currently retained local failure and controller disconnect reason from
/// `connection`; absence stays unavailable and no new HCI/ATT request is issued.
pub(crate) fn connection_context(connection: &Connection<'_, Pool>) -> ConnectionFailureContext {
    ConnectionFailureContext {
        local_error: connection
            .local_disconnect_error()
            .map(|error| failure_reason::<()>(BleHostError::BleHost(error))),
        disconnect_reason: connection.disconnect_reason().map(|code| code.into_inner()),
    }
}
/// Connects `central` only to selected `peer` for `request` on `stack`, with
/// `active_inspection` retained until acknowledged cleanup. Eight-second connect
/// and 30-second inventory bounds race generation change; losing futures drop
/// before cleanup. Rejects peer mismatch and always disconnects after inventory.
/// Valid partial results remain evidence; completion does not imply proprietary access.
/// Only the standard binary aliases this entrypoint, even with identity features.
/// The identity binary aliases its own inspector without an alternate async state.
pub async fn inspect_peer(
    stack: &RadioStack<'_>,
    central: &mut Central<'_, RadioController, Pool>,
    request: Request,
    peer: Peer,
    active_inspection: &Cell<Option<u32>>,
) {
    let peers = [selected_address(peer)];
    let config = selected_config(&peers);
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

    // Cleanup publishes late ingress and HCI reason even after successful work.
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
/// Returns selected `peer` in the controller's little-endian address representation,
/// preserving its public/random type. No address is recovered from names or saved handles.
pub(crate) fn selected_address(peer: Peer) -> Address {
    let mut address = peer.address;
    address.reverse();
    Address::new(
        if peer.random {
            AddrKind::RANDOM
        } else {
            AddrKind::PUBLIC
        },
        BdAddr::new(address),
    )
}

/// Returns the common bounded connection timing/scan policy restricted to `peers`.
/// Caller retains the address slice through connection completion or cancellation.
pub(crate) fn selected_config(peers: &[Address]) -> ConnectConfig<'_> {
    ConnectConfig {
        scan_config: ScanConfig {
            filter_accept_list: peers,
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
    }
}
/// Returns a static bounded stage label for `error` without formatting payload data.
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
/// Returns elapsed milliseconds since monotonic `started`, saturated to u32::MAX.
pub(crate) fn elapsed_ms(started: Instant) -> u32 {
    Instant::now()
        .duration_since(started)
        .as_millis()
        .min(u64::from(u32::MAX)) as u32
}
/// Awaits `future` for three seconds and returns its value or mapped failure
/// carrying `stage` and elapsed milliseconds. Timeout drops the ATT future;
/// enclosing inspection must clean up this bearer rather than issuing another request.
pub(crate) async fn att<T, E>(
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

/// Starts a GATT client on `stack`/`connection` while connection events remain
/// polled, then enumerates under `revision`. `connected_at_ms`/`connect_elapsed_ms`
/// retain pre-publication timing; `mtu_ready` latches only after client startup.
/// Returns capacity-limited flag or stage failure; drops losing work before the
/// caller requests disconnect. No pairing or proprietary values are requested.
async fn inspect(
    stack: &RadioStack<'_>,
    connection: &Connection<'_, Pool>,
    revision: u32,
    connected_at_ms: u64,
    connect_elapsed_ms: u32,
    mtu_ready: &Cell<bool>,
) -> Result<bool, InspectionFailure> {
    let mut client_slot = None;
    crate::session::inspect_with_events(
        pin!(att(
            FailureStage::Mtu,
            pin!(Client::new_into(stack, connection, &mut client_slot))
        )),
        async |client| {
            let client = *client;
            mtu_ready.set(true);
            link_info(connection, revision, connected_at_ms, connect_elapsed_ms, true).await;
            let outcome = {
                let work = pin!(crate::inventory::inventory(client, revision, &bridge::emit_event));
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
        },
        pin!(connection_events(stack, connection)),
    )
    .await
}
/// Publishes local `connection` parameters under `revision`, retaining supplied
/// `connected_at_ms`/`connect_elapsed_ms`. `mtu_ready` controls whether ATT MTU is
/// available; timing fields saturate to u32 microseconds and issue no radio request.
pub(crate) async fn link_info(
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
/// Consumes events from `connection`, rejects parameter-change requests through
/// `stack`, and returns the first disconnect or event-operation failure. Runs
/// during MTU startup as well as inventory; does not request pairing.
pub(crate) async fn connection_events(
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
    /// Returns copied inclusive service bounds and validated UUID; unsupported
    /// UUID bytes return InvalidValue without a new ATT operation.
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
    /// Locked-client service handle retaining bounded range/UUID metadata.
    type Service = ServiceHandle;

    /// Discovers directly into `output`, clearing it on first poll. Returns unit
    /// after complete discovery or a mapped host failure; failed/canceled output
    /// may retain an unpublished prefix. Inventory publishes only after success.
    async fn services(
        &self,
        output: &mut Vec<ServiceHandle, 24>,
    ) -> Result<(), FailureReason> {
        Client::services_into(self, output).await.map_err(failure_reason)
    }

    /// Enumerates `uuid` in inclusive `start`..=`end` through the locked client,
    /// invoking `callback` on ordered validated tuples. Returns callback-break
    /// Some or normal None, preserving exact bounded host failure classification.
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

    /// Enumerates inclusive `start`..=`end`, invoking `callback` for each
    /// validated handle/UUID. Returns Some on callback break, None at normal end,
    /// or a mapped host error; accepted callback prefixes are not rolled back.
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

    /// Reads `handle` into `bytes`; returns copied count no greater than its
    /// capacity or mapped host failure. Locked-client blobs preserve bounded copies.
    async fn read_handle(
        &self,
        handle: u16,
        bytes: &mut [u8],
    ) -> Result<usize, FailureReason> {
        Client::read_handle(self, handle, bytes).await.map_err(failure_reason)
    }
}
