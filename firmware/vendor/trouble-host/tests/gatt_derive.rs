//! Ignored physical two-adapter integration scenario; compilation alone does not validate radio behavior.
//! External gatt_service generates typed handles, entry/CCCD counts, new(table) and exclusive-end handle_range().
//! External gatt_server generates server/service fields, new(table), new_default(name), new_with_config(gap),
//! get/set delegation, per-connection ATT-table get/set and Deref to AttributeServer. Name builders return
//! a length error; table exhaustion and reusing one-shot storage for values larger than MAX_SMALL_DATA_SIZE can panic.
//! The locked 0.6.0 generators discard struct/server-field docs and omit generated-member contracts;
//! this fixture documents their use without claiming the external expansions are fully documented.
#![allow(dead_code)]
use std::time::Duration;

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use tokio::select;
use trouble_host::prelude::*;

/// Shared serial-adapter discovery/setup and supervision checks.
mod common;

/// One concurrent logical link per test-side host.
const CONNECTIONS_MAX: usize = 1;
/// Three dynamic-channel slots per test-side host.
const L2CAP_CHANNELS_MAX: usize = 3;

/// Test-specific 128-bit service identity, supplied in Bluetooth little-endian byte order.
const SERVICE_UUID: Uuid = Uuid::new_long([
    0x00, 0x00, 0x10, 0x00, 0xb0, 0xcd, 0x11, 0xec, 0x87, 0x1f, 0xd4, 0x5d, 0xdf, 0x13, 0x88, 0x40,
]);
/// Test-specific byte characteristic identity used for discovery and read/write assertions.
const VALUE_UUID: Uuid = Uuid::new_long([
    0x00, 0x00, 0x10, 0x01, 0xb0, 0xcd, 0x11, 0xec, 0x87, 0x1f, 0xd4, 0x5d, 0xdf, 0x13, 0x88, 0x40,
]);

#[gatt_server(connections_max = CONNECTIONS_MAX, mutex_type = NoopRawMutex)]
/// Derived server combining the custom read/write service and battery-level service.
struct Server {
    /// Five generated custom characteristics and their descriptors.
    service: CustomService,
    /// Battery service with read/notify value and descriptive metadata.
    bas: BatteryService,
}

/// Expression-valued UUID fixture matching the custom service UUID; tests non-string macro input.
const UUID_U128: u128 = 0x408813df_5dd4_1f87_ec11_cdb000100000;

#[gatt_service(uuid = "408813df-5dd4-1f87-ec11-cdb000100000")]
/// Macro fixture with scalar, fixed-array and bounded-vector characteristic storage.
struct CustomService {
    #[descriptor(uuid = "2b20", value = "Read Only Descriptor", read)]
    #[characteristic(uuid = "408813df-5dd4-1f87-ec11-cdb001100000", value = 42, read, write, notify)]
    #[descriptor(uuid = "2b21", value = [0x01,0x02,0x03], read)]
    /// Read/write/notify byte initialized to 42, with text and three-byte read-only descriptors.
    pub value: u8,
    #[characteristic(uuid = UUID_U128, value = 123.321, read, write, notify)]
    /// Read/write/notify float initialized to 123.321; doc/descriptor attribute ordering is intentional.
    #[descriptor(uuid = "2b20", read, value = 42u16.to_le_bytes())] // empty descriptor
    pub second: f32,
    /// Two-byte read/write/notify value initialized to [0, 1].
    ///
    /// A separate paragraph verifies that multi-line docs survive characteristic generation.
    #[characteristic(uuid = "408815df-5dd4-1f87-ec11-cdb001100000", value = [0,1], read, write, notify)]
    pub third: [u8; 2],
    #[characteristic(uuid = "408816df-5dd4-1f87-ec11-cdb001100000", read, write, notify)]
    /// Empty bounded vector allowing at most three value bytes.
    pub fourth: heapless::Vec<u8, 3>,
    #[characteristic(uuid = "408817df-5dd4-1f87-ec11-cdb001100000", read, write, notify)]
    /// Empty bounded vector allowing at most 33 value bytes.
    pub fifth: heapless::Vec<u8, 33>,
}

#[gatt_service(uuid = service::BATTERY)]
/// Battery-service macro fixture with standard UUID constants and two read-only descriptors.
struct BatteryService {
    /// Read/notify battery percentage initialized to 10; descriptor advertises 0..=100, without a value validator.
    #[descriptor(uuid = descriptors::VALID_RANGE, read, value = [0, 100])]
    #[descriptor(uuid = descriptors::MEASUREMENT_DESCRIPTION, read, value = "Battery Level")]
    #[characteristic(uuid = characteristic::BATTERY_LEVEL, read, notify, value = 10)]
    level: u8,
}

