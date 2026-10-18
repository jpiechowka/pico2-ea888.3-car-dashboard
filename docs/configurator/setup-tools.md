# Explicit Windows tool setup

Run `configurator/Studio.ps1 -Setup` on Windows 10/11 x64 in **PowerShell 7.2 or
newer** (`pwsh`, preferably a currently supported release). Windows PowerShell
5.1 is rejected before setup performs I/O. Install **Python 3.11 or newer on PATH**,
rustup and Visual Studio C++ Build Tools with Windows SDK first. Python owns
bounded child-process execution during setup; it is not installed by this script.
[Install PowerShell](https://learn.microsoft.com/en-us/powershell/scripting/install/install-powershell-on-windows),
[Python for Windows](https://www.python.org/downloads/windows/),
[Rust installation](https://www.rust-lang.org/tools/install).

Setup installs the pinned Rust toolchain/targets, Trunk and wasm-bindgen, then
verifies and installs:

| Tool | Official release | Installed executable |
| --- | --- | --- |
| Binaryen 133 | WebAssembly/binaryen `version_133`, `binaryen-version_133-x86_64-windows.tar.gz` | `target/configurator-tools/bin/wasm-opt.exe` |
| picotool 2.3.1 | raspberrypi/pico-sdk-tools `v2.3.1-0`, `picotool-2.3.1-x64-win.zip` | `target/configurator-tools/bin/picotool.exe` |

Archive SHA256 values are pinned in `configurator/SetupTools.ps1`, checked before
extraction or execution, and were verified against the official GitHub release
asset digests. An invalid cached archive fails with its exact path and a remedy;
it is never installed. Archives stay under `target/configurator-tools` for reuse.
Only named executable members are extracted. Version checks and picotool load
help run before publishing the executables. No USB command, driver installer,
firmware flashing, or global PATH modification occurs.

`-Check`, normal build and serving do not invoke release installation. Setup's
final check reports any missing MSVC/SDK components. A checkout-wide setup lock
rejects simultaneous installers. Downloads have a 120-second deadline and
128 MiB bound, use unique temporary files and are checked before cache admission.
Extraction/version checks use isolated staging; executable replacements are
atomic individually, not one transaction for the pair. A failed run can be
retried explicitly. If a cached archive fails its hash, remove only the reported
archive and retry; no unverified executable is installed.

Tool subprocesses use the shared owned-process supervisor with stream limits,
deadlines and descendant cleanup. `configurator/Test-SetupTools.ps1` covers the
legacy-shell guard, locking, interrupted downloads, cached-package rejection and
failed publication without downloading tools or accessing USB.

The legacy tracked `firmware/tools/picotool.exe` remains unchanged at
2.2.0-a4 (6,121,118 bytes; SHA256
`c6f857b4d9ab2f70db52c09b4957da93ec41d865900f4a7d7ae30b49d0c945d0`).
Setup and the native helper use the ignored tool directory instead. The official
2.3.1 executable is 6,239,513 bytes, SHA256
`7c1bf5b49b3b1c3b8094ca300c9aac45d15d205adbc54fcc6d6d209f6f69e60d`.
Updating prerequisites does not rewrite the repository's tracked binaries.

## PC requirements and measurement limits

| Work | Host requirements and scope |
| --- | --- |
| Edit an already served Studio build | A browser with WebAssembly and browser storage. No Rust compiler is needed on that browser's computer. Local browser storage is origin-specific; export a project for a portable backup. |
| Build/serve Studio and run the native helper | The supplied launcher supports Windows 10/11 x64 and PowerShell 7.2+. Install rustup, Visual Studio C++ Build Tools with Windows SDK, and Python 3.11+ on PATH before Setup. Explicit `-Setup` downloads the pinned Rust toolchain, ARM/Wasm targets and editor/helper tools. Windows `tar.exe` is needed for the Binaryen archive. |
| Build and validate firmware | The same native tools, plus writable disk space for frozen source, compilation and retained artifacts. Practical image checks run before artifact admission. A successful build/download does not prove device transfer, reboot or hardware operation. |
| Experimental instruction proof | A separate research workload with substantially different resource behavior. It is not a prerequisite for normal practical image checks. |

For browser-to-helper use, serve Studio on `http://127.0.0.1:8080` or
`http://localhost:8080` on the helper's computer. The helper listens on loopback
port 8765 and accepts those two origins. A different `-Port` can serve the editor,
but does not change the helper's origin allowlist.

## Linux: editor and development checks

Linux support is deliberately partial. A modern browser can edit an already
served Studio build, including previews, local saves and project export. CI on
Ubuntu also builds the Wasm app, exercises browser failures, runs Rust/Python
tests and cross-compiles firmware. These checks do not qualify Arch/Fedora
installation, USB drivers or physical flashing. **The native helper and
PowerShell launcher still require Windows**; Linux pure-helper tests do not
change that boundary. Export a complete project and use Windows for the supported
Build & Flash workflow.

For a manual development setup, install a C/C++ build toolchain, Git, Python 3.11+
and rustup. Example system-package commands for recent distributions:

```sh
# Arch Linux
sudo pacman -S --needed base-devel git python rustup
# Fedora (use this instead of the Arch command)
sudo dnf install gcc gcc-c++ make git python3 rustup
```

Package availability varies by release. Use the official
[Arch rustup package](https://archlinux.org/packages/extra/x86_64/rustup/),
[Fedora package](https://packages.fedoraproject.org/pkgs/rustup/rustup/) or
[rustup installation instructions](https://rust-lang.github.io/rustup/installation/index.html).
From the repository root, install the dated pin in `rust-toolchain.toml`, then
build the editor using the same CLI versions as CI:

```sh
rustup toolchain install nightly-2026-10-18 --profile minimal --component rustfmt,clippy,llvm-tools-preview --target wasm32-unknown-unknown,thumbv8m.main-none-eabihf
cargo install trunk --version 0.21.14 --locked
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cd configurator
cargo fetch --locked
trunk serve --release --locked --address 127.0.0.1 --port 8080
```

Open `http://127.0.0.1:8080`; stop this foreground server with Ctrl+C. To check
without serving, use `trunk build --release --locked`. See the
[official Trunk guide](https://trunk-rs.github.io/trunk/) for platform tooling.
No Node/npm application toolchain is required. The default locked Trunk build
uses Rustls; alternate native-TLS builds can require additional system libraries.
For firmware compilation only, enter `firmware/` and run the documented
`cargo build` commands. Do not substitute `cargo run`, which can invoke a
hardware runner. No Linux hardware-validation claim is made.

### Disk space

The helper requires at least **4 GiB scratch reservation plus 256 MiB free-space
headroom** on the repository volume before admitting a build. This is an enforced
build admission bound, not an estimate of the complete installation size. Rust
installations, downloaded tools, Cargo caches, Studio output and retained build
attempts need additional space. No whole-installation minimum has been measured.
Use **Build storage** to inspect retained attempts and explicitly remove eligible
closed attempts. Active, protected and incomplete attempts cannot be treated as
ordinary disposable files.

### Tested host and memory evidence

A practical native build from source `d9c493d` completed on Windows 11 Pro 22631
x64, with an Intel Core i9-13900HX (24 cores / 32 logical processors) and about
31.8 GiB installed RAM. The build and artifact validation took 112.501 seconds;
the downloaded UF2 hash was checked. No USB transfer was performed. This is one
observed run, not a benchmark or a minimum hardware recommendation.

Only **17 late-start memory samples** were collected for that run. The largest
observed sum of private bytes for the tracked helper/build processes was
**185.7 MiB**. Sampling omitted the earlier compilation stages, so this is neither
a full-run peak nor the memory required to build. It excludes other applications;
system-wide commit usage must not be attributed entirely to this build. Keep RAM,
pagefile/commit headroom and free disk space available, but no minimum PC RAM or
commit specification has been qualified.

A prior experimental instruction-proof run sampled about **16.5 GB of process
private memory**. That belongs to the separate research path. It is not a RAM
requirement for browser editing or normal practical firmware checks, and it is
unrelated to the Pico's own RAM budget.
