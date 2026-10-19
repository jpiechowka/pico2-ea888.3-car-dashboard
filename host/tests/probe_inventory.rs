#[path = "../../firmware/ble-probe/src/errors.rs"]
mod errors;
#[path = "../../firmware/ble-probe/src/inventory.rs"]
mod inventory;

use core::future::Future;
use core::ops::ControlFlow;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::cell::{Cell, RefCell};

use dashboard_core::discovery::{Event, FailureReason, InspectionItem, InventoryStage, Service, Uuid as ProbeUuid};
use inventory::InventoryClient;
use trouble_host::prelude::Uuid;

#[derive(Debug, PartialEq)]
enum Request {
    Type(u16, u16, u16),
    Find(u16, u16),
    Read(u16),
}

#[test]
fn mixed_uuid_service_discovery_order_keeps_every_descriptor_range() {
    let mut fixture = Fixture::default();
    // Mixed UUID widths can return later service groups before pending gaps.
    for start in [1, 21, 11] {
        fixture.service(start, start + 9, 0xFFF0);
        fixture.characteristic(start + 1, 0xFFF1, false);
    }
    fixture.descriptors = vec![4, 14, 24];
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(false));
    for (from, to) in [(4, 10), (14, 20), (24, 30)] {
        assert!(
            fixture.calls.borrow().contains(&Request::Find(from, to)),
            "missing descriptor query {from}..{to}; calls={:?}",
            fixture.calls.borrow()
        );
    }
    assert!(complete(&events, InventoryStage::Descriptors));
}

#[test]
fn all_service_slots_keep_empty_and_multi_characteristic_ranges_separate() {
    let mut fixture = Fixture::default();
    for index in (0..24).rev() {
        let start = 1 + index * 10;
        fixture.service(start, start + 9, 0xFFF0);
        if index % 3 == 0 {
            fixture.characteristic(start + 1, 0xFFF1, false);
            fixture.characteristic(start + 5, 0xFFF2, false);
        }
    }
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(false));
    let finds: Vec<_> = fixture
        .calls
        .borrow()
        .iter()
        .filter_map(|call| {
            match call {
                Request::Find(from, to) => Some((*from, *to)),
                _ => None,
            }
        })
        .collect();
    let expected: Vec<_> = (0..24)
        .rev()
        .filter(|index| index % 3 == 0)
        .flat_map(|index| {
            let start = 1 + index * 10;
            [(start + 3, start + 4), (start + 7, start + 9)]
        })
        .collect();
    assert_eq!(finds, expected);
    assert!(complete(&events, InventoryStage::Descriptors));
}

#[test]
fn capacity_at_service_boundary_never_queries_unvisited_service_rows() {
    let mut fixture = Fixture::default();
    fixture.service(100, 300, 0xFFF0);
    for index in 0..64 {
        fixture.characteristic(101 + index * 3, 0xFFF1, false);
    }
    fixture.service(1, 20, 0xFFF0);
    fixture.characteristic(2, 0xFFF1, false);
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(true));
    assert!(
        !fixture
            .calls
            .borrow()
            .iter()
            .any(|call| matches!(call, Request::Find(from, _) if *from < 100))
    );
    assert!(fixture.calls.borrow().contains(&Request::Find(292, 300)));
    assert!(!complete(&events, InventoryStage::Descriptors));
}
#[derive(Default)]
struct Fixture {
    services: Vec<Service>,
    declarations: Vec<(u16, Vec<u8>)>,
    descriptors: Vec<u16>,
    values: Vec<(u16, Result<Vec<u8>, FailureReason>)>,
    hash: Option<(u16, Vec<u8>)>,
    calls: RefCell<Vec<Request>>,
    descriptor_error_after: Option<u16>,
    services_pending: Cell<bool>,
    services_dropped: Cell<usize>,
    services_error: Option<FailureReason>,
}
impl Fixture {
    fn service(
        &mut self,
        start: u16,
        end: u16,
        uuid: u16,
    ) {
        self.services.push(Service {
            start,
            end,
            uuid: ProbeUuid::from_wire(&uuid.to_le_bytes()).unwrap(),
        });
    }

