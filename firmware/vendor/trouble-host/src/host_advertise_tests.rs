extern crate std;
use super::*;
use crate::advertise::Advertisement;
use crate::prelude::DefaultPacketPool;
use crate::HostResources;
use bt_hci::WriteHci;
use core::cell::Cell;
use core::future::Future;
use core::pin::pin;
use core::task::Waker;
use embassy_futures::block_on;

struct AdvController {
    commands: RefCell<std::vec::Vec<(u16, std::vec::Vec<u8>)>>,
    fail: Cell<Option<u16>>,
    supported_sets: Cell<u8>,
    set_properties: RefCell<std::collections::BTreeMap<u8, u16>>,
    advertising_data: RefCell<std::collections::BTreeMap<u8, std::vec::Vec<u8>>>,
    enabled_sets: RefCell<std::collections::BTreeSet<u8>>,
}
impl Default for AdvController {
    fn default() -> Self {
        Self {
            commands: RefCell::new(std::vec::Vec::new()),
            fail: Cell::new(None),
            supported_sets: Cell::new(2),
            set_properties: RefCell::new(std::collections::BTreeMap::new()),
            advertising_data: RefCell::new(std::collections::BTreeMap::new()),
            enabled_sets: RefCell::new(std::collections::BTreeSet::new()),
        }
    }
}
impl embedded_io::ErrorType for AdvController {
    type Error = core::convert::Infallible;
}
impl Controller for AdvController {
    type Buffer<'a> = [u8; 259];
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0; 259])
    }
    async fn read<'a>(&self, _: &'a mut Self::Buffer<'_>) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        core::future::pending().await
    }
    async fn write_acl_data(&self, _: &AclPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
    async fn write_sync_data(&self, _: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
    async fn write_iso_data(&self, _: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
}
impl<C: SyncCmd> ControllerCmdSync<C> for AdvController {
    async fn exec(&self, cmd: &C) -> Result<C::Return, cmd::Error<Self::Error>> {
        let mut bytes = std::vec![0; cmd.params().size()];
        cmd.params().write_hci(&mut bytes[..]).unwrap();
        assert!(bytes.len() <= 255, "command parameters exceed the HCI length octet");
        let opcode = C::OPCODE.to_raw();
        self.commands.borrow_mut().push((opcode, bytes.clone()));
        if self.fail.get() == Some(opcode) {
            return Err(cmd::Error::Hci(bt_hci::param::Error::CMD_DISALLOWED));
        }
        // HCI 7.8.53/54: extended scannable and legacy directed PDUs have
        // no advertising-data field, even for an empty data command.
        let supports_data = |props: u16| (props & 0x10 == 0 && props & 2 == 0) || (props & 0x10 != 0 && props & 4 == 0);
        if opcode == 0x2036 {
            let props = u16::from_le_bytes([bytes[1], bytes[2]]);
            if !supports_data(props)
                && self
                    .advertising_data
                    .borrow()
                    .get(&bytes[0])
                    .is_some_and(|v| !v.is_empty())
            {
                return Err(cmd::Error::Hci(bt_hci::param::Error::INVALID_HCI_PARAMETERS));
            }
            self.set_properties.borrow_mut().insert(bytes[0], props);
        } else if opcode == 0x2037 {
            if self
                .set_properties
                .borrow()
                .get(&bytes[0])
                .is_some_and(|p| !supports_data(*p))
            {
                return Err(cmd::Error::Hci(bt_hci::param::Error::INVALID_HCI_PARAMETERS));
            }
            self.advertising_data.borrow_mut().insert(bytes[0], bytes[4..].to_vec());
        } else if opcode == 0x203d {
            if !self.enabled_sets.borrow().is_empty() {
                return Err(cmd::Error::Hci(bt_hci::param::Error::CMD_DISALLOWED));
            }
            self.set_properties.borrow_mut().clear();
            self.advertising_data.borrow_mut().clear();
        } else if opcode == 0x2039 {
            let mut enabled = self.enabled_sets.borrow_mut();
            if bytes[0] == 0 && bytes[1] == 0 {
                enabled.clear();
            } else {
                for set in bytes[2..].chunks_exact(4) {
                    if bytes[0] == 0 {
                        enabled.remove(&set[0]);
                    } else {
                        enabled.insert(set[0]);
                    }
                }
            }
        }
        // Advertising responses plus the real ControlRunner's startup queries.
        let result = match opcode {
            0x203b => [self.supported_sets.get(), 0, 0, 0, 0, 0, 0, 0],
            0x2002 => [251, 0, 1, 0, 0, 0, 0, 0], // LE ACL packet length/count.
            0x200f => [2, 0, 0, 0, 0, 0, 0, 0],   // Filter accept list size.
            0x1009 => [1, 2, 3, 4, 5, 6, 0, 0],   // Public address.
            0x2018 => [0x5a; 8],                  // Deterministic LE Rand for security builds.
            _ => [0; 8],                          // Unit or selected advertising TX power.
        };
        Ok(C::Return::from_hci_bytes(&result).unwrap().0)
    }
}
impl<C: AsyncCmd> ControllerCmdAsync<C> for AdvController {
    async fn exec(&self, _: &C) -> Result<(), cmd::Error<Self::Error>> {
        unreachable!()
    }
}
fn initialized(stack: &crate::Stack<'_, AdvController, DefaultPacketPool>) {
    stack
        .host()
        .state
        .initialized
        .init(InitialState {
            acl_max: 251,
            acl_total: 1,
        })
        .ok()
        .unwrap();
}

#[test]
fn legacy_empty_updates_clear_previous_payloads_and_oversized_updates_are_atomic() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(AdvController::default(), &mut resources).build();
    initialized(&stack);
    let mut peripheral = stack.peripheral();
    for data in [
        Advertisement::NonconnectableScannableUndirected {
            adv_data: &[1; 31],
            scan_data: &[2; 31],
        },
        Advertisement::NonconnectableScannableUndirected {
            adv_data: &[],
            scan_data: &[],
        },
    ] {
        block_on(peripheral.update_adv_data(data)).unwrap();
    }
    let commands = stack.controller.commands.borrow();
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[0], (0x2008, [std::vec![31], std::vec![1; 31]].concat()));
    assert_eq!(commands[1], (0x2009, [std::vec![31], std::vec![2; 31]].concat()));
    assert_eq!(commands[2], (0x2008, std::vec![0; 32]));
    assert_eq!(commands[3], (0x2009, std::vec![0; 32]));
    drop(commands);
    for (adv_data, scan_data) in [(&[0; 32][..], &[][..]), (&[][..], &[0; 32][..])] {
        let before = stack.controller.commands.borrow().len();
        assert!(block_on(
            peripheral.update_adv_data(Advertisement::NonconnectableScannableUndirected { adv_data, scan_data })
        )
        .is_err());
        assert_eq!(stack.controller.commands.borrow().len(), before);
    }
}

