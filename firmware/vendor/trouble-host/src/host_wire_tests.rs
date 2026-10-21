//! Raw HCI envelopes cross the locked parser and production receive dispatcher; synthetic packets exercise failure/state boundaries.

extern crate std;

use core::cell::Cell;
use core::convert::Infallible;
use core::future::Future;
use core::pin::pin;
use core::task::Waker;

use super::*;
use crate::prelude::DefaultPacketPool;
use crate::HostResources;

/// Only the controller I/O is replaced; packets still cross the locked HCI
/// parser and the production RxRunner dispatch loop.
struct WireController<'a> {
    /// Borrowed complete H4 packets returned sequentially; fixtures must fit the 259-byte scratch.
    packets: &'a [&'a [u8]],
    /// Next unconsumed packet index; end of input makes read permanently pending.
    next: Cell<usize>,
}

impl embedded_io::ErrorType for WireController<'_> {
    /// Transport is infallible; malformed fixture H4 framing panics at parse.
    type Error = Infallible;
}

impl Controller for WireController<'_> {
    /// 259-byte owned H4 receive scratch.
    type Buffer<'a> = [u8; 259];

    /// Returns zeroed receive scratch.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0; 259])
    }

    /// Copies the next fixture into `buf` and returns its borrowed H4 packet; exhaustion remains pending. Oversized or malformed framing panics.
    async fn read<'a>(&self, buf: &'a mut Self::Buffer<'_>) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        let Some(packet) = self.packets.get(self.next.get()) else {
            return core::future::pending().await;
        };
        self.next.set(self.next.get() + 1);
        buf[..packet.len()].copy_from_slice(packet);
        Ok(ControllerToHostPacket::from_hci_bytes_complete(&buf[..packet.len()]).unwrap())
    }

    /// Panics on unexpected ACL output; returns no result.
    async fn write_acl_data(&self, _: &AclPacket<'_>) -> Result<(), Self::Error> {
        panic!("unexpected ACL write")
    }

    /// Panics on unexpected synchronous-data output; returns no result.
    async fn write_sync_data(&self, _: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        panic!("unexpected synchronous write")
    }

    /// Panics on unexpected ISO output; returns no result.
    async fn write_iso_data(&self, _: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        panic!("unexpected ISO write")
    }
}

/// Synchronous commands are outside receive-dispatch fixtures and panic.
impl<C: SyncCmd> ControllerCmdSync<C> for WireController<'_> {
    /// Panics for every synchronous command; returns no result.
    async fn exec(&self, _: &C) -> Result<C::Return, cmd::Error<Self::Error>> {
        panic!("unexpected synchronous command")
    }
}

/// Asynchronous commands succeed immediately without a transport or completion event.
impl<C: AsyncCmd> ControllerCmdAsync<C> for WireController<'_> {
    /// Returns Ok for every asynchronous command without recording or performing I/O.
    async fn exec(&self, _: &C) -> Result<(), cmd::Error<Self::Error>> {
        Ok(())
    }
}

