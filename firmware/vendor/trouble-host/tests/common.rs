use std::path::PathBuf;
use std::{fmt::Debug, io};

use bt_hci::controller::ExternalController;
use bt_hci_serial::SerialTransport;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embedded_io_adapters::tokio_1::FromTokio;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::time::Duration;
use tokio_serial::{DataBits, Parity, SerialStream, StopBits};

#[allow(dead_code)]
pub type Controller = ExternalController<
    SerialTransport<NoopRawMutex, FromTokio<ReadHalf<SerialStream>>, FromTokio<WriteHalf<SerialStream>>>,
    10,
>;

pub fn find_controllers() -> Vec<PathBuf> {
    let folder = "/dev/serial/by-id";
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        let file_name = path.file_name().unwrap().to_string_lossy();
        if file_name.starts_with("usb-ZEPHYR_Zephyr_HCI_UART_sample") {
            paths.push(path.to_path_buf());
        }
    }
    paths.sort();
    assert!(
        paths.len() >= 2,
        "these hardware tests require two Zephyr HCI UART adapters"
    );
    paths
}

pub fn assert_task_result<E: Debug, J: Debug>(name: &str, result: Result<Result<(), E>, J>) {
    result
        .unwrap_or_else(|error| panic!("{name} task failed: {error:?}"))
        .unwrap_or_else(|error| panic!("{name} operation failed: {error:?}"));
}

fn drain_input(mut read: impl FnMut(&mut [u8]) -> io::Result<usize>) -> io::Result<()> {
    let mut remaining = 65536;
    let mut buffer = [0; 256];
    // Both input volume and interrupted calls are bounded.
    for _ in 0..1024 {
        match read(&mut buffer) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) if count <= buffer.len() && count <= remaining => remaining -= count,
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
        if remaining == 0 {
            break;
        }
    }
    Err(io::ErrorKind::TimedOut.into())
}

#[allow(unused)]
pub(crate) async fn create_controller(
    port: &PathBuf,
) -> ExternalController<
    SerialTransport<NoopRawMutex, FromTokio<ReadHalf<SerialStream>>, FromTokio<WriteHalf<SerialStream>>>,
    10,
> {
    let port = port.to_string_lossy();
    let baudrate = 1000000;
    let mut port = SerialStream::open(
        &tokio_serial::new(port, baudrate)
            .baud_rate(baudrate)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One),
    )
    .unwrap();

    // Drain input
    tokio::time::sleep(Duration::from_secs(1)).await;
    drain_input(|bytes| port.try_read(bytes)).expect("controller input drain failed");

    let (reader, writer) = tokio::io::split(port);

    let reader = embedded_io_adapters::tokio_1::FromTokio::new(reader);
    let writer = embedded_io_adapters::tokio_1::FromTokio::new(writer);

    let driver: SerialTransport<NoopRawMutex, _, _> = SerialTransport::new(reader, writer);
    ExternalController::new(driver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_errors_cannot_be_reported_as_success() {
        assert_task_result("success", Ok::<_, &str>(Ok::<_, &str>(())));
        for result in [Err("join"), Ok(Err("operation"))] {
            assert!(std::panic::catch_unwind(|| assert_task_result("failed", result)).is_err());
        }
    }

    #[test]
    fn input_drain_bounds_volume_and_interrupts_and_preserves_errors() {
        let mut calls = 0;
        drain_input(|bytes| {
            calls += 1;
            if calls == 1 {
                Ok(bytes.len())
            } else {
                Err(io::ErrorKind::WouldBlock.into())
            }
        })
        .unwrap();
        assert_eq!(calls, 2);
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::PermissionDenied] {
            assert_eq!(drain_input(|_| Err(kind.into())).unwrap_err().kind(), kind);
        }
        assert_eq!(drain_input(|_| Ok(0)).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(drain_input(|_| Ok(257)).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            drain_input(|bytes| Ok(bytes.len())).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            drain_input(|_| Err(io::ErrorKind::Interrupted.into()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
}