#[test]
fn invalid_start_and_dropped_waiter_do_not_cancel_an_existing_advertiser() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(AdvController::default(), &mut resources).build();
    initialized(&stack);
    let mut peripheral = stack.peripheral();
    let params = Default::default();
    let data = Advertisement::NonconnectableNonscannableUndirected { adv_data: &[] };
    let active = block_on(peripheral.advertise(&params, data)).unwrap();
    let mut other = stack.peripheral();
    {
        let mut waiter = pin!(other.advertise(&params, data));
        assert!(waiter
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
    }
    assert!(stack.host().advertise_command_state().is_active());
    drop(active);
    assert!(!stack.host().advertise_command_state().is_active());
}

#[test]
fn advertising_command_failure_never_enables_and_releases_through_cancellation() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(AdvController::default(), &mut resources).build();
    initialized(&stack);
    stack.controller.fail.set(Some(0x2009));
    let mut peripheral = stack.peripheral();
    assert!(block_on(peripheral.advertise(
        &Default::default(),
        Advertisement::NonconnectableScannableUndirected {
            adv_data: &[],
            scan_data: &[1]
        }
    ))
    .is_err());
    assert!(stack
        .controller
        .commands
        .borrow()
        .iter()
        .all(|(opcode, _)| *opcode != 0x200a));
    assert!(stack
        .host()
        .advertise_command_state()
        .poll_cancelled(&mut Context::from_waker(Waker::noop()))
        .is_ready());
}

