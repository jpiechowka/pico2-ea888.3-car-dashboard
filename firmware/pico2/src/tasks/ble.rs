//! Core0 owns radio DMA, HCI and GATT. Only bounded status leaves this task.
#[path = "ble_error.rs"]
/// Compact host-runner error formatting preserving HCI/decode distinctions.
mod ble_error;

use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use dashboard_pico2::ble::link::{LinkFault, LinkSnapshot, LinkState};
use dashboard_pico2::ble::runtime::{WriteMode, write_mode};
use dashboard_pico2::ble::session::{Command, Completion, Mode, QualifiedRead, Response, Session};
use dashboard_pico2::maintenance::{Command as MaintenanceCommand, Owner, Request};
use dashboard_pico2::view_config::{BleAddressKind, BleProfile, BleSettings};
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH1, PIN_23, PIN_24, PIN_25, PIN_29, PIO0};
use embassy_rp::{Peri, pac};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use static_cell::StaticCell;
use trouble_host::prelude::*;

/// External CYW HCI controller with five concurrent command slots.
type Controller = ExternalController<cyw43::bluetooth::BtDriver<'static>, 5>;
/// Compile-time bounded packet storage shared by this host stack.
type Pool = DefaultPacketPool;
/// Borrowed host stack used by the sole dashboard connection lifecycle.
type BleStack<'a> = Stack<'a, Controller, Pool>;
/// GATT client caching at most four services for the configured adapter profile.
type Client<'a> = GattClient<'a, Controller, Pool, 4>;

/// Coherent status/epoch/transaction counters; no vehicle sample data crosses here.
static STATUS: Mutex<CriticalSectionRawMutex, Cell<LinkSnapshot>> = Mutex::new(Cell::new(LinkSnapshot {
    state: LinkState::Disabled,
    epoch: 0,
    completed_transactions: 0,
    lost_notifications: 0,
    last_latency_ms: None,
    fault: None,
}));
/// Wrapping completed-radio-boundary liveness sequence, also advancing when disabled.
pub static PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Latched host/cleanup failure requiring independent watchdog recovery.
pub static FATAL: AtomicBool = AtomicBool::new(false);
/// Checked monotonic generation invalidating every session spanning flash parking.
static PARK_GENERATION: AtomicU32 = AtomicU32::new(0);
/// PIO/DMA initialization completed enough to require DMA1 self-chain admission.
static RADIO_STARTED: AtomicBool = AtomicBool::new(false);

/// Exclusive uninitialized Pico 2 W internal wireless tokens transferred to core0.
pub struct Resources {
    /// PIO0 owner taking SM0 and IRQ0 for CYW GSPI.
    pub pio: Peri<'static, PIO0>,
    /// DMA1 reserved for the radio, separate from display DMA0.
    pub dma: Peri<'static, DMA_CH1>,
    /// GP23 wireless enable, initially low.
    pub power: Peri<'static, PIN_23>,
    /// GP24 wireless data, not the non-W board VBUS input.
    pub data: Peri<'static, PIN_24>,
    /// GP25 SIO active-low wireless chip select.
    pub cs: Peri<'static, PIN_25>,
    /// GP29 wireless clock/shared VSYS pin; ADC cannot use it concurrently.
    pub clock: Peri<'static, PIN_29>,
}

/// Returns one coherent link snapshot without accessing hardware or sensor samples.
pub fn snapshot() -> LinkSnapshot { STATUS.lock(Cell::get) }

/// Updates `state` and optional `fault` coherently, clearing latency on faults.
/// Logs only changed state/fault pairs after releasing the lock; keeps epoch and
/// transaction/loss counters intact.
fn publish(
    state: LinkState,
    fault: Option<LinkFault>,
) {
    let changed = STATUS.lock(|cell| {
        let mut next = cell.get();
        let changed = next.state != state || next.fault != fault;
        next.state = state;
        next.fault = fault;
        if fault.is_some() {
            next.last_latency_ms = None;
        }
        cell.set(next);
        changed.then_some(next.epoch)
    });
    if let Some(epoch) = changed {
        dashboard_pico2::ble::link::log_transition(
            &crate::profiling::LOGS,
            Instant::now().as_millis(),
            state,
            fault,
            epoch,
        );
    }
}

