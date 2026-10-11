use dashboard_host_validation::diagnostics::bootstrap::{Bootstrap, Command, Error, Event, Status};

const SETTING: &[u8] = b"ENTER INTO SETTING MODE\r\n";
const OK: &[u8] = b"OK\r\n";
const DATA: &[u8] = b"OK\r\nENTER DATA MODE\r\n";

fn command(
    boot: &mut Bootstrap,
    now: u64,
    expected: &[u8],
) -> Command {
    let command = boot.poll(now).unwrap().unwrap();
    assert_eq!(command.bytes, expected);
    assert!(boot.poll(now).unwrap().is_none());
    command
}
fn reply(
    boot: &mut Bootstrap,
    bytes: &[u8],
    now: u64,
) -> Event {
    let mut last = Event::None;
    for &byte in bytes {
        assert_eq!(last, Event::None);
        last = boot.receive(byte, now).unwrap();
    }
    last
}
fn enter(boot: &mut Bootstrap) {
    let tx = command(boot, 50, b"+++");
    assert_eq!(boot.tx_completed(tx.token, 51), Ok(Event::None));
    assert_eq!(reply(boot, SETTING, 52), Event::None);
    let tx = command(boot, 52, b"AT\r\n");
    assert_eq!(boot.tx_completed(tx.token, 53), Ok(Event::None));
    assert_eq!(reply(boot, OK, 54), Event::None);
}

#[test]
fn every_banner_split_and_binary_tail_preserves_the_exact_boundary() {
    for split in 0..=DATA.len() {
        let mut boot = Bootstrap::new(0);
        enter(&mut boot);
        let tx = command(&mut boot, 54, b"AT+Q\r\n");
        boot.tx_completed(tx.token, 55).unwrap();
        let binary = [0, 0, 7, 0xE8, 3, 0x41, 5, 135, 0, 0, 0, 0];
        let mut got = Vec::new();
        let mut boundaries = 0;
        for chunk in [&DATA[..split], &DATA[split..], &binary] {
            for &byte in chunk {
                if boot.status() == Status::Online {
                    got.push(byte);
                } else if boot.receive(byte, 56).unwrap() == Event::Boundary {
                    boundaries += 1;
                }
            }
        }
        assert_eq!(boundaries, 1);
        assert_eq!(got, binary);
        assert!(boot.poll(10_000).unwrap().is_none());
    }
}

#[test]
fn quiet_input_guard_is_bounded_and_never_establishes_a_record_boundary() {
    let mut boot = Bootstrap::new(0);
    for now in 0..50 {
        assert!(boot.poll(now).unwrap().is_none());
    }
    assert_eq!(boot.receive(b'x', 49), Ok(Event::None));
    assert!(boot.poll(98).unwrap().is_none());
    command(&mut boot, 99, b"+++");
    assert_ne!(boot.status(), Status::Online);

    let mut busy = Bootstrap::new(0);
    for now in 0..=1000 {
        busy.receive(b'x', now).unwrap();
        assert!(busy.poll(now).unwrap().is_none());
    }
    assert_eq!(busy.poll(1001), Err(Error::QuietTimeout));
    assert_eq!(busy.status(), Status::Fault(Error::QuietTimeout));
}

#[test]
fn transmit_and_reply_timeouts_have_fixed_inclusive_deadlines() {
    let mut boot = Bootstrap::new(0);
    let tx = command(&mut boot, 50, b"+++");
    assert_eq!(boot.tx_completed(tx.token, 150), Ok(Event::None));
    assert!(boot.poll(550).unwrap().is_none());
    assert_eq!(boot.receive(b'E', 551), Err(Error::ReplyTimeout));

    let mut boot = Bootstrap::new(0);
    let tx = command(&mut boot, 50, b"+++");
    assert_eq!(boot.tx_completed(tx.token, 151), Err(Error::TxTimeout));

    let mut boot = Bootstrap::new(0);
    let tx = command(&mut boot, 50, b"+++");
    boot.tx_completed(tx.token, 51).unwrap();
    for (i, &byte) in SETTING.iter().enumerate() {
        if i + 1 == SETTING.len() {
            assert_eq!(boot.receive(byte, 551), Err(Error::ReplyTimeout));
        } else {
            boot.receive(byte, 60 + i as u64 * 20).unwrap();
        }
    }
}

