//! Ignored physical two-adapter integration scenario; compilation alone does not validate radio behavior.
use tokio::select;
use tokio::time::Duration;
use trouble_host::prelude::*;

/// Shared serial-adapter discovery/setup and supervision checks.
mod common;

/// One concurrent logical link per test-side host.
const CONNECTIONS_MAX: usize = 1;
/// Three dynamic-channel slots per test-side host.
const L2CAP_CHANNELS_MAX: usize = 3;

/// Exchanges ten indexed four-byte SDUs each way on SPSM 0xf2 through two physical HCI adapters.
/// Explicitly ignored; task errors or the 30-second overall timeout fail the scenario.
#[tokio::test]
#[ignore = "requires two physical Zephyr HCI UART adapters"]
async fn l2cap_connection_oriented_channels() {
    let _ = env_logger::try_init();
    let adapters = common::find_controllers();
    let peripheral = adapters[0].clone();
    let central = adapters[1].clone();

    let peripheral_address: Address = Address::random([0xff, 0x9f, 0x1a, 0x05, 0xe4, 0xff]);

    let local = tokio::task::LocalSet::new();

    /// Four bytes in each of ten messages sent in each direction.
    const PAYLOAD_LEN: usize = 4;

    // Spawn peripheral
    let peripheral = local.spawn_local(async move {
        let controller_peripheral = common::create_controller(&peripheral).await;

        let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> = HostResources::new();
        let stack = trouble_host::new(controller_peripheral, &mut resources)
            .set_random_address(peripheral_address)
            .register_l2cap_spsm(0xf2)
            .build();
        let mut runner = stack.runner();
        let mut peripheral = stack.peripheral();


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
                    &[AdStructure::CompleteLocalName(b"trouble-l2cap-int")],
                    &mut scan_data[..],
                ).unwrap();

                {
                    println!("[peripheral] advertising");
                    let acceptor = peripheral.advertise(&Default::default(), Advertisement::ConnectableScannableUndirected {
                        adv_data: &adv_data[..adv_data_len],
                        scan_data: &scan_data[..scan_data_len],
                    }).await?;
                    let conn = acceptor.accept().await?;
                    println!("[peripheral] connected");

                    let mut ch1 = L2capChannel::listen(&stack, &conn).accept(&Default::default()).await?;

                    println!("[peripheral] channel created");

                    // Size of payload we're expecting
                    let mut rx = [0; PAYLOAD_LEN];
                    for i in 0..10 {
                        let len = ch1.receive(&stack, &mut rx).await?;
                        assert_eq!(len, rx.len());
                        assert_eq!(rx, [i; PAYLOAD_LEN]);
                    }
                    println!("[peripheral] data received");

                    for i in 0..10 {
                        let tx = [i; PAYLOAD_LEN];
                        ch1.send(&stack, &tx).await?;
                    }
                    println!("[peripheral] data sent");
                }
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
                {
                    let conn = central.connect(&config).await.unwrap();
                    println!("[central] connected");
                    let mut ch1 = L2capChannel::create(&stack, &conn, 0xf2, &Default::default()).await?;
                    println!("[central] channel created");
                    for i in 0..10 {
                        let tx = [i; PAYLOAD_LEN];
                        ch1.send(&stack, &tx).await?;
                    }
                    println!("[central] data sent");
                    let mut rx = [0; PAYLOAD_LEN];
                    for i in 0..10 {
                        let len = ch1.receive(&stack, &mut rx).await?;
                        assert_eq!(len, rx.len());
                        assert_eq!(rx, [i; PAYLOAD_LEN]);
                    }
                    println!("[central] data received");
                }
                Ok(())
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
            panic!("L2CAP hardware exchange timed out");
        }
    }
}
