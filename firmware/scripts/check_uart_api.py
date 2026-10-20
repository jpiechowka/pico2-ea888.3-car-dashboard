"""Compile actual target API ownership guards, with an accepted control case."""

import json
import os
from pathlib import Path
import sys

from cargo_diagnostics import compiler_errors

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / "host/scripts"))
from owned_process import run_checked  # noqa: E402

COMMON = """#![no_std]
#![no_main]
use panic_probe as _;
use embassy_rp::{Peri, peripherals as p, bind_interrupts};
use embassy_rp::uart::{BufferedUart, BufferedInterruptHandler, Config, Uart, Blocking};
bind_interrupts!(struct Irqs { UART0_IRQ => BufferedInterruptHandler<p::UART0>; });
"""
ARGS = "uart: Peri<'static, p::UART0>, tx: Peri<'static, p::PIN_0>, rx: Peri<'static, p::PIN_1>, tx_buf: &'static mut [u8], rx_buf: &'static mut [u8]"
CASES = [
    (
        "static_control",
        f"pub fn build({ARGS}) -> BufferedUart {{ BufferedUart::new(uart, tx, rx, Irqs, tx_buf, rx_buf, Config::default()) }}",
        None,
    ),
    (
        "local_rx_storage",
        f"pub fn build({ARGS}) -> BufferedUart {{ let mut local = [0; 16]; BufferedUart::new(uart, tx, rx, Irqs, tx_buf, &mut local, Config::default()) }}",
        "E0597",
    ),
    (
        "local_tx_storage",
        f"pub fn build({ARGS}) -> BufferedUart {{ let mut local = [0; 16]; BufferedUart::new(uart, tx, rx, Irqs, &mut local, rx_buf, Config::default()) }}",
        "E0597",
    ),
    (
        "reborrowed_uart",
        f"pub fn build(mut {ARGS}) -> BufferedUart {{ BufferedUart::new(uart.reborrow(), tx, rx, Irqs, tx_buf, rx_buf, Config::default()) }}",
        "E0597",
    ),
    (
        "borrowed_conversion",
        "pub fn build<'a>(uart: Uart<'a, Blocking>, tx: &'static mut [u8], rx: &'static mut [u8]) -> BufferedUart { uart.into_buffered::<p::UART0>(Irqs, tx, rx) }",
        "E0521",
    ),
    (
        "send_to_other_core",
        "fn require_send<T: Send>() {} pub fn check() { require_send::<BufferedUart>(); }",
        "E0277",
    ),
    (
        "share_between_cores",
        "fn require_sync<T: Sync>() {} pub fn check() { require_sync::<BufferedUart>(); }",
        "E0277",
    ),
]


def main() -> None:
    examples = ROOT / "pico2/examples"
    examples.mkdir(exist_ok=True)
    name = f"__uart_guard_{os.getpid()}"
    path = examples / f"{name}.rs"
    # Never overwrite or delete a pre-existing user file.
    with path.open("x", encoding="utf-8"):
        pass
    results = []
    try:
        for label, source, expected in CASES:
            path.write_text(COMMON + source, encoding="utf-8")
            proc = run_checked(
                [
                    "cargo",
                    "check",
                    "--locked",
                    "-p",
                    "dashboard-pico2",
                    "--example",
                    name,
                    "--message-format=json",
                ],
                cwd=ROOT,
                timeout=600,
                capture_output=True,
            )
            errors = compiler_errors(proc.stdout)
            codes = {error.code for error in errors}
            passed = (
                proc.returncode == 0 and not errors
                if expected is None
                else proc.returncode != 0 and codes == {expected}
            )
            print(
                f"{label}: {'PASS' if passed else 'FAIL'} (exit {proc.returncode}, errors {codes})"
            )
            if not passed:
                print(proc.stderr)
                for error in errors:
                    print(error.rendered)
            results.append({"case": label, "expected_error": expected, "passed": passed})
    finally:
        path.unlink()
    output = ROOT / "target/uart-api-guards.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    raise SystemExit(0 if all(r["passed"] for r in results) else 1)


if __name__ == "__main__":
    main()
