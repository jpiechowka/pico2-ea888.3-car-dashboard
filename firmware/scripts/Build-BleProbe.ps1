[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$')][string]$Name,
    [ValidateSet('stock', '280-70')][string]$Profile = 'stock',
    [switch]$Expert
)
$ErrorActionPreference = 'Stop'
$arguments = @((Join-Path $PSScriptRoot 'build_ble_probe.py'), $Name, '--profile', $Profile)
if ($Expert) { $arguments += '--expert' }
& python @arguments
if ($LASTEXITCODE -ne 0) { throw 'BLE probe build or practical image admission failed' }
