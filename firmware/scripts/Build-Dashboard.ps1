[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9_-]*$')][string]$Name,
    [string]$Profile = 'stock',
    [ValidateSet('Demo', 'Vehicle')][string]$Mode = 'Demo',
    [string]$Config,
    [switch]$SerialDisplay,
    [switch]$FullOutline,
    [string]$CpuHz,
    [string]$SpiHz,
    [string]$VselMv,
    [ValidateSet('3','2','s','z')][string]$Optimization = '3',
    [bool]$FrameTrace = $true,
    [bool]$TeTrace = $true,
    [switch]$FullFrame,
    [switch]$Expert
)
$ErrorActionPreference = 'Stop'
$firmwareRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $firmwareRoot '..')).Path
function Invoke-DashboardJson([string[]]$ToolArguments, [int]$Limit) {
    # The supervisor owns Cargo and descendants before they run. Machine stdout
    # is published only on success; stderr stays live and independently bounded.
    $supervisor = Join-Path $repoRoot 'host/scripts/owned_process.py'
    $arguments = @($supervisor,'--json','--cwd',$repoRoot,'--timeout','120',
        '--stdout-limit',($Limit + 2).ToString(),'--','cargo','run','--locked','--offline',
        '-p','dashboard-config-tools','--') + $ToolArguments
    $oldEncoding = [Console]::OutputEncoding
    try {
        $utf8 = [Text.UTF8Encoding]::new($false,$true)
        [Console]::OutputEncoding = $utf8
        $text = (& python @arguments) -join "`n"
        if ($LASTEXITCODE -ne 0) { throw 'Rust catalogue/options producer failed or exceeded JSON bound' }
        $text = $text.TrimEnd("`n","`r")
        if ($utf8.GetByteCount($text) -gt $Limit) { throw 'JSON output exceeds bound' }
        return @($text, ($text | ConvertFrom-Json -Depth 32))
    } finally {
        [Console]::OutputEncoding = $oldEncoding
    }
}
function Assert-Keys($Value, [string[]]$Keys) {
    $actual = ($Value.PSObject.Properties.Name | Sort-Object) -join ','
    $expected = ($Keys | Sort-Object) -join ','
    if ($actual -cne $expected) {
        throw 'Unexpected Rust JSON schema'
    }
}
$profilesResult = Invoke-DashboardJson @('profiles') 65536
$catalogue = $profilesResult[1]
Assert-Keys $catalogue @('schema_version','profiles')
if ($catalogue.schema_version -ne 1 -or $catalogue.profiles.Count -ne 8) { throw 'Unsupported catalogue' }
$names = @($catalogue.profiles | ForEach-Object { $_.profile.aliases } | ForEach-Object { $_ })
if ($Profile -cnotin $names) { throw 'Profile must match an exact Rust catalogue alias' }
$selection = @('--mode',$Mode.ToLowerInvariant(),'--optimization',$Optimization,
    '--simple-outline',(-not $FullOutline).ToString().ToLowerInvariant(),
    '--serial-display',$SerialDisplay.IsPresent.ToString().ToLowerInvariant(),
    '--full-frame',$FullFrame.IsPresent.ToString().ToLowerInvariant(),
    '--frame-trace',$FrameTrace.ToString().ToLowerInvariant(),
    '--te-trace',$TeTrace.ToString().ToLowerInvariant())
$customPresent = @('CpuHz','SpiHz','VselMv' | Where-Object { $PSBoundParameters.ContainsKey($_) })
if ($customPresent.Count -gt 0) {
    if ($PSBoundParameters.ContainsKey('Profile')) { throw 'Explicit Profile conflicts with custom clock' }
    if ($customPresent.Count -ne 3) { throw 'Custom clock requires CpuHz, SpiHz and VselMv' }
    $selection += @('--cpu-hz',$CpuHz,'--spi-hz',$SpiHz,'--vsel-mv',$VselMv)
} else { $selection += @('--profile',$Profile) }
$compatibilityExpert = $PSBoundParameters.ContainsKey('Profile') -and $Profile -ceq '280-70'
$selection += @('--expert',($Expert.IsPresent -or $compatibilityExpert).ToString().ToLowerInvariant())
$optionsResult = Invoke-DashboardJson (@('build-options') + $selection) 4096
$options = $optionsResult[1]
Assert-Keys $options @('schema_version','requested','clock','features')
Assert-Keys $options.requested @('mode','clock','optimization','simple_outline','serial_display','full_frame','frame_trace','te_trace','expert')
if ($options.schema_version -ne 1 -or $options.features.Count -gt 16) { throw 'Unsupported normalized build options' }
foreach ($key in @('simple_outline','serial_display','full_frame','frame_trace','te_trace','expert')) {
    if ($options.requested.$key -isnot [bool]) { throw 'Invalid normalized boolean' }
}
$arguments = @((Join-Path $PSScriptRoot 'capture_target_bench.py'),$Name,'--firmware','--options-json',$optionsResult[0])
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