    fn characteristic(
        &mut self,
        declaration: u16,
        uuid: u16,
        readable: bool,
    ) {
        let handle = declaration + 1;
        let [lo, hi] = handle.to_le_bytes();
        let [ulo, uhi] = uuid.to_le_bytes();
        self.declarations
            .push((declaration, vec![if readable { 2 } else { 0 }, lo, hi, ulo, uhi]));
    }
}
impl InventoryClient for Fixture {
    type Service = Service;

    async fn services(
        &self,
        output: &mut heapless::Vec<Service, 24>,
    ) -> Result<(), FailureReason> {
        struct Finished<'a>(&'a Cell<usize>);
        impl Drop for Finished<'_> {
            fn drop(&mut self) { self.0.set(self.0.get() + 1); }
        }
        let _finished = Finished(&self.services_dropped);
        if self.services_pending.get() {
            core::future::pending::<()>().await;
        }
        if let Some(reason) = self.services_error {
            return Err(reason);
        }
        output.clear();
        output.extend(self.services.iter().copied());
        Ok(())
    }

    async fn read_by_type(
        &self,
        start: u16,
        end: u16,
        uuid: &Uuid,
        mut callback: impl FnMut(u16, &[u8]) -> ControlFlow<()>,
    ) -> Result<Option<()>, FailureReason> {
        let uuid = u16::from_le_bytes(uuid.as_raw().try_into().unwrap());
        self.calls.borrow_mut().push(Request::Type(start, end, uuid));
        let rows = if uuid == 0x2803 {
            &self.declarations
        } else {
            assert_eq!(uuid, 0x2B2A, "no unapproved UUID request");
            if let Some((handle, bytes)) = &self.hash {
                return Ok(callback(*handle, bytes).break_value());
            }
            return Err(FailureReason::Att(0x0A));
        };
        for (handle, bytes) in rows {
            if (start..=end).contains(handle) && callback(*handle, bytes).is_break() {
                return Ok(Some(()));
            }
        }
        Ok(None)
    }

    async fn find_information(
        &self,
        start: u16,
        end: u16,
        mut callback: impl FnMut(u16, Uuid) -> ControlFlow<()>,
    ) -> Result<Option<()>, FailureReason> {
        self.calls.borrow_mut().push(Request::Find(start, end));
        for &handle in &self.descriptors {
            if (start..=end).contains(&handle) {
                if callback(handle, Uuid::new_short(0x2902)).is_break() {
                    return Ok(Some(()));
                }
                if self.descriptor_error_after == Some(handle) {
                    return Err(FailureReason::UnexpectedResponse);
                }
            }
        }
        Ok(None)
    }

    async fn read_handle(
        &self,
        handle: u16,
        bytes: &mut [u8],
    ) -> Result<usize, FailureReason> {
        self.calls.borrow_mut().push(Request::Read(handle));
        let value = self
            .values
            .iter()
            .find(|(h, _)| *h == handle)
            .map(|(_, v)| v.clone())
            .unwrap_or(Ok(vec![b'x']))?;
        let count = value.len().min(bytes.len());
        bytes[..count].copy_from_slice(&value[..count]);
        Ok(count)
    }
}

#[test]
fn cancelled_service_discovery_drops_borrowed_operation_without_publishing_or_querying() {
    let mut fixture = Fixture::default();
    fixture.service(1, 5, 0xFFF0);
    fixture.characteristic(2, 0xFFF1, false);
    fixture.services_pending.set(true);
    let events = RefCell::new(Vec::new());
    let emit = async |event| events.borrow_mut().push(event);
    {
        let mut future = pin!(inventory::inventory(&fixture, 7, &emit));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(fixture.services_dropped.get(), 0);
        assert!(events.borrow().is_empty());
        assert!(fixture.calls.borrow().is_empty());
    }
    assert_eq!(fixture.services_dropped.get(), 1);
    fixture.services_pending.set(false);
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(false));
    assert!(complete(&events, InventoryStage::Services));
    assert_eq!(fixture.services_dropped.get(), 2);
}

