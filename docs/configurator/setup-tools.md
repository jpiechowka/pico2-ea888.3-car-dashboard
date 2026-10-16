# Explicit Windows tool setup

Run `configurator/Studio.ps1 -Setup` on Windows 10/11 x64. Setup installs the
pinned Rust toolchain/targets, Trunk and wasm-bindgen, then verifies and installs:

| Tool | Official release | Installed executable |
| --- | --- | --- |
| Binaryen 133 | WebAssembly/binaryen `version_133`, `binaryen-version_133-x86_64-windows.tar.gz` | `target/configurator-tools/bin/wasm-opt.exe` |
| picotool 2.3.1 | raspberrypi/pico-sdk-tools `v2.3.1-0`, `picotool-2.3.1-x64-win.zip` | `firmware/tools/picotool.exe` |

Archive SHA256 values are pinned in `configurator/SetupTools.ps1`, checked before
extraction or execution, and were verified against the official GitHub release
asset digests. An invalid cached archive fails with its exact path and a remedy;
it is never installed. Archives stay under `target/configurator-tools` for reuse.
Only named executable members are extracted. Version checks and picotool load
help run before publishing the executables. No USB command, driver installer,
firmware flashing, or global PATH modification occurs.

`-Check`, normal build and serving do not invoke release installation. Setup's
remaining checker results may require Python or Visual Studio Build Tools with
C++ and Windows SDK; these system installers remain explicit manual actions.
`configurator/Test-SetupTools.ps1` covers PowerShell parsing and cached-package
hash/size rejection without downloading tools or accessing USB.