#[tokio::test]
#[ignore = "requires two physical Zephyr HCI UART adapters"]
/// Verifies a rejected first write preserves value, then an accepted second write stores the expected increment.
/// Requires two physical adapters and a DEVICE_NAME of at most 22 UTF-8 bytes; 30-second timeout/task errors fail.
async fn gatt_client_server() {
    let _ = env_logger::try_init();
    let adapters = common::find_controllers();
    let peripheral = adapters[0].clone();
    let central = adapters[1].clone();

    let name = std::env::var("DEVICE_NAME").unwrap_or("TrouBLE".into());

    let peripheral_address: Address = Address::random([0xff, 0x9f, 0x1a, 0x05, 0xe4, 0xff]);

    let local = tokio::task::LocalSet::new();

    // Spawn peripheral
    let peripheral = local.spawn_local(async move {
        let controller_peripheral = common::create_controller(&peripheral).await;

        let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> = HostResources::new();
        let stack = trouble_host::new(controller_peripheral, &mut resources)
            .set_random_address(peripheral_address)
            .build();
        let mut runner = stack.runner();
        let mut peripheral = stack.peripheral();

        let gap = GapConfig::Peripheral(PeripheralConfig {
            name: &name,
            appearance: &appearance::power_device::GENERIC_POWER_DEVICE,
        });
        let server: Server = Server::new_with_config(
            gap,
        ).unwrap();

        // Random starting value to 'prove' the incremented value is correct
        let value: u8 = rand::random();
        // The first write will be rejected by the write callback, so value is not expected to change the first time
        let expected = value;
        server.set(&server.service.value, &value).unwrap();

        select! {
            r = runner.run() => {
                r
            }
            r = async {
                let mut adv_data = [0; 31];
                let adv_data_len = AdStructure::encode_slice(
                    &[AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED)],
                    &mut adv_data[..],
                ).unwrap();

                let mut scan_data = [0; 31];
                let scan_data_len = AdStructure::encode_slice(
                    &[AdStructure::CompleteLocalName(b"trouble-gatt-int")],
                    &mut scan_data[..],
                ).unwrap();

                let mut done = false;
                while !done {
                    println!("[peripheral] advertising");
                    let acceptor = peripheral.advertise(&Default::default(), Advertisement::ConnectableScannableUndirected {
                        adv_data: &adv_data[..adv_data_len],
                        scan_data: &scan_data[..scan_data_len],
                    }).await?;
                    let conn = acceptor.accept().await?.with_attribute_server(&server)?;
                    println!("[peripheral] connected");
                    let mut writes = 0;
                    while !done {
                        match conn.next().await {
                            GattConnectionEvent::Disconnected { reason } => {
                                println!("Disconnected: {:?}", reason);
                                break;
                            }
                            GattConnectionEvent::Gatt { event: GattEvent::Write(event) } => {
                                if writes == 0 {
                                    event.reject(AttErrorCode::VALUE_NOT_ALLOWED).unwrap().send().await;
                                    assert_eq!(server.get(&server.service.value).unwrap(), expected);
                                    writes += 1;
                                } else {
                                    let characteristic = server.server.table().find_characteristic_by_value_handle(event.handle()).unwrap();
                                    event.accept().unwrap().send().await;
                                    let value: u8 = conn.get(&characteristic).unwrap();
                                    assert_eq!(expected.wrapping_add(2), value);
                                    writes += 1;
                                    if writes == 2 {
                                        println!("expected value written twice, test pass");

                                        done = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    // NOTE: Ensure that adapter gets polled again
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                println!("[peripheral] done");
                Ok(())
            } => {
                r
            }
        }
    });

    // Spawn central
    let central = local.spawn_local(async move {
        let controller_central = common::create_controller(&central).await;
        let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> = HostResources::new();
        let stack = trouble_host::new(controller_central, &mut resources).build();
        let mut runner = stack.runner();
        let mut central = stack.central();

        select! {
            r = runner.run() => {
                r
            }
            r = async {
                let config = ConnectConfig {
                    connect_params: Default::default(),
                    scan_config: ScanConfig {
                        active: true,
                        filter_accept_list: &[peripheral_address],
                        ..Default::default()
                    },
                };

                println!("[central] connecting");
                let conn = central.connect(&config).await.unwrap();
                println!("[central] connected");
                tokio::time::sleep(Duration::from_secs(5)).await;

                println!("[central] creating gatt client");
                let client = GattClient::<common::Controller, DefaultPacketPool, 10>::new(&stack, &conn).await.unwrap();

                select! {
                    r = async {
                        client.task().await
                    } => {
                        r
                    }
                    r = async {
                        println!("[central] discovering services");
                        let services = client.services_by_uuid(&SERVICE_UUID).await.unwrap();

                        let service = services.first().unwrap().clone();

                        println!("[central] service discovered successfully");
                        let c: Characteristic<u8> = client.characteristic_by_uuid(&service, &VALUE_UUID).await.unwrap();

                        let mut data = [0; 1];
                        client.read_characteristic(&c, &mut data[..]).await.unwrap();
                        println!("[central] read value: {}", data[0]);
                        data[0] = data[0].wrapping_add(1);
                        println!("[central] write value: {}", data[0]);
                        if let Err(BleHostError::BleHost(Error::Att(AttErrorCode::VALUE_NOT_ALLOWED))) = client.write_characteristic(&c, &data[..]).await {
                            println!("[central] Frist write was rejected by write callback as expected.");
                        } else {
                            println!("[central] First write was expected to be rejected by server write callback!");
                            panic!();
                         }
                        data[0] = data[0].wrapping_add(1);
                        println!("[central] write value: {}", data[0]);
                        if let Ok(()) = client.write_characteristic(&c, &data[..]).await {
                            println!("[central] Second write accepted by server.");
                        } else {
                            println!("[central] Second write was expected to be accepted by the server!");
                            panic!();
                        }
                        println!("[central] write done");
                        Ok(())
                    } => {
                        r
                    }
                }
            } => {
                r
            }
        }
    });

    match tokio::time::timeout(Duration::from_secs(30), local).await {
        Ok(_) => {
            let (central, peripheral) = tokio::join!(central, peripheral);
            common::assert_task_result("central", central);
            common::assert_task_result("peripheral", peripheral);
            println!("Test completed successfully");
        }
        Err(e) => {
            println!("Test timed out: {:?}", e);
            panic!();
        }
    }
}
