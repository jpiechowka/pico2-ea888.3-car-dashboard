[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9_-]*$')][string]$Name,
    [ValidateSet('stock', '280-70')][string]$Profile = 'stock',
    [ValidateSet('Demo', 'Vehicle')][string]$Mode = 'Demo',
    [string]$Config,
    [switch]$SerialDisplay,
    [switch]$FullOutline
)
$ErrorActionPreference = 'Stop'
$firmwareRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$features = [Collections.Generic.List[string]]::new()
if ($Mode -eq 'Demo') { $features.Add('demo') } else { $features.Add('longan-rx12') }
if ($Profile -eq '280-70') { $features.Add('cpu280-spi70-1v30') }
$features.Add('frame-trace')
$features.Add('te-trace')
if ($SerialDisplay) { $features.Add('serial-display') }
$arguments = @((Join-Path $PSScriptRoot 'capture_target_bench.py'), $Name, '--firmware', '--features', ($features -join ','))
if ($FullOutline) { $arguments += '--no-default-features' }
if ($PSBoundParameters.ContainsKey('Config')) {
    if ([string]::IsNullOrWhiteSpace($Config)) { throw 'Config must name a binary .eav file' }
    $selected = (Resolve-Path -LiteralPath $Config).Path
    $arguments += @('--dashboard-config', $selected)
}
Push-Location -LiteralPath $firmwareRoot
try {
    # Build, verify memory/flash helpers and freeze exact sources/config/ELF.
    # The capture refuses existing names. It never opens or flashes a device.
    & python @arguments
    if ($LASTEXITCODE -ne 0) { throw "Dashboard build failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}