/// Called only after CYW PIO commands returned, or before radio initialization.
/// GPIO25 is SIO-controlled CS. PIO owns GPIO24/29, never a flash DMA channel.
/// Returns whether CS is high and DMA1 idle with its expected self-chain setting
/// after startup; caller must already be at a completed bus boundary.
pub fn bus_idle() -> bool {
    pac::SIO.gpio_out(0).value().read() & (1 << 25) != 0
        && !pac::DMA.ch(1).ctrl_trig().read().busy()
        && (!RADIO_STARTED.load(Ordering::Relaxed) || pac::DMA.ch(1).ctrl_trig().read().chain_to() == 1)
}

/// Publishes radio progress, then parks/resumes the latest maintenance request.
/// `last_resume` suppresses repeats; each new request advances the checked park
/// generation before acknowledgement, invalidating live sessions even on rejected
/// saves. Never cancel here; exhaustion panics and IRQs stay available while parked.
async fn boundary(last_resume: &mut Option<Request>) {
    PROGRESS.fetch_add(1, Ordering::Relaxed);
    let Some(current) = super::maintenance::command() else {
        return;
    };
    let request = match current {
        MaintenanceCommand::Park(r) | MaintenanceCommand::Resume(r) => r,
    };
    if *last_resume == Some(request) {
        return;
    }
    // Invalidate every session spanning the pause, including rejected saves.
    let generation = PARK_GENERATION
        .load(Ordering::Relaxed)
        .checked_add(1)
        .expect("radio park identity exhausted");
    PARK_GENERATION.store(generation, Ordering::Relaxed);
    while super::maintenance::command() == Some(MaintenanceCommand::Park(request)) {
        let timely = Instant::now()
            .as_millis()
            .checked_sub(request.started_ms())
            .is_some_and(|ms| ms <= dashboard_pico2::maintenance::PREPARE_MS);
        if bus_idle() && super::maintenance::interrupt_ready() {
            super::maintenance::acknowledge(MaintenanceCommand::Park(request), Owner::Radio, timely);
        } else if !timely {
            super::maintenance::acknowledge(MaintenanceCommand::Park(request), Owner::Radio, false);
        }
        Timer::after_millis(1).await;
    }
    *last_resume = Some(request);
    super::maintenance::acknowledge(MaintenanceCommand::Resume(request), Owner::Radio, bus_idle());
}

/// Waits until `generation` differs from the current radio park generation,
/// polling each millisecond. This signals invalidation, not completed restoration.
async fn wait_park(generation: u32) {
    while PARK_GENERATION.load(Ordering::Relaxed) == generation {
        Timer::after_millis(1).await;
    }
}

