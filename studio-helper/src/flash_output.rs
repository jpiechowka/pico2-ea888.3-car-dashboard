//! Exact picotool 2.3.1 completion grammar, independent of lossy display logs.
//! Source: pinned main.cpp progress_bar, load_guts, verify_command, reboot_command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Load,
    Verify,
    Reboot,
}

pub fn complete(
    command: Command,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    ranges: usize,
) -> bool {
    if code != Some(0) || !stderr.is_empty() || stdout.len() > 16 * 1024 * 1024 || !stdout.is_ascii() {
        return false;
    }
    if command == Command::Reboot {
        return stdout == b"The device was rebooted into application mode.\n"
            || stdout == b"The device was rebooted into application mode.\r\n";
    }
    if !(1..=1024).contains(&ranges) {
        return false;
    }
    let prefix = if command == Command::Load {
        "Loading into Flash:   "
    } else {
        "Verifying Flash:      "
    };
    let mut rest = stdout;
    for _ in 0..ranges {
        let mut previous = None;
        loop {
            let Some(end) = rest.iter().position(|b| *b == b'\r') else {
                return false;
            };
            if end > 128 {
                return false;
            }
            let Ok(record) = std::str::from_utf8(&rest[..end]) else {
                return false;
            };
            let Some(tail) = record.strip_prefix(prefix).and_then(|s| s.strip_prefix('[')) else {
                return false;
            };
            if tail.len() < 35 {
                return false;
            }
            let (bar, suffix) = tail.split_at(30);
            let Some(number) = suffix.strip_prefix("]  ").and_then(|s| s.strip_suffix('%')) else {
                return false;
            };
            let Ok(percent) = number.parse::<usize>() else {
                return false;
            };
            if percent > 100
                || number != percent.to_string()
                || previous.is_none() && percent != 0
                || previous.is_some_and(|p| p >= percent)
                || !bar
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| b == if i < 30 * percent / 100 { b'=' } else { b' ' })
            {
                return false;
            }
            rest = &rest[end + 1..];
            previous = Some(percent);
            if percent == 100 {
                break;
            }
        }
        // The progress record CR is followed by the destructor newline (Windows CRLF).
        rest = match rest.strip_prefix(b"\r\n").or_else(|| rest.strip_prefix(b"\n")) {
            Some(tail) => tail,
            None => return false,
        };
        if command == Command::Verify {
            rest = match rest.strip_prefix(b"  OK\r\n").or_else(|| rest.strip_prefix(b"  OK\n")) {
                Some(tail) => tail,
                None => return false,
            };
        }
    }
    rest.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn group(
        command: Command,
        windows: bool,
    ) -> Vec<u8> {
        let prefix = if command == Command::Load {
            "Loading into Flash:   "
        } else {
            "Verifying Flash:      "
        };
        let mut text = String::new();
        for p in [0, 1, 50, 100] {
            text.push_str(&format!(
                "{prefix}[{}{}]  {p}%\r",
                "=".repeat(30 * p / 100),
                " ".repeat(30 - 30 * p / 100)
            ));
        }
        text.push_str(if windows { "\r\n" } else { "\n" });
        if command == Command::Verify {
            text.push_str(if windows { "  OK\r\n" } else { "  OK\n" });
        }
        text.into_bytes()
    }
    #[test]
    fn exact_complete_groups_and_windows_exit_codes() {
        for command in [Command::Load, Command::Verify] {
            for windows in [false, true] {
                let mut raw = group(command, windows);
                raw.extend(group(command, windows));
                assert!(complete(command, Some(0), &raw, &[], 2));
                assert!(!complete(command, Some(0), &raw, &[], 1));
                assert!(!complete(command, Some(-11), &raw, &[], 2));
                assert!(!complete(command, Some(0), &raw, b"\x1b", 2));
                for end in 0..raw.len() {
                    assert!(!complete(command, Some(0), &raw[..end], &[], 2));
                }
                let mut bad = raw.clone();
                bad[0] = b'X';
                assert!(!complete(command, Some(0), &bad, &[], 2));
                for at in 0..raw.len() {
                    let mut bad = raw.clone();
                    bad[at] = 0xFF;
                    assert!(!complete(command, Some(0), &bad, &[], 2));
                }
                raw.extend_from_slice(b"  OK\n");
                assert!(!complete(command, Some(0), &raw, &[], 2));
            }
        }
    }
    #[test]
    fn reboot_and_no_ranges_cannot_manufacture_verification() {
        assert!(complete(
            Command::Reboot,
            Some(0),
            b"The device was rebooted into application mode.\r\n",
            &[],
            0
        ));
        assert!(!complete(Command::Verify, Some(0), b"No ranges to verify.\n", &[], 0));
        assert!(!complete(
            Command::Reboot,
            Some(0),
            b"The device was rebooted into BOOTSEL mode.\n",
            &[],
            0
        ));
    }
}