#[cfg(feature = "extended-advertising")]
mod extended {
    use super::*;
    use crate::advertise::{AdvHandle, AdvSet, AdvertisementSet};

    const DATA: u16 = 0x2037;
    const SCAN: u16 = 0x2038;
    const ENABLE: u16 = 0x2039;

    fn set(data: Advertisement<'_>) -> AdvertisementSet<'_> {
        AdvertisementSet {
            params: Default::default(),
            data,
            address: None,
        }
    }

    fn nonscannable(bytes: &[u8]) -> Advertisement<'_> {
        Advertisement::ExtNonconnectableNonscannableUndirected {
            anonymous: false,
            adv_data: bytes,
        }
    }

    fn handles(count: usize) -> std::vec::Vec<AdvSet> {
        (0..count)
            .map(|i| AdvSet {
                adv_handle: AdvHandle::new(i as u8),
                duration: bt_hci::param::Duration::from_u16(0),
                max_ext_adv_events: 0,
            })
            .collect()
    }

    #[test]
    fn scannable_extended_and_directed_legacy_sets_skip_unsupported_adv_data_commands() {
        let peer = crate::Address::random([1, 2, 3, 4, 5, 0xc6]);
        for data in [
            Advertisement::ExtNonconnectableScannableUndirected { scan_data: &[1] },
            Advertisement::ExtNonconnectableScannableDirected { peer, scan_data: &[2] },
            Advertisement::ConnectableNonscannableDirected { peer },
        ] {
            let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
            let stack = crate::new(AdvController::default(), &mut resources).build();
            initialized(&stack);
            let mut peripheral = stack.peripheral();
            let sets = [set(data)];
            let active = block_on(peripheral.advertise_ext(&sets, &mut handles(1))).unwrap();
            let commands = stack.controller.commands.borrow();
            assert!(commands.iter().all(|(opcode, _)| *opcode != DATA));
            assert_eq!(commands.last().unwrap().0, ENABLE);
            drop(commands);
            drop(active);
        }
    }

