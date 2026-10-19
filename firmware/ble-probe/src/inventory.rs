//! Bounded read-only inventory orchestration shared with native flow regressions.
use core::future::Future;
use core::ops::ControlFlow;
use core::pin::pin;

use dashboard_core::discovery::{
    Characteristic,
    Descriptor as ProbeDescriptor,
    Event,
    FailureReason,
    FailureStage,
    InfoValue,
    InspectionFailure,
    InspectionItem,
    InventoryStage,
    Payload,
    ReadResult,
    Service,
    Uuid as ProbeUuid,
    characteristic_from_wire,
    descriptor_range,
    may_read,
};
use embassy_time::{Duration, Instant, with_timeout};
use heapless::Vec;
use trouble_host::att::AttErrorCode;
use trouble_host::prelude::Uuid;

use crate::errors::device_info_att_result;

/// I/O boundary only; the production adapter calls the locked GATT procedures.
pub trait ServiceRecord {
    fn snapshot(&self) -> Result<Service, FailureReason>;
}
impl ServiceRecord for Service {
    fn snapshot(&self) -> Result<Service, FailureReason> { Ok(*self) }
}
pub trait InventoryClient {
    type Service: ServiceRecord;
    // Fill caller-owned storage; the timed wrapper returns only a small status.
    async fn services(
        &self,
        output: &mut Vec<Self::Service, 24>,
    ) -> Result<(), FailureReason>;
    async fn read_by_type(
        &self,
        start: u16,
        end: u16,
        uuid: &Uuid,
        callback: impl FnMut(u16, &[u8]) -> ControlFlow<()>,
    ) -> Result<Option<()>, FailureReason>;
    async fn find_information(
        &self,
        start: u16,
        end: u16,
        callback: impl FnMut(u16, Uuid) -> ControlFlow<()>,
    ) -> Result<Option<()>, FailureReason>;
    async fn read_handle(
        &self,
        handle: u16,
        bytes: &mut [u8],
    ) -> Result<usize, FailureReason>;
}
fn elapsed_ms(started: Instant) -> u32 {
    Instant::now()
        .duration_since(started)
        .as_millis()
        .min(u64::from(u32::MAX)) as u32
}
async fn att<T>(
    stage: FailureStage,
    future: impl Future<Output = Result<T, FailureReason>>,
) -> Result<T, InspectionFailure> {
    let started = Instant::now();
    let result = match with_timeout(Duration::from_secs(3), future).await {
        Ok(result) => result,
        Err(_) => Err(FailureReason::Timeout),
    };
    result.map_err(|reason| {
        let mut failure = InspectionFailure::new(stage, reason);
        failure.elapsed_ms = Some(elapsed_ms(started));
        failure
    })
}
pub async fn inventory(
    client: &impl InventoryClient,
    revision: u32,
    emit: &impl AsyncFn(Event),
) -> Result<bool, InspectionFailure> {
    let mut services = Vec::new();
    att(FailureStage::Services, pin!(client.services(&mut services))).await?;
    for service in &services {
        let service = service
            .snapshot()
            .map_err(|reason| InspectionFailure::new(FailureStage::Services, reason))?;
        emit(Event::Item {
            revision,
            item: InspectionItem::Service(service),
        })
        .await;
    }
    emit(Event::Item {
        revision,
        item: InspectionItem::StageComplete(InventoryStage::Services),
    })
    .await;
    let mut total = 0usize;
    let mut descriptor_total = 0usize;
    let mut characteristics_complete = true;
    let mut descriptors_complete = true;
    let mut reads_complete = true;
    let mut service_complete = [false; 24];
    // Each service appends one contiguous run, already validated in declaration
    // order. Keep its indices even when mixed UUID widths reorder the services.
    // Every endpoint is at most64; unvisited/empty services retain an empty run.
    let mut service_rows = [[0u8; 2]; 24];
    let mut rows: Vec<Characteristic, 64> = Vec::new();
    let mut readable: Vec<(u16, u16, u16), 16> = Vec::new();
    for (service_index, service) in services.iter().enumerate() {
        if total == 64 {
            characteristics_complete = false;
            break;
        }
        let service = service
            .snapshot()
            .map_err(|reason| InspectionFailure::new(FailureStage::Characteristics, reason))?;
        let start = service.start;
        let end = service.end;
        let service_uuid = service.uuid;
        let first_row = rows.len();
        let mut complete = true;
        let mut malformed = None;
        let mut previous_value = start;
        let mut last_accepted = None;
        let started = Instant::now();
        let result = att(
            FailureStage::Characteristics,
            pin!(
                client.read_by_type(start, end, &Uuid::new_short(0x2803), |declaration, bytes| {
                    let row = match characteristic_from_wire(start, end, previous_value, declaration, bytes) {
                        Ok(row) => row,
                        Err(reason) => {
                            malformed = Some(reason);
                            return ControlFlow::Break(());
                        }
                    };
                    if total == 64 {
                        complete = false;
                        return ControlFlow::Break(());
                    }
                    if rows.push(row).is_err() {
                        complete = false;
                        return ControlFlow::Break(());
                    }
                    previous_value = row.handle;
                    last_accepted = Some((declaration, row.handle));
                    total += 1;
                    ControlFlow::Continue(())
                })
            ),
        )
        .await;
        let query_elapsed_ms = elapsed_ms(started);
        service_rows[service_index] = [first_row as u8, rows.len() as u8];
        // Keep valid rows preceding a malformed response or the capacity boundary.
        for &row in &rows[first_row..] {
            if may_read(service_uuid, row.uuid, row.properties)
                && let Some(uuid) = row.uuid.short()
                && readable.push((start, uuid, row.handle)).is_err()
            {
                reads_complete = false;
            }
            emit(Event::Item {
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
        service_complete[service_index] = complete;
        characteristics_complete &= complete;
    }
    if characteristics_complete {
        emit(Event::Item {
            revision,
            item: InspectionItem::StageComplete(InventoryStage::Characteristics),
        })
        .await;
    }
    // Descriptor work cannot erase completion of characteristic enumeration.
    for (service_index, service) in services.iter().enumerate() {
        let service = service
            .snapshot()
            .map_err(|reason| InspectionFailure::new(FailureStage::Descriptors, reason))?;
        let [first, end] = service_rows[service_index];
        let service_rows = &rows[usize::from(first)..usize::from(end)];
        descriptors_complete &= service_complete[service_index];
        for (index, row) in service_rows.iter().enumerate() {
            if let Some((from, to)) = descriptor_range(service, service_rows, index, service_complete[service_index]) {
                descriptors_complete &= descriptors(
                    client,
                    revision,
                    service.start,
                    row.handle,
                    (from, to),
                    &mut descriptor_total,
                    emit,
                )
                .await?;
            }
        }
    }
    if descriptors_complete {
        emit(Event::Item {
            revision,
            item: InspectionItem::StageComplete(InventoryStage::Descriptors),
        })
        .await;
    }
    reads_complete &= characteristics_complete;
    for &(service_start, uuid, handle) in &readable {
        let mut bytes = [0u8; 64];
        let started = Instant::now();
        let failure = |reason| {
            let mut failure = InspectionFailure::new(FailureStage::DeviceInfo, reason);
            failure.range = Some((handle, handle));
            failure.elapsed_ms = Some(elapsed_ms(started));
            failure
        };
        let read = async {
            if uuid == 0x2B2A {
                // Core Vol3 PartG7.3 requires ReadUsingCharacteristicUUID.
                let mut length = None;
                client
                    .read_by_type(1, u16::MAX, &Uuid::new_short(0x2B2A), |found_handle, value| {
                        if found_handle == handle {
                            let count = value.len().min(bytes.len());
                            bytes[..count].copy_from_slice(&value[..count]);
                            length = Some(count);
                            ControlFlow::Break(())
                        } else {
                            ControlFlow::Continue(())
                        }
                    })
                    .await?;
                length.ok_or(FailureReason::NotFound)
            } else {
                client.read_handle(handle, &mut bytes).await
            }
        };
        let read_result = with_timeout(Duration::from_secs(3), pin!(read)).await;
        let (result, value, read_failure) = match read_result {
            Err(_) => (ReadResult::Error, Payload::empty(), Some(FailureReason::Timeout)),
            Ok(Ok(length)) => {
                let mut value = Payload::new(&bytes[..length]);
                value.truncated = length == bytes.len();
                (ReadResult::Ok, value, None)
            }
            Ok(Err(FailureReason::Att(code))) => {
                (
                    device_info_att_result(AttErrorCode::new(code)),
                    Payload::empty(),
                    Some(FailureReason::Att(code)),
                )
            }
            Ok(Err(error)) => (ReadResult::Error, Payload::empty(), Some(error)),
        };
        emit(Event::Item {
            revision,
            item: InspectionItem::Value(InfoValue {
                service_start,
                handle,
                uuid,
                value,
                result,
                failure: read_failure,
                elapsed_ms: elapsed_ms(started),
            }),
        })
        .await;
        if result == ReadResult::Error
            && let Some(reason) = read_failure
        {
            return Err(failure(reason));
        }
    }
    if reads_complete {
        emit(Event::Item {
            revision,
            item: InspectionItem::StageComplete(InventoryStage::Reads),
        })
        .await;
    }
    Ok(!(characteristics_complete && descriptors_complete && reads_complete))
}

async fn descriptors(
    client: &impl InventoryClient,
    revision: u32,
    service_start: u16,
    characteristic_handle: u16,
    range: (u16, u16),
    total: &mut usize,
    emit: &impl AsyncFn(Event),
) -> Result<bool, InspectionFailure> {
    let (mut start, end) = range;
    loop {
        if *total == 128 {
            return Ok(false);
        }
        // Stream small batches; no128-record temporary on either core's stack.
        let mut batch: Vec<ProbeDescriptor, 8> = Vec::new();
        let mut malformed = false;
        let result = att(
            FailureStage::Descriptors,
            pin!(client.find_information(start, end, |handle, uuid| {
                let Some(uuid) = ProbeUuid::from_wire(uuid.as_raw()) else {
                    malformed = true;
                    return ControlFlow::Break(());
                };
                let _ = batch.push(ProbeDescriptor {
                    service_start,
                    characteristic_handle,
                    handle,
                    uuid,
                });
                if batch.len() == 8 || *total + batch.len() == 128 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })),
        )
        .await;
        let last = batch.last().map(|row| row.handle);
        *total += batch.len();
        for &row in &batch {
            emit(Event::Item {
                revision,
                item: InspectionItem::Descriptor(row),
            })
            .await;
        }
        let broke = result
            .map_err(|mut failure| {
                failure.range = Some((start, end));
                failure
            })?
            .is_some();
        if malformed {
            return Err(InspectionFailure::new(
                FailureStage::Descriptors,
                FailureReason::InvalidValue,
            ));
        }
        if !broke || last == Some(end) {
            return Ok(true);
        }
        start = last
            .and_then(|last| last.checked_add(1))
            .ok_or_else(|| InspectionFailure::new(FailureStage::Descriptors, FailureReason::InvalidValue))?;
    }
}