#[embassy_executor::task]
/// Owns `resources` under immutable `settings` for the application lifetime.
/// Disabled settings retain pins and join maintenance without initializing CYW;
/// enabled settings join permanent bus/host owners. Never cancel DMA or the radio
/// runner. Unsupported clock or allocation invariants panic for watchdog recovery.
pub async fn radio_task(
    settings: BleSettings,
    resources: Resources,
) {
    let power = Output::new(resources.power, Level::Low);
    let cs = Output::new(resources.cs, Level::High);
    let mut last_resume = None;
    if !settings.enabled {
        // Retain disabled owners and still participate in flash coordination.
        let _pins = (power, cs);
        loop {
            boundary(&mut last_resume).await;
            Timer::after_millis(10).await;
        }
    }
    publish(LinkState::Starting, None);
    let mut pio = embassy_rp::pio::Pio::new(resources.pio, crate::Irqs);
    let divider = dashboard_pico2::ble::runtime::pio_divider(embassy_rp::clocks::clk_sys_freq())
        .expect("unsupported radio clock");
    crate::log_info!(
        "BLE PIO divider {}; GSPI {} Hz",
        divider,
        dashboard_pico2::ble::runtime::radio_bus_hz(embassy_rp::clocks::clk_sys_freq()).unwrap()
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
    RADIO_STARTED.store(true, Ordering::Relaxed);
    /// One-time CYW protocol and ring state retained by the permanent bus runner.
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let (_wifi, driver, _control, runner) = cyw43::new_with_bluetooth(
        STATE.init_with(cyw43::State::new),
        power,
        spi,
        cyw43::aligned_bytes!("../../../vendor/cyw43-firmware/43439A0.bin"),
        cyw43::aligned_bytes!("../../../vendor/cyw43-firmware/43439A0_btfw.bin"),
        cyw43::aligned_bytes!("../../../vendor/cyw43-firmware/nvram_rp2040.bin"),
    )
    .await;
    // Never cancel the radio runner. HCI errors leave it alive for safe park and
    // status reporting; only the hardware watchdog may reset a stalled DMA owner.
    join(
        runner.run_with_boundary(async || boundary(&mut last_resume).await),
        host(driver, settings),
    )
    .await;
}

/// Builds the bounded pairing-capable host around `driver` using `settings`.
/// Runs connection policy alongside HCI; terminal lifecycle faults retain HCI for
/// disconnect cleanup, and stopped/stalled cleanup latches fatal recovery.
/// Never returns normally or restarts a failed host in place.
async fn host(
    driver: cyw43::bluetooth::BtDriver<'static>,
    settings: BleSettings,
) {
    /// One connection, two channels and one volatile bond slot for the host.
    static RESOURCES: StaticCell<HostResources<Pool, 1, 2, 0, 1>> = StaticCell::new();
    let stack = trouble_host::new(
        ExternalController::<_, 5>::new(driver),
        RESOURCES.init_with(HostResources::new),
    )
    .set_io_capabilities(IoCapabilities::NoInputNoOutput)
    .build();
    let mut runner = stack.runner();
    let lifecycle = async {
        let fault = connections(&stack, settings).await;
        publish(LinkState::Fault, Some(fault));
        // Keep the HCI runner alive to complete queued cancellation/disconnect.
        // A stalled controller is not made healthy by continued CYW bus traffic.
        if with_timeout(Duration::from_secs(5), async {
            while !stack.connections_idle() {
                Timer::after_millis(5).await;
            }
        })
        .await
        .is_err()
        {
            crate::log_fault!("BLE controller cleanup stalled; watchdog recovery");
            FATAL.store(true, Ordering::Relaxed);
        }
        core::future::pending::<()>().await;
    };
    match select(runner.run(), lifecycle).await {
        Either::First(result) => crate::log_fault!("{}", ble_error::RunnerFault(&result)),
        Either::Second(()) => unreachable!(),
    }
    publish(LinkState::Fault, Some(LinkFault::Transport));
    FATAL.store(true, Ordering::Relaxed);
    core::future::pending::<()>().await;
}

/// Runs selected-peer connection epochs on `stack` using `settings`, invalidating
/// sessions on park and requiring actual disconnect/idle before another attempt.
/// Returns a terminal fault when reconnect is disabled, cleanup fails or epoch
/// space exhausts. Automatic retries back off 1..10 seconds; they never qualify
/// ECU measurements or persist volatile bonds.
async fn connections(
    stack: &BleStack<'_>,
    settings: BleSettings,
) -> LinkFault {
    let mut address = settings.peer.address;
    address.reverse(); // Config/display order is opposite the HCI wire order.
    let peer = Address::new(
        match settings.peer.kind {
            BleAddressKind::Public => AddrKind::PUBLIC,
            BleAddressKind::RandomStatic => AddrKind::RANDOM,
        },
        BdAddr::new(address),
    );
    let peers = [peer];
    let connect = ConnectConfig {
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
            min_event_length: Duration::from_millis(0),
            max_event_length: Duration::from_millis(0),
            supervision_timeout: Duration::from_secs(4),
        },
    };
    let mode = if settings.profile == BleProfile::TestPeer {
        Mode::SyntheticPeer
    } else {
        Mode::AdapterOnly
    };
    let mut session = Session::new(mode);
    let mut central = stack.central();
    let mut epoch = 0_u64;
    let mut backoff_ms = 1000;
    loop {
        let generation = PARK_GENERATION.load(Ordering::Relaxed);
        let Some(next) = epoch.checked_add(1) else {
            return LinkFault::Exhausted;
        };
        epoch = next;
        STATUS.lock(|cell| {
            let mut next = cell.get();
            next.epoch = epoch;
            cell.set(next);
        });
        publish(LinkState::Connecting, None);
        let result = select(
            wait_park(generation),
            with_timeout(
                Duration::from_millis(u64::from(settings.connect_timeout_ms)),
                central.connect(&connect),
            ),
        )
        .await;
        let fault = match result {
            Either::First(()) => None,
            Either::Second(Err(_)) => Some(LinkFault::Timeout),
            Either::Second(Ok(Err(_))) => Some(LinkFault::Transport),
            Either::Second(Ok(Ok(connection))) => {
                if connection.peer_address() != peer {
                    return LinkFault::Configuration;
                }
                let outcome = select(
                    wait_park(generation),
                    connected(stack, &connection, settings, &mut session, epoch, generation),
                )
                .await;
                if matches!(snapshot().state, LinkState::AdapterReady | LinkState::SyntheticReady) {
                    backoff_ms = 1000;
                }
                session.invalidate();
                connection.disconnect();
                // Drop all GATT work, then require actual HCI disconnect before a
                // new epoch. No transaction runs over an ambiguous old stream.
                if with_timeout(Duration::from_secs(5), async {
                    // The event task may already have consumed Disconnected.
                    // A disconnect request alone is not controller completion.
                    while !connection.is_disconnected() {
                        Timer::after_millis(5).await;
                    }
                })
                .await
                .is_err()
                {
                    return LinkFault::Transport;
                }
                let reason = connection.disconnect_reason().map(|code| code.into_inner());
                let local_error = connection.local_disconnect_error().is_some();
                dashboard_pico2::ble::link::log_disconnect(
                    &crate::profiling::LOGS,
                    Instant::now().as_millis(),
                    epoch,
                    reason,
                    local_error,
                );
                dashboard_pico2::ble::link::closed_fault(
                    PARK_GENERATION.load(Ordering::Relaxed) != generation,
                    match outcome {
                        Either::First(()) => None,
                        Either::Second(fault) => Some(fault),
                    },
                    reason,
                    local_error,
                )
            }
        };
        session.invalidate();
        // A canceled Create Connection can still complete successfully. Wait for
        // the host's quarantine/disconnect before admitting another attempt.
        if with_timeout(Duration::from_secs(5), async {
            while !stack.connections_idle() {
                Timer::after_millis(5).await;
            }
        })
        .await
        .is_err()
        {
            FATAL.store(true, Ordering::Relaxed);
            return LinkFault::Transport;
        }
        if fault.is_none() {
            publish(LinkState::Maintenance, None);
            while super::maintenance::command().is_some() {
                Timer::after_millis(1).await;
            }
            continue;
        }
        if !settings.auto_reconnect {
            return fault.unwrap();
        }
        publish(LinkState::Backoff, fault);
        Timer::after_millis(backoff_ms).await;
        backoff_ms = (backoff_ms * 2).min(10_000);
    }
}

