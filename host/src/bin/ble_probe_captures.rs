//! Recover only previously saved BLE-probe evidence over normal USB CDC.
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use dashboard_core::discovery::capture_store::PAYLOAD_MAX;
use dashboard_core::discovery::capture_wire::{FrameStatus, Request, parse_list_payload};
use dashboard_host_validation::probe_captures::*;

const HELP: &str = "BLE probe saved captures (normal firmware USB, not BOOTSEL)\n\n  ble_probe_captures ports\n  \
                    ble_probe_captures list [--port COMx]\n  ble_probe_captures export SLOT NEW_DIRECTORY [--port \
                    COMx]\n  ble_probe_captures decode CAPTURE.blecap NEW_DIRECTORY\n\nSLOT is 1..4. NEW_DIRECTORY \
                    must not exist; its parent must exist.\nExport writes device-labelled .blecap, .json, .txt files \
                    and COMPLETE. Labels include the observed address and a capture hash suffix; full content IDs are \
                    in JSON/text. JSON includes derived evidence hints for a future Studio importer; it is not a \
                    connection profile.\nNo save, erase, pairing, adapter connection or ECU commands are \
                    supported.\nIf Windows shows a generic USB Serial Device name, select its COM port explicitly.\n";
fn transaction(
    request: Request,
    explicit: Option<&str>,
) -> Result<Vec<Reply>, String> {
    let ports = serialport::available_ports().map_err(|e| format!("enumerate USB serial ports: {e}"))?;
    let name = select_port(&ports, explicit)?;
    let mut port = serialport::new(&name, 115_200)
        .timeout(Duration::from_millis(250))
        .flow_control(serialport::FlowControl::None)
        .open()
        .map_err(|e| format!("open {name}: {e}"))?;
    // Assert DTR for CDC hosts; firmware must never interpret DTR/baud as reboot.
    port.write_data_terminal_ready(true)
        .map_err(|e| format!("set CDC DTR: {e}"))?;
    exchange(port.as_mut(), request, Duration::from_secs(5))
}
fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() || matches!(args[0].as_str(), "--help" | "-h" | "help") {
        println!("{HELP}");
        return Ok(());
    }
    match args[0].as_str() {
        "ports" if args.len() == 1 => {
            for p in serialport::available_ports().map_err(|e| e.to_string())? {
                if is_candidate(&p)
                    && let serialport::SerialPortType::UsbPort(usb) = p.port_type
                {
                    println!(
                        "{} {:04X}:{:04X} product={} serial={}",
                        p.port_name,
                        usb.vid,
                        usb.pid,
                        usb.product.as_deref().unwrap_or("unavailable"),
                        usb.serial_number.as_deref().unwrap_or("unavailable")
                    );
                }
            }
            Ok(())
        }
        "list" => {
            let explicit = port_argument(&args[1..])?;
            for entry in transaction(Request::List, explicit)? {
                let slot = entry.header.slot.ok_or("missing slot")? + 1;
                if entry.header.status == FrameStatus::Ok {
                    let (length, crc) =
                        parse_list_payload(&entry.payload).map_err(|e| format!("list payload: {e:?}"))?;
                    println!(
                        "Slot {slot}: saved generation={} bytes={length} CRC32={crc:08X}",
                        entry.header.generation
                    );
                } else {
                    println!("Slot {slot}: {:?}", entry.header.status);
                }
            }
            Ok(())
        }
        "export" if args.len() >= 3 => {
            let slot = match args[1].as_str() {
                "1" => 0,
                "2" => 1,
                "3" => 2,
                "4" => 3,
                _ => return Err("SLOT must be exactly 1, 2, 3 or 4".into()),
            };
            let explicit = port_argument(&args[3..])?;
            let directory = Path::new(&args[2]);
            if directory.exists() {
                return Err("output directory already exists".into());
            }
            let replies = transaction(Request::Get(slot), explicit)?;
            let reply = replies.first().ok_or("missing capture")?;
            let published = publish(directory, &reply.payload, Some(slot), Some(reply.header.generation))?;
            println!("Validated slot {} exported to {}", slot + 1, directory.display());
            print_artifacts(&published);
            Ok(())
        }
        "decode" if args.len() == 3 => {
            let mut bytes = Vec::new();
            File::open(&args[1])
                .map_err(|e| e.to_string())?
                .take((PAYLOAD_MAX + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let published = publish(Path::new(&args[2]), &bytes, None, None)?;
            println!("Validated offline capture decoded to {}", args[2]);
            print_artifacts(&published);
            Ok(())
        }
        _ => Err(format!("invalid arguments\n{HELP}")),
    }
}
fn print_artifacts(published: &PublishedCapture) {
    // Only derived ASCII labels reach the terminal, never the raw BLE name.
    println!("Capture ID: {}", published.capture_id);
    for extension in ["blecap", "json", "txt"] {
        println!("  {}.{extension}", published.file_stem);
    }
    println!("  COMPLETE");
}
fn port_argument(args: &[String]) -> Result<Option<&str>, String> {
    match args {
        [] => Ok(None),
        [flag, port] if flag == "--port" && !port.is_empty() => Ok(Some(port)),
        _ => Err("expected optional --port COMx".into()),
    }
}
fn main() {
    if let Err(error) = run(&env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("Capture export failed: {error}");
        std::process::exit(1);
    }
}
