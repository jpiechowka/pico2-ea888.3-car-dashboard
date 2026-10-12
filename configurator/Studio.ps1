[CmdletBinding()]
param(
    [switch]$Serve,
    [ValidateRange(1024,65535)][int]$Port = 8080
)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$toolRoot = Join-Path $repoRoot 'target/configurator-tools'
$trunk = Join-Path $toolRoot 'bin/trunk.exe'
$previousColor = $env:NO_COLOR
Push-Location -LiteralPath $PSScriptRoot
try {
    # Trunk's CLI expects a boolean here; NO_COLOR=1 is not accepted.
    $env:NO_COLOR = 'true'
    if (-not (Test-Path -LiteralPath $trunk)) {
        & cargo install trunk --version 0.21.14 --locked --root $toolRoot
        if ($LASTEXITCODE -ne 0) { throw 'Trunk installation failed' }
    }
    $version = & $trunk --version
    if ($LASTEXITCODE -ne 0 -or $version.Trim() -ne 'trunk 0.21.14') {
        throw 'Expected Trunk 0.21.14 in target/configurator-tools/bin'
    }
    & rustup target add wasm32-unknown-unknown --toolchain nightly-2026-10-11
    if ($LASTEXITCODE -ne 0) { throw 'Wasm target installation failed' }
    if ($Serve) {
        & $trunk serve --release --locked --address 127.0.0.1 --port $Port
    } else {
        & $trunk build --release --locked
    }
    if ($LASTEXITCODE -ne 0) { throw 'Studio build or server failed' }
} finally {
    $env:NO_COLOR = $previousColor
    Pop-Location
}