/// Services `connection` on `stack` using `settings`, mutable parser `session`,
/// current `epoch` and flash `generation`. CX requires encryption before discovery;
/// event processing remains active during security/MTU/setup. Returns the first
/// terminal link fault and drops losing client work for caller-owned disconnect.
async fn connected(
    stack: &BleStack<'_>,
    connection: &Connection<'_, Pool>,
    settings: BleSettings,
    session: &mut Session,
    epoch: u64,
    generation: u32,
) -> LinkFault {
    let service = async {
        if settings.profile == BleProfile::ObdLinkCx {
            publish(LinkState::Securing, None);
            connection.set_bondable(true).map_err(|_| LinkFault::Security)?;
            if !connection
                .security_level()
                .map_err(|_| LinkFault::Security)?
                .encrypted()
            {
                connection.request_security().map_err(|_| LinkFault::Security)?;
            }
            with_timeout(Duration::from_millis(u64::from(settings.connect_timeout_ms)), async {
                loop {
                    if connection
                        .security_level()
                        .map_err(|_| LinkFault::Security)?
                        .encrypted()
                    {
                        // The linked stack excludes legacy pairing. LE Secure
                        // Connections uses a 128-bit key; do not enable legacy.
                        return Ok(());
                    }
                    Timer::after_millis(5).await;
                }
            })
            .await
            .map_err(|_| LinkFault::Security)??;
        }
        publish(LinkState::Discovering, None);
        let client = with_timeout(
            Duration::from_millis(u64::from(settings.connect_timeout_ms)),
            Client::new(stack, connection),
        )
        .await
        .map_err(|_| LinkFault::Timeout)?
        .map_err(|_| LinkFault::Discovery)?;
        match select(
            client.task(),
            serial(&client, connection, settings, session, epoch, generation),
        )
        .await
        {
            Either::First(_) => Err(LinkFault::Transport),
            Either::Second(result) => result,
        }
    };
    match select(connection_events(stack, connection), service).await {
        Either::First(fault) => fault,
        Either::Second(Err(fault)) => fault,
        Either::Second(Ok(())) => LinkFault::Transport,
    }
}