    #[test]
    fn complete_251_byte_commands_and_empty_clear_preserve_exact_payloads() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1, 2>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        initialized(&stack);
        let mut peripheral = stack.peripheral();
        let payload: std::vec::Vec<u8> = (0..251).collect();
        let mut handles = handles(2);
        handles[0].adv_handle = AdvHandle::new(0xef);
        handles[1].adv_handle = AdvHandle::new(7);
        let mut sets = [
            set(nonscannable(&payload)),
            set(Advertisement::ExtNonconnectableScannableUndirected { scan_data: &payload }),
        ];
        sets[1].params.fragment = true;
        block_on(peripheral.update_adv_data_ext(&sets, &mut handles)).unwrap();
        let expected = [
            (DATA, [std::vec![0xef, 3, 0, 251], payload.clone()].concat()),
            (SCAN, std::vec![0xef, 3, 0, 0]),
            (SCAN, [std::vec![7, 3, 1, 251], payload].concat()),
        ];
        assert_eq!(*stack.controller.commands.borrow(), expected);
        stack.controller.commands.borrow_mut().clear();
        let clear = [
            set(nonscannable(&[])),
            set(Advertisement::ExtNonconnectableScannableUndirected { scan_data: &[] }),
        ];
        block_on(peripheral.update_adv_data_ext(&clear, &mut handles)).unwrap();
        assert_eq!(
            *stack.controller.commands.borrow(),
            [
                (DATA, std::vec![0xef, 3, 0, 0]),
                (SCAN, std::vec![0xef, 3, 0, 0]),
                (SCAN, std::vec![7, 3, 0, 0]),
            ]
        );
    }

    #[test]
    fn invalid_later_payloads_and_legacy_limits_reject_the_entire_batch_before_io() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1, 2>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        initialized(&stack);
        let mut peripheral = stack.peripheral();
        for invalid in [
            nonscannable(&[0; 252]),
            Advertisement::ExtNonconnectableScannableUndirected { scan_data: &[0; 252] },
            Advertisement::NonconnectableScannableUndirected {
                adv_data: &[0; 32],
                scan_data: &[],
            },
            Advertisement::NonconnectableScannableUndirected {
                adv_data: &[],
                scan_data: &[0; 32],
            },
            Advertisement::ExtConnectableNonscannableUndirected { adv_data: &[2, 1, 0] },
        ] {
            let sets = [set(nonscannable(&[9])), set(invalid)];
            assert!(block_on(peripheral.update_adv_data_ext(&sets, &mut handles(2))).is_err());
            assert!(block_on(peripheral.advertise_ext(&sets, &mut handles(2))).is_err());
            assert!(stack.controller.commands.borrow().is_empty());
            assert!(stack.host().advertise_command_state().is_idle());
        }
    }

    #[test]
    fn zero_mismatched_and_64_set_requests_do_not_reach_the_controller() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        initialized(&stack);
        let mut peripheral = stack.peripheral();
        // An empty update is a no-op; enabling zero sets is invalid.
        block_on(peripheral.update_adv_data_ext(&[], &mut [])).unwrap();
        assert!(block_on(peripheral.advertise_ext(&[], &mut [])).is_err());
        let sets = std::vec![set(nonscannable(&[])); 64];
        for (set_count, handle_count) in [(0, 1), (1, 0), (1, 2), (2, 1), (64, 64)] {
            assert!(block_on(peripheral.update_adv_data_ext(&sets[..set_count], &mut handles(handle_count))).is_err());
            assert!(block_on(peripheral.advertise_ext(&sets[..set_count], &mut handles(handle_count))).is_err());
            assert!(stack.controller.commands.borrow().is_empty());
        }
    }

    #[test]
    fn duplicate_and_reserved_update_handles_reject_before_the_first_set_write() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1, 2>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        initialized(&stack);
        let mut peripheral = stack.peripheral();
        let sets = [set(nonscannable(&[1])), set(nonscannable(&[2]))];
        for raw in [0, 0xf0, 0xff] {
            let mut handles = handles(2);
            handles[1].adv_handle = AdvHandle::new(raw);
            assert!(block_on(peripheral.update_adv_data_ext(&sets, &mut handles)).is_err());
            assert!(stack.controller.commands.borrow().is_empty());
        }
    }

    #[test]
    fn all_63_sets_fit_the_enable_packet_without_count_or_handle_truncation() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1, 63>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        initialized(&stack);
        stack.controller.supported_sets.set(63);
        let mut peripheral = stack.peripheral();
        let sets = std::vec![set(nonscannable(&[])); 63];
        let mut handles = handles(63);
        let advertiser = block_on(peripheral.advertise_ext(&sets, &mut handles)).unwrap();
        let commands = stack.controller.commands.borrow();
        let (opcode, bytes) = commands.last().unwrap();
        assert_eq!(*opcode, ENABLE);
        assert_eq!(&bytes[..2], &[1, 63]);
        assert_eq!(bytes.len(), 254);
        for (i, record) in bytes[2..].chunks_exact(4).enumerate() {
            assert_eq!(record, &[i as u8, 0, 0, 0]);
        }
        assert_eq!(commands.iter().filter(|(op, _)| *op == DATA).count(), 63);
        assert_eq!(commands.iter().filter(|(op, _)| *op == SCAN).count(), 63);
        drop(commands);
        drop(advertiser);
    }

    #[test]
    fn capacity_failure_never_modifies_sets_or_acquires_cancellation() {
        for controller_capacity in [0, 1, 2] {
            // Two sets cannot fit this host even when the controller supports them.
            let mut resources = HostResources::<DefaultPacketPool, 1, 1, 1>::new();
            let stack = crate::new(AdvController::default(), &mut resources).build();
            initialized(&stack);
            stack.controller.supported_sets.set(controller_capacity);
            let mut peripheral = stack.peripheral();
            let sets = [set(nonscannable(&[])), set(nonscannable(&[]))];
            assert!(block_on(peripheral.advertise_ext(&sets, &mut handles(2))).is_err());
            assert_eq!(*stack.controller.commands.borrow(), [(0x203b, std::vec![])]);
            assert!(stack.host().advertise_command_state().is_idle());
        }
    }

    #[test]
    fn setup_errors_stop_before_enable_and_retain_cancellation_ownership() {
        for failed in [0x203b, 0x203d, 0x2036, 0x2035, DATA, SCAN, ENABLE] {
            let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
            let stack = crate::new(AdvController::default(), &mut resources).build();
            initialized(&stack);
            stack.controller.fail.set(Some(failed));
            let mut peripheral = stack.peripheral();
            let mut sets = [set(nonscannable(&[1]))];
            sets[0].address = Some(bt_hci::param::BdAddr::new([1, 2, 3, 4, 5, 0xc6]));
            assert!(block_on(peripheral.advertise_ext(&sets, &mut handles(1))).is_err());
            let commands = stack.controller.commands.borrow();
            assert_eq!(commands.last().unwrap().0, failed);
            assert_eq!(
                commands.iter().filter(|(op, _)| *op == ENABLE).count(),
                usize::from(failed == ENABLE)
            );
            drop(commands);
            let host = stack.host();
            let state = host.advertise_command_state();
            if failed == 0x203b {
                assert!(state.is_idle());
            } else {
                assert!(!state.is_idle());
                assert_eq!(
                    state.poll_cancelled(&mut Context::from_waker(Waker::noop())),
                    Poll::Ready(true)
                );
            }
        }
    }

    #[test]
    fn dropped_waiter_cannot_cancel_active_set_and_real_cleanup_allows_reuse() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        let mut control = ControlRunner { host: stack.host() };
        let mut control_task = pin!(control.run());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        stack.controller.commands.borrow_mut().clear();
        let mut peripheral = stack.peripheral();
        let mut other = stack.peripheral();
        let sets = [set(nonscannable(&[1]))];
        let active = block_on(peripheral.advertise_ext(&sets, &mut handles(1))).unwrap();
        let before = stack.controller.commands.borrow().len();
        {
            let mut storage = handles(1);
            let mut waiter = pin!(other.advertise_ext(&sets, &mut storage));
            assert!(waiter.as_mut().poll(&mut cx).is_pending());
        }
        assert!(stack.host().advertise_command_state().is_active());
        assert!(stack
            .host()
            .advertise_command_state()
            .poll_cancelled(&mut cx)
            .is_pending());
        assert_eq!(&stack.controller.commands.borrow()[before..], &[(0x203b, std::vec![])]);
        drop(active);
        assert!(!stack.host().advertise_command_state().is_idle());
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            stack.controller.commands.borrow().last().unwrap(),
            &(ENABLE, std::vec![0, 0])
        );
        assert!(stack.host().advertise_command_state().is_idle());
        let resumed = block_on(other.advertise_ext(&sets, &mut handles(1))).unwrap();
        assert!(stack.host().advertise_command_state().is_active());
        drop(resumed);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        assert!(stack.host().advertise_command_state().is_idle());
        // Failed setup retains admission until the same real disable path finishes.
        stack.controller.fail.set(Some(SCAN));
        assert!(block_on(peripheral.advertise_ext(&sets, &mut handles(1))).is_err());
        assert!(!stack.host().advertise_command_state().is_idle());
        stack.controller.fail.set(None);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            stack.controller.commands.borrow().last().unwrap(),
            &(ENABLE, std::vec![0, 0])
        );
        assert!(stack.host().advertise_command_state().is_idle());
        let after_failure = block_on(peripheral.advertise_ext(&sets, &mut handles(1))).unwrap();
        drop(after_failure);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        assert!(stack.host().advertise_command_state().is_idle());
    }

    #[test]
    fn type_restart_waits_for_disable_then_clears_old_data_and_clear_errors_are_recoverable() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        let mut control = ControlRunner { host: stack.host() };
        let mut control_task = pin!(control.run());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        let mut peripheral = stack.peripheral();
        let initial = [set(nonscannable(&[9, 8, 7]))];
        let active = block_on(peripheral.advertise_ext(&initial, &mut handles(1))).unwrap();
        assert_eq!(stack.controller.advertising_data.borrow().get(&0).unwrap(), &[9, 8, 7]);
        drop(active);
        stack.controller.commands.borrow_mut().clear();
        let next = [set(Advertisement::ExtNonconnectableScannableUndirected {
            scan_data: &[1],
        })];
        let next_advertiser = {
            let mut storage = handles(1);
            let mut waiting = pin!(peripheral.advertise_ext(&next, &mut storage));
            assert!(waiting.as_mut().poll(&mut cx).is_pending());
            assert_eq!(*stack.controller.commands.borrow(), [(0x203b, std::vec![])]);
            assert!(control_task.as_mut().poll(&mut cx).is_pending());
            match waiting.as_mut().poll(&mut cx) {
                Poll::Ready(result) => result.expect("restart rejected after the prior disable"),
                _ => panic!("restart did not finish after the prior disable"),
            }
        };
        let commands = stack.controller.commands.borrow();
        assert_eq!(
            commands.iter().map(|(op, _)| *op).collect::<std::vec::Vec<_>>(),
            [0x203b, ENABLE, 0x203d, 0x2036, SCAN, ENABLE]
        );
        assert_eq!(commands[1].1, [0, 0]);
        assert!(stack.controller.advertising_data.borrow().is_empty());
        drop(commands);
        drop(next_advertiser);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());

        stack.controller.commands.borrow_mut().clear();
        stack.controller.fail.set(Some(0x203d));
        assert!(block_on(peripheral.advertise_ext(&initial, &mut handles(1))).is_err());
        assert_eq!(
            *stack.controller.commands.borrow(),
            [(0x203b, std::vec![]), (0x203d, std::vec![])]
        );
        assert!(!stack.host().advertise_command_state().is_idle());
        stack.controller.fail.set(None);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        assert!(stack.host().advertise_command_state().is_idle());
        let resumed = block_on(peripheral.advertise_ext(&initial, &mut handles(1))).unwrap();
        assert_eq!(stack.controller.advertising_data.borrow().get(&0).unwrap(), &[9, 8, 7]);
        drop(resumed);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
    }

    #[test]
    fn accepting_one_connection_disables_remaining_sets_before_restart() {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1, 2>::new();
        let stack = crate::new(AdvController::default(), &mut resources).build();
        let mut control = ControlRunner { host: stack.host() };
        let mut control_task = pin!(control.run());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        let mut peripheral = stack.peripheral();
        let sets = [
            set(Advertisement::ExtConnectableNonscannableUndirected { adv_data: &[2, 1, 6] }),
            set(nonscannable(&[9])),
        ];
        let active = block_on(peripheral.advertise_ext(&sets, &mut handles(2))).unwrap();
        // The controller terminates only the set that accepted this connection.
        stack.controller.enabled_sets.borrow_mut().remove(&0);
        stack.host().advertise_state().terminate(AdvHandle::new(0));
        stack
            .host()
            .connections()
            .connect(
                bt_hci::param::ConnHandle::new(0),
                crate::Address::random([1, 2, 3, 4, 5, 0xc6]),
                bt_hci::param::LeConnRole::Peripheral,
                crate::connection::ConnParams::new(),
            )
            .unwrap();
        let connection = block_on(active.accept()).unwrap();
        assert_eq!(*stack.controller.enabled_sets.borrow(), [1].into_iter().collect());
        stack.controller.commands.borrow_mut().clear();
        let mut storage = handles(2);
        let mut restart = pin!(peripheral.advertise_ext(&sets, &mut storage));
        assert!(restart.as_mut().poll(&mut cx).is_pending());
        assert_eq!(*stack.controller.commands.borrow(), [(0x203b, std::vec![])]);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            stack.controller.commands.borrow().last().unwrap(),
            &(ENABLE, std::vec![0, 0])
        );
        assert!(stack.host().connections().is_handle_connected(connection.handle()));
        let next = match restart.as_mut().poll(&mut cx) {
            Poll::Ready(result) => result.unwrap(),
            _ => panic!("restart did not finish after all sets were disabled"),
        };
        drop(next);
        assert!(control_task.as_mut().poll(&mut cx).is_pending());
        assert!(stack.host().connections().is_handle_connected(connection.handle()));
        assert!(stack
            .controller
            .commands
            .borrow()
            .iter()
            .all(|(opcode, _)| *opcode != 0x0406));
        drop(connection);
    }
}
