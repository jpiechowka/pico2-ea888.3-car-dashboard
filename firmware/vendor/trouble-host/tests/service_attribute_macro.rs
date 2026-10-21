//! Fixture for the locked external gatt_service generator, including properties, UUID forms and ordinary fields.
//! Generated new(table) appends the service plus default-valued characteristics and returns their handles;
//! insufficient table capacity panics. ATTRIBUTE_COUNT/CCCD_COUNT count table entries/client configurations,
//! and handle_range() returns the service's exclusive-end ATT range. Large value storage uses one-shot statics.
//! The external 0.6.0 generator drops struct doc attributes and omits docs on these generated members;
//! owned fixture/field contracts here do not claim complete documentation of that external expansion.

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use trouble_host::prelude::*;

#[gatt_service(uuid = "7e701cf1-b1df-42a1-bb5f-6a1028c793b0")]
/// Compile/run fixture covering short/long UUIDs, characteristic properties and an ordinary field.
struct CustomService {
    #[characteristic(uuid = "2a37", read, write)]
    /// Read/write u8 with a 16-bit UUID, generated as a typed characteristic handle.
    short_uuid: u8,
    #[characteristic(uuid = "7e711cf1-b1df-42a1-bb5f-6a1028c793b0", write_without_response, indicate)]
    /// Float characteristic allowing commands and indications under a 128-bit UUID.
    long_uuid: f32,
    #[characteristic(uuid = "2a38", read, notify)]
    /// Eight-byte readable/notifiable characteristic with a generated CCCD.
    notify: [u8; 8],
    /// Ordinary u8 field retained without adding any ATT entries.
    non_characteristic_field: u8,
}

#[tokio::test]
/// Constructs one service and checks every generated handle plus the ordinary field is accessible.
async fn gatt_service_derive() {
    let mut table: AttributeTable<NoopRawMutex, 10> = AttributeTable::new();
    let service = CustomService::new(&mut table);

    // Check all fields of service have been generated and are accessible
    let _handle = service.handle;
    let _non_characteristic_field = service.non_characteristic_field;
    let _characteristic_short_uuid = service.short_uuid;
    let _characteristic_long_uuid = service.long_uuid;
    let _notify = service.notify;
}