/// Consumes `connection` events using `stack`, returning transport/security faults.
/// Stores successful bonds only in RAM; rejects user-interaction pairing and
/// parameter requests outside zero latency and <=50 ms maximum interval.
async fn connection_events(
    stack: &BleStack<'_>,
    connection: &Connection<'_, Pool>,
) -> LinkFault {
    loop {
        match connection.next().await {
            ConnectionEvent::Disconnected { .. } => return LinkFault::Transport,
            ConnectionEvent::PairingFailed(_) | ConnectionEvent::BondLost => return LinkFault::Security,
            ConnectionEvent::PassKeyInput
            | ConnectionEvent::PassKeyConfirm(_)
            | ConnectionEvent::PassKeyDisplay(_)
            | ConnectionEvent::OobRequest => return LinkFault::Security,
            ConnectionEvent::PairingComplete { bond: Some(bond), .. } => {
                if stack.add_bond_information(bond).is_err() {
                    return LinkFault::Security;
                }
            }
            ConnectionEvent::RequestConnectionParams(request) => {
                // Preserve the latency budget instead of accepting arbitrary
                // long peripheral latency. Rejected peers may disconnect.
                if request.params().is_valid()
                    && request.params().max_latency == 0
                    && request.params().max_connection_interval <= Duration::from_millis(50)
                {
                    if request.accept(None, stack).await.is_err() {
                        return LinkFault::Transport;
                    }
                } else if request.reject(stack).await.is_err() {
                    return LinkFault::Transport;
                }
            }
            ConnectionEvent::ConnectionParamsUpdated {
                conn_interval,
                peripheral_latency,
                ..
            } => {
                crate::log_info!(
                    "BLE interval {} us, latency {}",
                    conn_interval.as_micros(),
                    peripheral_latency
                );
            }
            _ => {}
        }
    }
}

/// Returns service, RX-notify and TX-write UUIDs for `profile`; TestPeer uses a
/// distinct private 128-bit namespace and cannot select a real adapter service.
fn uuids(profile: BleProfile) -> (Uuid, Uuid, Uuid) {
    match profile {
        BleProfile::VlinkerMcPlus => {
            (
                Uuid::new_short(0x18F0),
                Uuid::new_short(0x2AF0),
                Uuid::new_short(0x2AF1),
            )
        }
        BleProfile::ObdLinkCx => {
            (
                Uuid::new_short(0xFFF0),
                Uuid::new_short(0xFFF1),
                Uuid::new_short(0xFFF2),
            )
        }
        BleProfile::TestPeer => {
            // Private test UUIDs, LE byte order. Never match a real ELM service.
            let mut uuid = [
                0x01, 0x00, 0xC8, 0x0E, 0x88, 0xEA, 0x52, 0xA8, 0x2F, 0x43, 0xC2, 0xAE, 0x00, 0x00, 0x88, 0xEA,
            ];
            let service = Uuid::new_long(uuid);
            uuid[12] = 1;
            let rx = Uuid::new_long(uuid);
            uuid[12] = 2;
            (service, rx, Uuid::new_long(uuid))
        }
    }
}

