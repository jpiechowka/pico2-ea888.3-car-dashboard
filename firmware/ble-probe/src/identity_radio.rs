//! Identity-image-only adapter wiring. One explicit peer, one INFO SDU, normal link cleanup.

use core::cell::{Cell, RefCell};
use core::pin::pin;

use dashboard_core::ble::canyonero::{
    CleanupResult,
    Epoch,
    Event as SessionEvent,
    InfoRequest,
    Outcome,
    Session,
    TransportDomain,
    TransportFault,
};
use dashboard_core::discovery::identity::{CANDIDATE_SPSM, ChannelParameters, Update};
use dashboard_core::discovery::{
    Event,
    FailureReason,
    FailureStage,
    InspectionFailure,
    InspectionItem,
    Peer,
    ProbeError,
    RadioPhase,
    Request,
};
use embassy_futures::select::{Either, select};
use embassy_time::{Instant, Timer};
use trouble_host::prelude::*;

use crate::bridge;
use crate::identity_flow::{self as flow, Clock, FreshEndpoints, Publication, Transport};
use crate::radio::{self, Client, Pool, RadioController, RadioStack};

/// Hardware monotonic timer, independent of rendered frame count.
pub struct Monotonic;

/// Borrowed attempt state retained by the outer lifecycle across canceled futures.
struct Attempt<'a> {
    /// Single-flight session including first fault and sole raw receive buffer.
    session: &'a mut Session,
    /// Exact owner token required by every completion.
    epoch: Epoch,
    /// Reliable publication offsets surviving cancellation.
    publication: &'a mut Publication,
}

impl Clock for Monotonic {
    /// Returns boot-relative hardware monotonic milliseconds.
    fn now_ms(&self) -> u64 { Instant::now().as_millis() }

    /// Waits until absolute monotonic `deadline_ms`, returning immediately if already past.
    async fn until(
        &self,
        deadline_ms: u64,
    ) {
        Timer::at(Instant::from_millis(deadline_ms)).await;
    }
}

impl flow::VendorClient for Client<'_> {
    /// Reads fresh `handle` into bounded `bytes`; returns copied length or exact classified error.
    /// Full output is conservatively marked truncated by the portable flow.
    async fn read(
        &self,
        handle: u16,
        bytes: &mut [u8],
    ) -> Result<usize, FailureReason> {
        Client::read_handle(self, handle, bytes)
            .await
            .map_err(crate::errors::failure_reason)
    }
}

/// One channel owner tied to the selected connection and originating stack.
struct Channel<'a, 's, 'c> {
    /// Live host stack; no foreign-stack handles can enter this adapter.
    stack: &'a RadioStack<'s>,
    /// Selected connection retained by the outer lifecycle through cleanup.
    connection: &'a Connection<'c, Pool>,
    /// Sole channel owner, absent before successful open or after explicit drop.
    channel: Option<L2capChannel<'a, Pool>>,
}

impl Drop for Channel<'_, '_, '_> {
    /// Requests channel close before releasing its sole handle; link cleanup is still required.
    fn drop(&mut self) {
        if let Some(channel) = self.channel.as_mut() {
            channel.disconnect();
        }
    }
}

/// Returns exact L2CAP refusal code from `error`, or its retained ATT/HCI/local classification.
/// `partial_send` records possible request submission; no retry is inferred from the error.
fn transport_error<E>(
    error: BleHostError<E>,
    partial_send: bool,
) -> TransportFault {
    match error {
        BleHostError::BleHost(trouble_host::Error::L2capConnectError(code)) => {
            TransportFault {
                domain: TransportDomain::L2cap,
                code: Some(code as u16),
                partial_send,
            }
        }
        other => flow::transport_fault(crate::errors::failure_reason(other), partial_send),
    }
}

