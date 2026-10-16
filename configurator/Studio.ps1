[CmdletBinding(DefaultParameterSetName='Build')]
param(
    [Parameter(ParameterSetName='Serve')][switch]$Serve,
    [Parameter(Mandatory,ParameterSetName='Helper')][switch]$Helper,
    [Parameter(Mandatory,ParameterSetName='Check')][switch]$Check,
    [Parameter(ParameterSetName='Check')][switch]$Json,
    [Parameter(Mandatory,ParameterSetName='Setup')][switch]$Setup,
    [Parameter(ParameterSetName='Serve')][ValidateRange(1024,65535)][int]$Port = 8080
)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$toolRoot = Join-Path $repoRoot 'target/configurator-tools'
$trunk = Join-Path $toolRoot 'bin/trunk.exe'
$helperRoot = Join-Path $repoRoot 'target/studio-helper'
$helperExecutable = Join-Path $helperRoot 'debug/studio-helper.exe'
$toolchainText = Get-Content -LiteralPath (Join-Path $repoRoot 'rust-toolchain.toml') -Raw
if ($toolchainText -notmatch '(?m)^channel = "(nightly-\d{4}-\d{2}-\d{2})"\s*$') { throw 'Expected dated nightly in rust-toolchain.toml' }
$toolchain = $Matches[1]
if (-not [Environment]::Is64BitOperatingSystem -or [Environment]::OSVersion.Platform -ne 'Win32NT' -or [Environment]::OSVersion.Version.Major -lt 10 -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Native Studio launcher requires Windows 10/11 x64. Static browser editing/export is independent.' }
if ($Helper) {
    if (-not (Test-Path -LiteralPath $helperExecutable -PathType Leaf)) { throw 'Native helper is missing. Run .\configurator\Studio.ps1 -Setup explicitly first.' }
    & $helperExecutable serve --repo $repoRoot
    exit $LASTEXITCODE
}
if ($Check) {
    if (-not (Test-Path -LiteralPath $helperExecutable -PathType Leaf)) {
        if ($Json) { '{"schema_version":1,"ready":false,"checks":[{"id":"native_helper","status":"missing","required":true,"evidence":"Run configurator/Studio.ps1 -Setup to compile the prerequisite checker"}]}' }
        else { Write-Output 'Prerequisite checker is missing. Run .\configurator\Studio.ps1 -Setup explicitly. Check did not install or build anything.' }
        exit 4
    }
    $checkArgs = @('check', '--repo', $repoRoot)
    if ($Json) { $checkArgs += '--json' }
    & $helperExecutable @checkArgs
    exit $LASTEXITCODE
}
$previousColor = $env:NO_COLOR
$previousAutoInstall = $env:RUSTUP_AUTO_INSTALL
$previousPath = $env:PATH
$previousToolchain = $env:RUSTUP_TOOLCHAIN
Push-Location -LiteralPath $PSScriptRoot
try {
    $env:NO_COLOR = 'true'
    $env:RUSTUP_AUTO_INSTALL = '0'
    $env:RUSTUP_TOOLCHAIN = $toolchain
    $env:PATH = (Join-Path $toolRoot 'bin') + [IO.Path]::PathSeparator + $previousPath
    if ($Setup) {
        if (-not (Get-Command rustup -CommandType Application -ErrorAction SilentlyContinue)) {
            throw 'Rustup is missing. Install Rust for Windows x64 from https://rustup.rs, reopen PowerShell, then rerun configurator/Studio.ps1 -Setup. Install Visual Studio Build Tools with Desktop development with C++ and the Windows SDK when prompted.'
        }
        Write-Host "Explicit setup: $toolchain, ARM/Wasm targets, LLVM tools, pinned editor CLIs and the prerequisite checker. Downloads may occur."
        & rustup toolchain install $toolchain --profile minimal --component llvm-tools-preview --component rustfmt --component clippy --target thumbv8m.main-none-eabihf --target wasm32-unknown-unknown
        if ($LASTEXITCODE -ne 0) { throw 'Pinned Rust setup failed. Install Rust from rustup.rs and MSVC Build Tools with Windows SDK if missing.' }
        & cargo "+$toolchain" install trunk --version 0.21.14 --locked --root $toolRoot
        if ($LASTEXITCODE -ne 0) { throw 'Trunk installation failed' }
        & cargo "+$toolchain" install wasm-bindgen-cli --version 0.2.129 --locked --root $toolRoot
        if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen installation failed' }
        . (Join-Path $PSScriptRoot 'SetupTools.ps1')
        Install-StudioReleaseTools -Repository $repoRoot
        & cargo "+$toolchain" build --manifest-path (Join-Path $repoRoot 'studio-helper/Cargo.toml') --locked --target-dir $helperRoot
        if ($LASTEXITCODE -ne 0) { throw 'Prerequisite checker compilation failed' }
        & cargo "+$toolchain" fetch --manifest-path (Join-Path $repoRoot 'configurator/Cargo.toml') --locked
        if ($LASTEXITCODE -ne 0) { throw 'Studio locked dependency fetch failed' }
        Write-Host 'Setup installed the pinned Rust/editor tools, Binaryen 133 and picotool 2.3.1. Python and MSVC/SDK may still require the manual steps listed by Check.'
        & $helperExecutable check --repo $repoRoot
        exit $LASTEXITCODE
    }
    if (-not (Test-Path -LiteralPath $trunk -PathType Leaf)) { throw 'Trunk is missing. Run .\configurator\Studio.ps1 -Setup explicitly first.' }
    $version = & $trunk --version
    if ($LASTEXITCODE -ne 0 -or $version.Trim() -ne 'trunk 0.21.14') { throw 'Expected Trunk 0.21.14 in target/configurator-tools/bin. Run -Setup.' }
    $targets = & rustup target list --installed --toolchain $toolchain
    if ($LASTEXITCODE -ne 0 -or $targets -notcontains 'wasm32-unknown-unknown') { throw 'Pinned Wasm target is missing. Run -Setup.' }
    # Existing static editor build/serve workflow. No implicit rustup/cargo installation.
    if ($Serve) { & $trunk serve --release --locked --offline --address 127.0.0.1 --port $Port }
    else { & $trunk build --release --locked --offline }
    if ($LASTEXITCODE -ne 0) { throw 'Studio build or server failed. Missing locked dependencies require an explicit setup/fetch step.' }
} finally {
    $env:NO_COLOR = $previousColor
    $env:RUSTUP_AUTO_INSTALL = $previousAutoInstall
    $env:PATH = $previousPath
    $env:RUSTUP_TOOLCHAIN = $previousToolchain
    Pop-Location
}