#[test]
fn service_error_has_original_stage_and_stops_before_later_operations() {
    let fixture = Fixture {
        services_error: Some(FailureReason::Hardware(0xE1)),
        ..Fixture::default()
    };
    let (result, events) = run(&fixture);
    let failure = result.unwrap_err();
    assert_eq!(failure.stage, dashboard_core::discovery::FailureStage::Services);
    assert_eq!(failure.reason, FailureReason::Hardware(0xE1));
    assert!(failure.elapsed_ms.is_some());
    assert!(events.is_empty());
    assert!(fixture.calls.borrow().is_empty());
    assert_eq!(fixture.services_dropped.get(), 1);
}
fn run(fixture: &Fixture) -> (Result<bool, dashboard_core::discovery::InspectionFailure>, Vec<Event>) {
    let events = RefCell::new(Vec::new());
    let emit = async |event| events.borrow_mut().push(event);
    let result = {
        let mut future = pin!(inventory::inventory(fixture, 7, &emit));
        match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("synchronous fixture unexpectedly pending"),
        }
    };
    (result, events.into_inner())
}
fn complete(
    events: &[Event],
    expected: InventoryStage,
) -> bool {
    events.iter().any(|event| matches!(event, Event::Item { revision: 7, item: InspectionItem::StageComplete(stage) } if *stage == expected))
}
#[test]
fn read_capacity_does_not_hide_final_descriptor_ranges_or_completed_discovery() {
    let mut fixture = Fixture::default();
    fixture.service(1, 60, 0x180A);
    for i in 0..17 {
        fixture.characteristic(2 + i * 3, 0x2A29, true);
    }
    fixture.service(70, 80, 0xFFF0);
    fixture.characteristic(71, 0xFFF1, false);
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(true));
    assert!(
        fixture.calls.borrow().contains(&Request::Find(73, 80)),
        "unrelated read-list capacity must not hide the later service's final descriptors"
    );
    assert!(complete(&events, InventoryStage::Characteristics));
    assert!(complete(&events, InventoryStage::Descriptors));
    assert!(!complete(&events, InventoryStage::Reads));
    assert_eq!(
        fixture
            .calls
            .borrow()
            .iter()
            .filter(|r| matches!(r, Request::Read(_)))
            .count(),
        16
    );
}
#[test]
fn descriptor_capacity_does_not_hide_completed_characteristics_or_empty_reads() {
    let mut fixture = Fixture::default();
    fixture.service(1, 200, 0xFFF0);
    fixture.characteristic(2, 0xFFF1, false);
    fixture.descriptors = (4..132).collect();
    fixture.service(210, 220, 0xFFF2);
    fixture.characteristic(211, 0xFFF3, false);
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(true));
    assert!(complete(&events, InventoryStage::Characteristics));
    assert!(!complete(&events, InventoryStage::Descriptors));
    assert!(complete(&events, InventoryStage::Reads));
    assert_eq!(
        events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Item {
                        item: InspectionItem::Descriptor(_),
                        ..
                    }
                )
            })
            .count(),
        128
    );
}
#[test]
fn standard_values_preserve_exact_att_outcomes_and_hash_uses_uuid_procedure() {
    let mut fixture = Fixture::default();
    fixture.service(1, 7, 0x1800);
    fixture.characteristic(2, 0x2A01, true);
    fixture.characteristic(5, 0x2A00, true);
    fixture.service(10, 14, 0x1801);
    fixture.characteristic(11, 0x2B2A, true);
    fixture.values = vec![
        (3, Ok(vec![0xAB])),
        (
            6,
            Err(errors::failure_reason::<()>(trouble_host::BleHostError::BleHost(
                trouble_host::Error::Att(trouble_host::att::AttErrorCode::INSUFFICIENT_AUTHENTICATION),
            ))),
        ),
    ];
    fixture.hash = Some((12, (0..16).collect()));
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(false));
    let values: Vec<_> = events
        .iter()
        .filter_map(|e| {
            match e {
                Event::Item {
                    item: InspectionItem::Value(v),
                    ..
                } => Some(v),
                _ => None,
            }
        })
        .collect();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0].value.as_slice(), &[0xAB]);
    assert_eq!(values[0].result, dashboard_core::discovery::ReadResult::Ok);
    assert_eq!(values[1].failure, Some(FailureReason::Att(0x05)));
    assert_eq!(values[1].result, dashboard_core::discovery::ReadResult::Protected);
    assert_eq!(values[2].value.as_slice(), &(0..16).collect::<Vec<_>>());
    assert!(fixture.calls.borrow().contains(&Request::Type(1, u16::MAX, 0x2B2A)));
    assert!(!fixture.calls.borrow().contains(&Request::Read(12)));
    assert!(complete(&events, InventoryStage::Reads));
}