impl Transport for Channel<'_, '_, '_> {
    /// Complete pool-owned SDU, with no intermediate copied receive buffer.
    type Sdu = Sdu<<Pool as trouble_host::PacketPool>::Packet>;

    /// Opens only configured candidate129; returns local/peer limits and unavailable CIDs/credits.
    async fn open(&mut self) -> Result<ChannelParameters, TransportFault> {
        if self.channel.is_some() {
            return Err(flow::transport_fault(FailureReason::InvalidState, false));
        }
        let config = L2capChannelConfig {
            mtu: Some(245),
            mps: Some(247),
            initial_credits: Some(1),
            flow_policy: CreditFlowPolicy::Every(1),
        };
        let channel = L2capChannel::create(self.stack, self.connection, CANDIDATE_SPSM, &config)
            .await
            .map_err(|error| transport_error(error, false))?;
        let parameters = ChannelParameters {
            local_mtu: Some(channel.mtu()),
            local_mps: Some(channel.mps()),
            peer_mtu: Some(channel.peer_mtu()),
            peer_mps: Some(channel.peer_mps()),
            ..flow::offered_parameters()
        };
        self.channel = Some(channel);
        Ok(parameters)
    }

    /// Submits the four INFO bytes in `request` as one SDU, returning local completion only.
    async fn send(
        &mut self,
        request: InfoRequest,
    ) -> Result<(), TransportFault> {
        let channel = self
            .channel
            .as_mut()
            .ok_or_else(|| flow::transport_fault(FailureReason::InvalidState, false))?;
        channel
            .send(self.stack, request.bytes())
            .await
            .map_err(|error| transport_error(error, true))
    }

    /// Returns one owned complete SDU, released by the flow before cross-core publication waits.
    async fn receive(&mut self) -> Result<Self::Sdu, TransportFault> {
        let channel = self
            .channel
            .as_mut()
            .ok_or_else(|| flow::transport_fault(FailureReason::InvalidState, false))?;
        channel
            .receive_sdu(self.stack)
            .await
            .map_err(|error| transport_error(error, false))
    }
}

/// Returns the lazy queue send for identity `update` under selected `revision`.
/// No publication occurs before polling. Callers bound active-operation waits
/// and retain final cleanup publication until queue acceptance.
fn emit(
    revision: u32,
    update: Update,
) -> impl core::future::Future<Output = ()> {
    bridge::emit_event(Event::Item {
        revision,
        item: InspectionItem::Identity(update),
    })
}

/// Runs one selected `peer` for `request` on `stack`/`central`, retaining `active_inspection`
/// through cleanup and terminal publication. Session begins before connect; active I/O
/// races generation cancellation and uses fixed stage deadlines. Terminal evidence
/// publication is reliable and may wait for queue capacity after the cleanup deadline.
/// No retry, alternate PSM,
/// pairing, voltage, RPC or automotive command is available on this route.
pub async fn inspect_peer(
    stack: &RadioStack<'_>,
    central: &mut Central<'_, RadioController, Pool>,
    request: Request,
    peer: Peer,
    active_inspection: &Cell<Option<u32>>,
) {
    let clock = Monotonic;
    let epoch = Epoch {
        connection: u64::from(request.revision),
        operation: u64::from(request.revision),
        maintenance: 0,
    };
    let mut session = Session::new();
    let started = clock.now_ms();
    let mut publication = Publication::new(started);
    if session.start(epoch, started).is_err() {
        radio::failed(
            request.revision,
            InspectionFailure::new(FailureStage::Connect, FailureReason::InvalidState),
        )
        .await;
        radio::fault(ProbeError::Transport).await;
    }
    let peers = [radio::selected_address(peer)];
    let config = radio::selected_config(&peers);
    let connect_started = Instant::now();
    let result = {
        let connect = async {
            radio::state(request.revision, RadioPhase::Connecting, ProbeError::None).await;
            central.connect(&config).await
        };
        select(
            radio::changed(request.revision),
            flow::connect(
                &clock,
                session.deadline_ms().unwrap(),
                &mut publication,
                &|update| emit(request.revision, update),
                pin!(connect),
            ),
        )
        .await
    };
    let connection = match result {
        Either::First(()) => {
            let _ = session.event(epoch, clock.now_ms(), SessionEvent::Cancel);
            None
        }
        Either::Second(result) => {
            match result.and_then(|result| result.map_err(crate::errors::failure_reason)) {
                Ok(connection) => Some(connection),
                Err(reason) => {
                    flow::fail(&mut session, epoch, &clock, reason);
                    let _ = flow::before(
                        &clock,
                        session.deadline_ms().unwrap_or(clock.now_ms()),
                        radio::failed(request.revision, InspectionFailure::new(FailureStage::Connect, reason)),
                    )
                    .await;
                    None
                }
            }
        }
    };
    let mut limited = false;
    if let Some(connection) = connection.as_ref() {
        let connected_at_ms = clock.now_ms();
        let connect_elapsed_ms = radio::elapsed_ms(connect_started);
        if connection.peer_address() != peers[0] {
            connection.disconnect();
            flow::fail(&mut session, epoch, &clock, FailureReason::PeerMismatch);
            let _ = flow::before(
                &clock,
                session.deadline_ms().unwrap_or(clock.now_ms()),
                radio::failed(
                    request.revision,
                    InspectionFailure::new(FailureStage::Connect, FailureReason::PeerMismatch),
                ),
            )
            .await;
        } else if session.event(epoch, clock.now_ms(), SessionEvent::Connected).is_ok() {
            let mtu_ready = Cell::new(false);
            let result = {
                let work = pin!(connected_work(
                    stack,
                    connection,
                    request.revision,
                    connected_at_ms,
                    connect_elapsed_ms,
                    &mtu_ready,
                    Attempt {
                        session: &mut session,
                        epoch,
                        publication: &mut publication
                    }
                ));
                // Select borrows the pinned work; cancellation drops it before cleanup.
                // The outer inspector retains the radio stack-admission boundary.
                select(radio::changed(request.revision), work).await
            };
            // Stop the live link before any error/evidence publication can wait for capacity.
            connection.disconnect();
            limited = match result {
                Either::First(()) => {
                    let _ = session.event(epoch, clock.now_ms(), SessionEvent::Cancel);
                    false
                }
                Either::Second(Err(failure)) => {
                    flow::fail(&mut session, epoch, &clock, failure.reason);
                    let _ = flow::before(
                        &clock,
                        session.deadline_ms().unwrap_or(clock.now_ms()),
                        radio::failed(request.revision, failure),
                    )
                    .await;
                    false
                }
                Either::Second(Ok(limited)) => limited,
            };
            // All GATT, channel and event futures are dropped before link disconnection.
            let _ = flow::before(
                &clock,
                session.deadline_ms().unwrap_or(clock.now_ms()),
                radio::link_info(
                    connection,
                    request.revision,
                    connected_at_ms,
                    connect_elapsed_ms,
                    mtu_ready.get(),
                ),
            )
            .await;
        }
    }
    // One cleanup continuation retains the connection owner through terminal publication.
    finish(
        stack,
        request.revision,
        connection.as_ref(),
        active_inspection,
        Attempt {
            session: &mut session,
            epoch,
            publication: &mut publication,
        },
        limited,
    )
    .await;
}