/// Discovers one profile service through `client` on `connection`, validates
/// properties from `settings`, then runs `session` under `epoch`/`generation`.
/// Returns discovery/security/protocol/transport failure; normal operation loops
/// forever. MC+/CX send initialization and periodic identity only; TestPeer reads
/// synthetic PID5 without publishing vehicle channels. Notifications are subscribed
/// only after the exact profile's RX/TX properties pass admission.
async fn serial<'stack>(
    client: &Client<'stack>,
    connection: &Connection<'stack, Pool>,
    settings: BleSettings,
    session: &mut Session,
    epoch: u64,
    generation: u32,
) -> Result<(), LinkFault> {
    let (service, rx, tx) = uuids(settings.profile);
    let discovery = async {
        let services = client
            .services_by_uuid(&service)
            .await
            .map_err(|_| LinkFault::Discovery)?;
        if services.len() != 1 {
            return Err(LinkFault::Discovery);
        }
        let rx = client
            .characteristic_by_uuid::<[u8]>(&services[0], &rx)
            .await
            .map_err(|_| LinkFault::Discovery)?;
        let tx = client
            .characteristic_by_uuid::<[u8]>(&services[0], &tx)
            .await
            .map_err(|_| LinkFault::Discovery)?;
        let mode = write_mode(
            settings.profile,
            rx.props.any(&[CharacteristicProp::Notify]),
            tx.props.any(&[CharacteristicProp::Write]),
            tx.props.any(&[CharacteristicProp::WriteWithoutResponse]),
        )
        .ok_or(LinkFault::Discovery)?;
        let listener = client.subscribe(&rx, false).await.map_err(|_| LinkFault::Discovery)?;
        Ok((tx, listener, mode))
    };
    let (tx, listener, mode) = with_timeout(Duration::from_millis(u64::from(settings.connect_timeout_ms)), discovery)
        .await
        .map_err(|_| LinkFault::Timeout)??;
    session
        .connect(epoch, Instant::now().as_millis())
        .map_err(LinkFault::Protocol)?;
    let mut serial = Serial {
        client,
        connection,
        tx,
        mode,
        listener,
        settings,
        session,
        epoch,
        generation,
        sequence: 0,
    };
    publish(LinkState::Identifying, None);
    while let Some(init) = serial.session.next_initialization() {
        serial.transact(Command::Initialize(init)).await?;
        serial.gap(u64::from(settings.poll_gap_ms)).await?;
    }
    serial.transact(Command::Identity).await?;
    let synthetic = settings.profile == BleProfile::TestPeer;
    publish(
        if synthetic {
            LinkState::SyntheticReady
        } else {
            LinkState::AdapterReady
        },
        None,
    );
    loop {
        serial
            .gap(if synthetic {
                u64::from(settings.poll_gap_ms).max(100)
            } else {
                2000
            })
            .await?;
        let command = if synthetic {
            Command::Read(QualifiedRead::synthetic_obd(5).map_err(LinkFault::Protocol)?)
        } else {
            Command::Identity
        };
        serial.transact(command).await?;
    }
}

/// One connected ELM/test-peer stream with generation, sequence and parser admission.
struct Serial<'a, 'stack> {
    /// Borrowed live GATT client whose runner is concurrently polled.
    client: &'a Client<'stack>,
    /// Connection checked before each transaction/chunk/notification acceptance.
    connection: &'a Connection<'stack, Pool>,
    /// Profile-qualified write characteristic; never restored from saved handles.
    tx: Characteristic<[u8]>,
    /// Profile-admitted ATT write request or command mode.
    mode: WriteMode,
    /// Bounded RX notification subscription with explicit lost-message reporting.
    listener: NotificationListener<'a, 244>,
    /// Immutable profile/deadline/poll-gap configuration for this connection.
    settings: BleSettings,
    /// Exclusive transaction parser; invalidated before connection replacement.
    session: &'a mut Session,
    /// Checked connection identity attached to every parser notification.
    epoch: u64,
    /// Flash-park generation captured when this stream was created.
    generation: u32,
    /// Checked per-stream notification sequence; exhaustion fails closed.
    sequence: u64,
}
impl Serial<'_, '_> {
    /// Returns success only for the current park generation, connected bearer
    /// and (for CX) retained encryption; otherwise returns Transport/Security.
    fn admission(&self) -> Result<(), LinkFault> {
        if PARK_GENERATION.load(Ordering::Relaxed) != self.generation {
            return Err(LinkFault::Transport);
        }
        if !self.connection.is_connected() {
            return Err(LinkFault::Transport);
        }
        if self.settings.profile == BleProfile::ObdLinkCx
            && !self
                .connection
                .security_level()
                .map_err(|_| LinkFault::Security)?
                .encrypted()
        {
            return Err(LinkFault::Security);
        }
        Ok(())
    }

