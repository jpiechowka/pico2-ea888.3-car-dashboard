//! Best-effort bounded diagnostics; pipe draining never waits for terminal I/O.
use std::io::{IsTerminal, Write};
use std::sync::OnceLock;
use std::sync::mpsc::{SyncSender, sync_channel};
static LOG: OnceLock<SyncSender<(bool, String)>> = OnceLock::new();
pub(crate) fn event(
    error: bool,
    context: &str,
    message: &str,
) {
    let sender = LOG.get_or_init(|| {
        let (sender, receiver) = sync_channel::<(bool, String)>(64);
        let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let _ = std::thread::Builder::new()
            .name("studio-terminal".into())
            .spawn(move || {
                while let Ok((error, text)) = receiver.recv() {
                    let mut stderr = std::io::stderr().lock();
                    if color {
                        let _ = write!(stderr, "\x1b[{}m", if error { 31 } else { 36 });
                    }
                    let _ = write!(stderr, "{text}");
                    if color {
                        let _ = write!(stderr, "\x1b[0m");
                    }
                    let _ = writeln!(stderr);
                }
            });
        sender
    });
    let text = format!(
        "[Studio {}] {}: {}",
        if error { "error" } else { "info" },
        context,
        message
    )
    .chars()
    .filter(|c| !c.is_control())
    .take(512)
    .collect();
    let _ = sender.try_send((error, text));
}