#[test]
fn every_changed_reply_byte_and_extra_response_fail_closed() {
    for position in 0..SETTING.len() {
        let mut boot = Bootstrap::new(0);
        let tx = command(&mut boot, 50, b"+++");
        boot.tx_completed(tx.token, 51).unwrap();
        reply(&mut boot, &SETTING[..position], 52);
        assert_eq!(boot.receive(SETTING[position] ^ 1, 52), Err(Error::UnexpectedReply));
        assert!(boot.poll(1000).is_err());
    }
    let mut boot = Bootstrap::new(0);
    enter(&mut boot);
    assert_eq!(boot.receive(b'O', 54), Err(Error::UnexpectedReply));
}

#[test]
fn delayed_uart_ack_cannot_be_replaced_by_a_complete_response() {
    let mut boot = Bootstrap::new(0);
    let tx = command(&mut boot, 50, b"+++");
    assert_eq!(reply(&mut boot, SETTING, 51), Event::None);
    assert!(boot.poll(51).unwrap().is_none());
    assert_eq!(boot.tx_completed(tx.token, 52), Ok(Event::None));
    command(&mut boot, 52, b"AT\r\n");
    assert_eq!(boot.tx_completed(tx.token, 10_000), Err(Error::StaleTx));
    assert!(boot.poll(52).unwrap().is_none());
}

#[test]
fn discarded_restart_keeps_tokens_unique_and_clock_failure_is_terminal() {
    let mut boot = Bootstrap::new(0);
    let old = command(&mut boot, 50, b"+++");
    boot.fault(Error::ReceiveLoss);
    boot.restart_after_discard(100).unwrap();
    let current = command(&mut boot, 150, b"+++");
    assert_ne!(old.token, current.token);
    assert_eq!(boot.tx_completed(old.token, 10_000), Err(Error::StaleTx));
    assert_eq!(boot.tx_completed(current.token, 151), Ok(Event::None));
    assert_eq!(boot.poll(150), Err(Error::Clock));
    assert_eq!(boot.restart_after_discard(152), Err(Error::Clock));
}

#[test]
fn final_banner_before_uart_ack_cannot_publish_a_boundary_early_or_twice() {
    let mut boot = Bootstrap::new(0);
    enter(&mut boot);
    let tx = command(&mut boot, 54, b"AT+Q\r\n");
    assert_eq!(reply(&mut boot, DATA, 55), Event::None);
    assert_ne!(boot.status(), Status::Online);
    assert_eq!(boot.tx_completed(tx.token, 56), Ok(Event::Boundary));
    assert_eq!(boot.tx_completed(tx.token, u64::MAX), Err(Error::StaleTx));
    assert_eq!(boot.status(), Status::Online);
    assert!(boot.poll(56).unwrap().is_none());
}

#[test]
fn old_partial_banner_is_not_reused_after_loss() {
    for at in 1..SETTING.len() {
        let mut boot = Bootstrap::new(0);
        let tx = command(&mut boot, 50, b"+++");
        boot.tx_completed(tx.token, 51).unwrap();
        reply(&mut boot, &SETTING[..at], 52);
        boot.fault(Error::ReceiveLoss);
        boot.restart_after_discard(100).unwrap();
        let tx = command(&mut boot, 150, b"+++");
        boot.tx_completed(tx.token, 151).unwrap();
        // Even a suffix starting in 'E' eventually disagrees with the full banner.
        let mut result = Ok(Event::None);
        for &byte in SETTING[at..].iter().chain(b"\0") {
            result = boot.receive(byte, 152);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result, Err(Error::UnexpectedReply));
    }
}