    /// Admits `notification`, accounting reported queue losses and checked
    /// sequence before feeding the parser. Returns a matched completion, None for
    /// an incomplete response, or generation/security/loss/protocol failure.
    fn receive(
        &mut self,
        notification: Result<Notification<244>, u64>,
    ) -> Result<Option<Completion>, LinkFault> {
        self.admission()?;
        let notification = notification.map_err(|count| {
            STATUS.lock(|cell| {
                let mut next = cell.get();
                next.lost_notifications = next.lost_notifications.saturating_add(count);
                cell.set(next);
            });
            LinkFault::NotificationLoss
        })?;
        self.sequence = self.sequence.checked_add(1).ok_or(LinkFault::Exhausted)?;
        self.session
            .notify(
                self.epoch,
                self.sequence,
                notification.as_ref(),
                Instant::now().as_millis(),
            )
            .map_err(LinkFault::Protocol)
    }

    /// Waits `ms` milliseconds while consuming notifications through admission.
    /// Returns success at deadline or first receive fault; does not open another
    /// transaction, and yields after each notification to bound cooperative work.
    async fn gap(
        &mut self,
        ms: u64,
    ) -> Result<(), LinkFault> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        loop {
            match select(Timer::at(deadline), self.listener.next_checked()).await {
                Either::First(()) => return Ok(()),
                Either::Second(notification) => {
                    self.receive(notification)?;
                }
            }
            embassy_futures::yield_now().await;
        }
    }

    /// Executes allowed `command` after checking at most four queued notifications.
    /// Sends <=20-byte chunks under one response deadline, then awaits the exact
    /// parser completion. Returns admission/loss/transport/protocol/timeout faults;
    /// partial writes cannot be retried on this stream, so caller disconnects it.
    async fn transact(
        &mut self,
        command: Command,
    ) -> Result<(), LinkFault> {
        self.admission()?;
        // Reject queued unsolicited responses before opening a new transaction.
        // Inspect at most the four queued notifications, then fail closed on more.
        for index in 0..=4 {
            match select(self.listener.next_checked(), core::future::ready(())).await {
                Either::First(notification) => {
                    if index == 4 {
                        return Err(LinkFault::NotificationLoss);
                    }
                    self.receive(notification)?;
                }
                Either::Second(()) => break,
            }
        }
        let transmission = self
            .session
            .begin(
                command,
                Instant::now().as_millis(),
                u64::from(self.settings.response_timeout_ms),
            )
            .map_err(LinkFault::Protocol)?;
        let timeout = Duration::from_millis(u64::from(self.settings.response_timeout_ms));
        let operation = async {
            // 20-byte chunks work at the mandatory 23-byte ATT MTU. A CR is sent
            // only in the final chunk. MC+ command-only peers have no ATT ACK;
            // completion still requires the exact ELM response and final prompt.
            for chunk in transmission.bytes.chunks(20) {
                self.admission()?;
                match self.mode {
                    WriteMode::Request => self.client.write_characteristic(&self.tx, chunk).await,
                    WriteMode::Command => self.client.write_characteristic_without_response(&self.tx, chunk).await,
                }
                .map_err(|_| LinkFault::Transport)?;
            }
            self.admission()?;
            self.session
                .sent(transmission.token, Instant::now().as_millis())
                .map_err(LinkFault::Protocol)?;
            loop {
                let notification = self.listener.next_checked().await;
                if let Some(done) = self.receive(notification)? {
                    completed(&done)?;
                    return Ok(());
                }
            }
        };
        with_timeout(timeout, operation).await.map_err(|_| LinkFault::Timeout)?
    }
}

/// Validates `done` response class and records checked transaction count/latency.
/// Returns protocol, exhausted-counter or clock-conversion failure without
/// publishing sensor values. Session completion guarantees ordered millisecond times.
fn completed(done: &Completion) -> Result<(), LinkFault> {
    if !matches!(
        done.response,
        Response::Ok | Response::Identity(_) | Response::Diagnostic(_)
    ) {
        return Err(LinkFault::Protocol(dashboard_pico2::ble::session::Error::Reply));
    }
    STATUS.lock(|cell| {
        let mut next = cell.get();
        next.completed_transactions = next.completed_transactions.checked_add(1).ok_or(LinkFault::Exhausted)?;
        next.last_latency_ms = Some(u32::try_from(done.completed_ms - done.started_ms).map_err(|_| LinkFault::Clock)?);
        cell.set(next);
        Ok(())
    })
}