/// Runs fresh inventory and INFO on `stack`/`connection`, publishing under `revision`.
/// `connected_at_ms`/`connect_elapsed_ms` preserve actual timing and `mtu_ready` records
/// completed startup. `attempt` retains session/epoch/publication outside
/// this future. Returns vendor-limited evidence or contextual failure; no cleanup is implied.
async fn connected_work(
    stack: &RadioStack<'_>,
    connection: &Connection<'_, Pool>,
    revision: u32,
    connected_at_ms: u64,
    connect_elapsed_ms: u32,
    mtu_ready: &Cell<bool>,
    attempt: Attempt<'_>,
) -> Result<bool, InspectionFailure> {
    let Attempt {
        session,
        epoch,
        publication,
    } = attempt;
    let clock = Monotonic;
    let discovery_deadline = session
        .deadline_ms()
        .ok_or_else(|| InspectionFailure::new(FailureStage::Inspection, FailureReason::InvalidState))?;
    let mut client_slot = None;
    crate::session::inspect_with_events(
        pin!(radio::att(
            FailureStage::Mtu,
            pin!(Client::new_into(stack, connection, &mut client_slot))
        )),
        async |client| {
            let client = *client;
            mtu_ready.set(true);
            let work = async {
                let endpoints = RefCell::new(FreshEndpoints::default());
                let observed = |event: Event| {
                    if let Event::Item { item, .. } = &event {
                        endpoints.borrow_mut().observe(item);
                    }
                    bridge::emit_event(event)
                };
                let inventory = {
                    // Lend the pinned discovery owner to the timer race. Its scope ends
                    // before interpreting the result or advancing session authority.
                    let discovery = pin!(async {
                        radio::state(revision, RadioPhase::Discovering, ProbeError::None).await;
                        radio::link_info(connection, revision, connected_at_ms, connect_elapsed_ms, true).await;
                        flow::publish_stage(session, &clock, &|update| emit(revision, update)).await;
                        crate::inventory::inventory(client, revision, &observed).await
                    });
                    flow::before(&clock, discovery_deadline, discovery).await
                };
                match inventory {
                    Ok(Ok(false)) => {}
                    Ok(Ok(true)) => {
                        return Err(InspectionFailure::new(
                            FailureStage::Inspection,
                            FailureReason::Capacity,
                        ));
                    }
                    Ok(Err(failure)) => return Err(failure),
                    Err(reason) => return Err(InspectionFailure::new(FailureStage::Inspection, reason)),
                }
                let (service_start, handles) = endpoints
                    .into_inner()
                    .finish()
                    .map_err(|reason| InspectionFailure::new(FailureStage::Characteristics, reason))?;
                session
                    .event(epoch, clock.now_ms(), SessionEvent::Discovered)
                    .map_err(|_| InspectionFailure::new(FailureStage::Inspection, FailureReason::Timeout))?;
                let mut channel = Channel {
                    stack,
                    connection,
                    channel: None,
                };
                flow::run_connected(
                    session,
                    &clock,
                    client,
                    &mut channel,
                    (service_start, handles),
                    publication,
                    &|update| emit(revision, update),
                )
                .await
                .map_err(|reason| InspectionFailure::new(FailureStage::Inspection, reason))
            };
            match select(pin!(work), pin!(client.task())).await {
                Either::First(result) => result,
                Either::Second(result) => {
                    Err(InspectionFailure::new(
                        FailureStage::ClientTask,
                        result
                            .err()
                            .map(crate::errors::failure_reason)
                            .unwrap_or(FailureReason::Disconnected),
                    ))
                }
            }
        },
        pin!(radio::connection_events(stack, connection)),
    )
    .await
}

