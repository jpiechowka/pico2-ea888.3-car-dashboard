//! Native helper dispatcher for bounded prerequisite checks and explicit serving.
/// Dispatch OS-string arguments to descriptor enumeration, loopback serving or
/// prerequisite CLI. Serving accepts only an absolute `--repo` path; malformed
/// syntax exits 2, server failure exits 1, and fatal cleanup exits 70. No arguments
/// are forwarded to a shell, and serving itself does not authorize a device write.
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--internal-device-descriptors" {
        std::process::exit(studio_helper::devices::enumerate_child());
    }
    if args.first().is_some_and(|s| s == "serve") {
        let repo = match args.as_slice() {
            [_] => std::env::current_dir().unwrap_or_default(),
            [_, flag, path] if flag == "--repo" && std::path::Path::new(path).is_absolute() => path.into(),
            _ => {
                eprintln!("Usage: studio-helper serve [--repo <absolute repository>]");
                std::process::exit(2);
            }
        };
        match studio_helper::server::serve(repo) {
            Ok(()) => (),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(if e.starts_with("cleanup fatal:") { 70 } else { 1 });
            }
        }
    } else {
        std::process::exit(studio_helper::prerequisites::cli(args));
    }
}