#[test]
fn descriptor_batches_resume_after_last_retained_handle_without_duplicates() {
    let mut fixture = Fixture::default();
    fixture.service(1, 14, 0xFFF0);
    fixture.characteristic(2, 0xFFF1, false);
    fixture.descriptors = (4..=12).collect();
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(false));
    let retained: Vec<_> = events
        .iter()
        .filter_map(|e| {
            match e {
                Event::Item {
                    item: InspectionItem::Descriptor(d),
                    ..
                } => Some(d.handle),
                _ => None,
            }
        })
        .collect();
    assert_eq!(retained, fixture.descriptors);
    let calls = fixture.calls.borrow();
    assert!(calls.contains(&Request::Find(4, 14)));
    assert!(calls.contains(&Request::Find(12, 14)));
    assert_eq!(calls.iter().filter(|r| matches!(r, Request::Find(..))).count(), 2);
}
#[test]
fn descriptor_error_retains_prefix_and_completed_characteristics_without_reading_values() {
    let mut fixture = Fixture::default();
    fixture.service(1, 20, 0x180A);
    fixture.characteristic(2, 0x2A29, true);
    fixture.descriptors = vec![4, 5];
    fixture.descriptor_error_after = Some(5);
    let (result, events) = run(&fixture);
    let failure = result.unwrap_err();
    assert_eq!(failure.stage, dashboard_core::discovery::FailureStage::Descriptors);
    assert_eq!(failure.reason, FailureReason::UnexpectedResponse);
    assert_eq!(failure.range, Some((4, 20)));
    assert!(complete(&events, InventoryStage::Characteristics));
    assert!(!complete(&events, InventoryStage::Descriptors));
    assert!(!complete(&events, InventoryStage::Reads));
    assert_eq!(
        events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Item {
                        item: InspectionItem::Descriptor(_),
                        ..
                    }
                )
            })
            .count(),
        2
    );
    assert!(!fixture.calls.borrow().iter().any(|r| matches!(r, Request::Read(_))));
}
#[test]
fn malformed_characteristic_preserves_valid_prefix_and_query_context() {
    let mut fixture = Fixture::default();
    fixture.service(1, 20, 0x180A);
    fixture.characteristic(2, 0x2A29, true);
    fixture.declarations.push((5, vec![2, 6, 0, 0x29]));
    let (result, events) = run(&fixture);
    let failure = result.unwrap_err();
    assert_eq!(failure.stage, dashboard_core::discovery::FailureStage::Characteristics);
    assert_eq!(failure.range, Some((1, 20)));
    assert_eq!(failure.last_accepted, Some((2, 3)));
    assert!(failure.elapsed_ms.is_some());
    assert_eq!(
        events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Item {
                        item: InspectionItem::Characteristic(_),
                        ..
                    }
                )
            })
            .count(),
        1
    );
    assert_eq!(fixture.calls.borrow().as_slice(), &[Request::Type(1, 20, 0x2803)]);
}
#[test]
fn incomplete_service_never_assigns_unknown_later_characteristics_as_descriptors() {
    let mut fixture = Fixture::default();
    fixture.service(1, 210, 0xFFF0);
    for i in 0..65 {
        fixture.characteristic(2 + i * 3, 0xFFF1, false);
    }
    let (result, events) = run(&fixture);
    assert_eq!(result, Ok(true));
    assert!(!complete(&events, InventoryStage::Characteristics));
    assert!(!complete(&events, InventoryStage::Descriptors));
    assert!(!complete(&events, InventoryStage::Reads));
    assert_eq!(
        events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Item {
                        item: InspectionItem::Characteristic(_),
                        ..
                    }
                )
            })
            .count(),
        64
    );
    assert!(
        !fixture
            .calls
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::Find(start, _) if *start >= 193))
    );
}
#[test]
fn hash_at_unexpected_handle_is_not_relabelled_as_the_discovered_characteristic() {
    let mut fixture = Fixture::default();
    fixture.service(1, 5, 0x1801);
    fixture.characteristic(2, 0x2B2A, true);
    fixture.hash = Some((9, vec![0xAA; 16]));
    let (result, events) = run(&fixture);
    assert_eq!(result.unwrap_err().reason, FailureReason::NotFound);
    let values: Vec<_> = events
        .iter()
        .filter_map(|e| {
            match e {
                Event::Item {
                    item: InspectionItem::Value(v),
                    ..
                } => Some(v),
                _ => None,
            }
        })
        .collect();
    assert_eq!(values.len(), 1);
    assert!(values[0].value.as_slice().is_empty());
    assert_eq!(values[0].handle, 3);
    assert_eq!(values[0].failure, Some(FailureReason::NotFound));
}
#[test]
fn all_protected_att_codes_and_unavailable_reads_continue_without_security_requests() {
    for code in [0x05, 0x08, 0x0C, 0x0F, 0x02, 0x0A] {
        let mut fixture = Fixture::default();
        fixture.service(1, 7, 0x180A);
        fixture.characteristic(2, 0x2A29, true);
        fixture.characteristic(5, 0x2A24, true);
        fixture.values = vec![(3, Err(FailureReason::Att(code))), (6, Ok(vec![1, 2, 3]))];
        let (result, events) = run(&fixture);
        assert_eq!(result, Ok(false));
        let values: Vec<_> = events
            .iter()
            .filter_map(|e| {
                match e {
                    Event::Item {
                        item: InspectionItem::Value(v),
                        ..
                    } => Some(v),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(values[0].failure, Some(FailureReason::Att(code)));
        assert_eq!(
            values[0].result,
            if [5, 8, 12, 15].contains(&code) {
                dashboard_core::discovery::ReadResult::Protected
            } else {
                dashboard_core::discovery::ReadResult::Unavailable
            }
        );
        assert_eq!(values[1].value.as_slice(), &[1, 2, 3]);
        assert!(complete(&events, InventoryStage::Reads));
    }
}
#[test]
fn transport_failure_records_failed_read_and_stops_before_next_request() {
    let mut fixture = Fixture::default();
    fixture.service(1, 7, 0x180A);
    fixture.characteristic(2, 0x2A29, true);
    fixture.characteristic(5, 0x2A24, true);
    fixture.values = vec![(3, Err(FailureReason::Hardware(0xF1)))];
    let (result, events) = run(&fixture);
    let failure = result.unwrap_err();
    assert_eq!(failure.reason, FailureReason::Hardware(0xF1));
    assert_eq!(failure.range, Some((3, 3)));
    assert!(!fixture.calls.borrow().contains(&Request::Read(6)));
    assert!(!complete(&events, InventoryStage::Reads));
}