/// Stops optional `connection` on `stack` and flushes retained `attempt` publication
/// under `revision`, then waits for normal idle within its cleanup
/// deadline. `active_inspection` is cleared only after terminal publication. `limited`
/// keeps protected/unavailable/truncated vendor evidence from showing clean success.
/// Never returns on unconfirmed cleanup; the retained operation lease blocks flash mutation.
async fn finish(
    stack: &RadioStack<'_>,
    revision: u32,
    connection: Option<&Connection<'_, Pool>>,
    active_inspection: &Cell<Option<u32>>,
    attempt: Attempt<'_>,
    limited: bool,
) {
    let Attempt {
        session,
        epoch,
        publication,
    } = attempt;
    let clock = Monotonic;
    if bridge::request().revision != revision {
        let _ = session.event(epoch, clock.now_ms(), SessionEvent::Cancel);
    }
    if let Some(connection) = connection {
        connection.disconnect();
    }
    let deadline = session.deadline_ms().unwrap_or(clock.now_ms());
    let complete = flow::before(&clock, deadline, async {
        radio::state(revision, RadioPhase::Disconnecting, ProbeError::None).await;
        flow::publish_progress(publication, session, &clock, &|update| emit(revision, update)).await;
        while !stack.connections_idle() {
            Timer::after_millis(10).await;
        }
    })
    .await
    .is_ok();
    let context = connection.map(radio::connection_context);
    if bridge::request().revision != revision {
        let _ = session.event(epoch, clock.now_ms(), SessionEvent::Cancel);
    }
    let normal = complete
        && context.is_none_or(|context| {
            context.local_error.is_none() && dashboard_core::ble::termination::normal_reason(context.disconnect_reason)
        });
    if complete {
        let _ = session.event(
            epoch,
            clock.now_ms(),
            SessionEvent::CleanupComplete(if normal {
                CleanupResult::Clean
            } else {
                CleanupResult::Failed
            }),
        );
    } else {
        let _ = session.event(epoch, clock.now_ms(), SessionEvent::Tick);
        radio::latch_fault();
    }
    // Original submission timestamps precede cleanup progress. Keep reliable terminal
    // publication ordered even if the timed cleanup flush was canceled midway.
    flow::publish_progress(publication, session, &clock, &|update| emit(revision, update)).await;
    emit(
        revision,
        Update::End {
            at_ms: clock.now_ms(),
            cleanup: session.cleanup(),
        },
    )
    .await;
    if let Some(context) = context {
        bridge::emit_event(Event::FailureContext { revision, context }).await;
    }
    bridge::emit_event(Event::InspectionEnd {
        revision,
        at_ms: clock.now_ms(),
        cleanup_complete: complete,
        disconnect_reason: context.and_then(|context| context.disconnect_reason),
    })
    .await;
    active_inspection.set(None);
    if !complete {
        radio::fault(ProbeError::Cleanup).await;
    }
    if session.outcome() == Some(Outcome::Success) {
        bridge::emit_event(Event::Complete { revision, limited }).await;
        radio::state(
            revision,
            RadioPhase::Paused,
            if limited {
                ProbeError::Capacity
            } else {
                ProbeError::None
            },
        )
        .await;
    } else {
        radio::state(revision, RadioPhase::Paused, ProbeError::Transport).await;
    }
}