/// Polls the actual receive runner once over `packets` using `handler`; returns terminal host error or Pending after draining fixtures. Does not run the control/transmit loops.
fn poll_rx(packets: &[&[u8]], handler: &impl EventHandler) -> Poll<Result<(), BleHostError<Infallible>>> {
    let controller = WireController {
        packets,
        next: Cell::new(0),
    };
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(controller, &mut resources).build();
    let mut runner = RxRunner {
        host: BleHost::new(&stack.controller, stack.host_state),
    };
    let mut run = pin!(runner.run_with_handler(handler));
    run.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
/// Missing/extra fixed event fields return HciDecode through the actual runner instead of panicking.
fn malformed_event_envelopes_return_decode_errors_without_panicking() {
    for packet in [
        &[4, 0x3e, 0][..],       // Missing LE subevent.
        &[4, 5, 1, 0][..],       // Truncated DisconnectionComplete.
        &[4, 0x13, 0][..],       // Missing completed-packet count.
        &[4, 0x3e, 1, 1][..],    // Truncated LE ConnectionComplete.
        &[4, 0x3e, 1, 0x0a][..], // Truncated EnhancedConnectionComplete.
        &[4, 0x10, 0][..],       // Missing HardwareError code.
        &[4, 0x10, 2, 1, 2][..], // Extra HardwareError bytes.
    ] {
        assert!(
            matches!(
                poll_rx(&[packet], &DummyHandler),
                Poll::Ready(Err(BleHostError::BleHost(Error::HciDecode(_))))
            ),
            "{packet:?}"
        );
    }
}

#[cfg(feature = "scan")]
#[test]
/// Malformed outer legacy/extended report counts and nonempty ScanTimeout parameters return HciDecode.
fn malformed_advertisement_envelopes_return_decode_errors_without_panicking() {
    for subevent in [2, 0x0d] {
        assert!(matches!(
            poll_rx(&[&[4, 0x3e, 1, subevent]], &DummyHandler),
            Poll::Ready(Err(BleHostError::BleHost(Error::HciDecode(_))))
        ));
    }
    // ScanTimeout has no parameters after its subevent byte.
    assert!(matches!(
        poll_rx(&[&[4, 0x3e, 2, 0x11, 0]], &DummyHandler),
        Poll::Ready(Err(BleHostError::BleHost(Error::HciDecode(_))))
    ));
}

#[test]
/// Every selected fixed LE subevent rejects a payload containing only its discriminator.
fn every_handled_fixed_le_event_rejects_missing_fields() {
    for subevent in [1, 3, 6, 7, 10, 12, 18, 53, 55] {
        assert!(
            matches!(
                poll_rx(&[&[4, 0x3e, 1, subevent]], &DummyHandler),
                Poll::Ready(Err(BleHostError::BleHost(Error::HciDecode(_))))
            ),
            "subevent={subevent}"
        );
    }
}

#[test]
/// An unknown LE subevent is skipped while the subsequent vendor event is delivered exactly once.
fn unknown_le_subevent_is_ignored_and_following_vendor_event_is_delivered() {
    /// Counts vendor callbacks; the tuple field is the observed delivery count.
    struct Handler(
        /// Number of vendor events delivered.
        Cell<usize>,
    );
    impl EventHandler for Handler {
        /// Counts the delivered vendor event; its unused payload is irrelevant to routing.
        fn on_vendor(&self, _: &Vendor<'_>) {
            self.0.set(self.0.get() + 1);
        }
    }
    let handler = Handler(Cell::new(0));
    assert!(poll_rx(&[&[4, 0x3e, 1, 0xff], &[4, 0xff, 1, 0xab]], &handler).is_pending());
    assert_eq!(handler.0.get(), 1);
}

#[test]
/// All 256 controller hardware-error codes terminate the receive runner unchanged.
fn hardware_error_terminates_host_with_every_vendor_code_unchanged() {
    for code in 0..=u8::MAX {
        assert!(matches!(poll_rx(&[&[4, 0x10, 1, code]], &DummyHandler),
            Poll::Ready(Err(BleHostError::BleHost(Error::HardwareError(raw)))) if raw == code));
    }
}

#[cfg(feature = "connection-params-update")]
#[test]
/// A full application event queue on a remote parameter request closes the link rather than silently waiting for a reply.
fn undeliverable_remote_parameter_request_disconnects_instead_of_silently_waiting() {
    let packet = [4, 0x3e, 11, 6, 5, 0, 24, 0, 40, 0, 0, 0, 200, 0];
    let packets = [&packet[..]];
    let controller = WireController {
        packets: &packets,
        next: Cell::new(0),
    };
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(controller, &mut resources).build();
    let host = stack.host();
    let mgr = host.connections();
    let peer = Address::random([1, 2, 3, 4, 5, 6]);
    mgr.connect(ConnHandle::new(5), peer, LeConnRole::Central, ConnParams::new())
        .unwrap();
    let conn = embassy_futures::block_on(mgr.accept(LeConnRole::Central, &[peer]));
    for _ in 0..crate::config::CONNECTION_EVENT_QUEUE_SIZE {
        mgr.post_handle_event(
            conn.handle(),
            ConnectionEvent::ConnectionParamsUpdated {
                conn_interval: Duration::from_millis(30),
                peripheral_latency: 0,
                supervision_timeout: Duration::from_secs(2),
            },
        )
        .unwrap();
    }
    let mut runner = RxRunner { host };
    let mut run = pin!(runner.run_with_handler(&DummyHandler));
    assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    assert!(!conn.is_connected());
    mgr.disconnected(conn.handle(), Status::SUCCESS).unwrap();
}

#[cfg(feature = "security")]
#[test]
/// Encryption change/v2/key-refresh events reject missing and extra fixed fields.
fn encryption_events_reject_missing_and_extra_fields() {
    for (opcode, size) in [(0x08, 4), (0x59, 5), (0x30, 3)] {
        for length in (0..size).chain([size + 1]) {
            let mut packet = std::vec![4, opcode, length as u8];
            packet.resize(3 + length, 0);
            assert!(matches!(
                poll_rx(&[&packet], &DummyHandler),
                Poll::Ready(Err(BleHostError::BleHost(Error::HciDecode(_))))
            ));
        }
    }
}

#[cfg(feature = "security")]
#[test]
/// LE encryption success derives security from the bond, ignoring the BR/EDR key-size byte; disabled/failed encryption revokes security and closes.
fn encryption_v2_crosses_rx_dispatch_and_never_uses_br_edr_key_size_for_le() {
    use crate::connection::SecurityLevel;
    use crate::security_manager::{BondInformation, LongTermKey};
    for key_size in [0, 7, 16, 255] {
        for (status, enabled) in [(0, 1), (0, 0), (5, 0)] {
            let packet = [4, 0x59, 5, status, 5, 0, enabled, key_size];
            let packets = [&packet[..]];
            let controller = WireController {
                packets: &packets,
                next: Cell::new(0),
            };
            let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
            let stack = crate::new(controller, &mut resources).build();
            let host = BleHost::new(&stack.controller, stack.host_state);
            let mgr = host.connections();
            let peer = Address::random([1, 2, 3, 4, 5, 0xc6]);
            mgr.connect(ConnHandle::new(5), peer, LeConnRole::Central, ConnParams::new())
                .unwrap();
            let conn = embassy_futures::block_on(mgr.accept(LeConnRole::Central, &[peer]));
            mgr.security_manager
                .add_bond_information(BondInformation::new(
                    peer.into(),
                    LongTermKey::new(1),
                    SecurityLevel::EncryptedAuthenticated,
                    true,
                ))
                .unwrap();
            mgr.with_connected_handle(conn.handle(), |storage| {
                storage.security_level = SecurityLevel::Encrypted;
                Ok(())
            })
            .unwrap();
            let mut runner = RxRunner { host };
            let mut run = pin!(runner.run_with_handler(&DummyHandler));
            assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
            let clean = status == 0 && enabled == 1;
            assert_eq!(conn.is_connected(), clean);
            assert_eq!(
                mgr.connection(0).security_level,
                if clean {
                    SecurityLevel::EncryptedAuthenticated
                } else {
                    SecurityLevel::NoEncryption
                }
            );
            mgr.disconnected(conn.handle(), Status::SUCCESS).unwrap();
        }
    }
}

#[cfg(feature = "scan")]
#[test]
/// An inner malformed advertisement reaches the iterator as InvalidSize, while the following valid batch retains its actual RSSI.
fn inner_short_report_reaches_iterator_callback_and_next_batch_still_arrives() {
    /// Counts valid and malformed inner advertisement reports.
    struct Handler {
        /// Successfully decoded reports with the expected measured RSSI.
        good: Cell<usize>,
        /// Reports rejected specifically with InvalidSize.
        bad: Cell<usize>,
    }
    impl EventHandler for Handler {
        /// Consumes `reports`, counting expected valid/InvalidSize outcomes; unexpected RSSI or error panics.
        fn on_adv_reports(&self, reports: bt_hci::param::LeAdvReportsIter<'_>) {
            for report in reports {
                match report {
                    Ok(report) => {
                        assert_eq!(report.rssi, -59);
                        self.good.set(self.good.get() + 1);
                    }
                    Err(bt_hci::FromHciBytesError::InvalidSize) => self.bad.set(self.bad.get() + 1),
                    Err(error) => panic!("{error:?}"),
                }
            }
        }
    }
    let handler = Handler {
        // Successfully decoded reports with the expected measured RSSI.
        good: Cell::new(0),
        // Reports rejected specifically with InvalidSize.
        bad: Cell::new(0),
    };
    // Exactly16 inner bytes with declared7. No fabricated RSSI or host fault.
    let short = [
        4, 0x3e, 18, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46,
    ];
    let valid = [4, 0x3e, 12, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 0, 0xc5];
    assert!(poll_rx(&[&short, &valid], &handler).is_pending());
    assert_eq!(handler.bad.get(), 1);
    assert_eq!(handler.good.get(), 1);
}

#[test]
/// Failed legacy/enhanced connection completions still require their full fixed-width envelope.
fn connection_failure_still_requires_complete_fixed_fields() {
    for (subevent, size) in [(1, 18), (10, 30)] {
        for length in (0..size).chain([size + 1]) {
            let mut packet = std::vec![4, 0x3e, (length + 1) as u8, subevent];
            packet.extend(core::iter::repeat_n(0xff, length));
            assert!(matches!(
                poll_rx(&[&packet], &DummyHandler),
                Poll::Ready(Err(BleHostError::BleHost(Error::HciDecode(_))))
            ));
        }
    }
}

#[test]
/// Legacy and extended connect futures preserve controller failure status even when unspecified trailing fields are all ones; no slot remains live.
fn failed_connection_complete_preserves_status_for_legacy_and_extended_callers() {
    use crate::central::Central;
    use crate::connection::{ConnectConfig, ScanConfig};
    use bt_hci::param::BdAddr;

    for extended in [false, true] {
        for code in [0x02, 0x0c, 0x3e] {
            for unspecified_fields in [false, true] {
                // Complete fixed fields follow status even though a failure does not
                // create a link. Enhanced completion adds the two six-byte RPAs.
                let mut packet = std::vec![
                    4,
                    0x3e,
                    if extended { 31 } else { 19 },
                    if extended { 10 } else { 1 },
                    code
                ];
                packet.extend_from_slice(&[0, 0, 0, 0, 1, 2, 3, 4, 5, 6]);
                if extended {
                    packet.extend_from_slice(&[0; 12]);
                }
                packet.extend_from_slice(&[24, 0, 0, 0, 200, 0, 0]);
                if unspecified_fields {
                    packet[5..].fill(0xff);
                }
                let packets = [&packet[..]];
                let controller = WireController {
                    packets: &packets,
                    next: Cell::new(0),
                };
                let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
                let stack = crate::new(controller, &mut resources).build();
                let host = BleHost::new(&stack.controller, stack.host_state);
                host.state
                    .initialized
                    .init(InitialState {
                        acl_max: 251,
                        acl_total: 1,
                    })
                    .ok()
                    .unwrap();
                let mut central = Central::new(host);
                let peers = [Address::new(AddrKind::PUBLIC, BdAddr::new([1, 2, 3, 4, 5, 6]))];
                let config = ConnectConfig {
                    scan_config: ScanConfig {
                        filter_accept_list: &peers,
                        ..Default::default()
                    },
                    connect_params: Default::default(),
                };
                let mut connect = pin!(async {
                    if extended {
                        central.connect_ext(&config).await
                    } else {
                        central.connect(&config).await
                    }
                });
                let mut cx = Context::from_waker(Waker::noop());
                assert!(connect.as_mut().poll(&mut cx).is_pending());
                let mut runner = RxRunner { host };
                let mut run = pin!(runner.run_with_handler(&DummyHandler));
                assert!(run.as_mut().poll(&mut cx).is_pending());
                let Poll::Ready(Err(BleHostError::BleHost(Error::Hci(error)))) = connect.as_mut().poll(&mut cx) else {
                    panic!("connection failure status {code:#x} was lost (extended={extended})")
                };
                assert_eq!(error.to_status(), Status::new(code));
                assert!(stack.connections_idle());
            }
        }
    }
}
