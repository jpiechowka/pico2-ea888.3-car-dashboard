param([string]$Wrapper = (Join-Path $PSScriptRoot 'Build-Dashboard.ps1'))
$ErrorActionPreference = 'Stop'
$global:task2Captured = @()
$global:task2CaptureExit = 0
function python { $global:task2Captured = @($args); $global:LASTEXITCODE = $global:task2CaptureExit }
function Assert($Condition, $Message) { if (-not $Condition) { throw $Message } }
function Options {
    $at = [Array]::IndexOf($global:task2Captured, '--options-json')
    Assert ($at -ge 0) 'wrapper must pass normalized options rather than a private feature selector'
    return ($global:task2Captured[$at + 1] | ConvertFrom-Json)
}
& $wrapper -Name wrapper-default
$options = Options
Assert ($options.requested.mode -eq 'demo') 'Demo default'
Assert ($options.requested.clock.named -eq 'stock') 'Stock default'
Assert ($options.requested.frame_trace -and $options.requested.te_trace) 'trace defaults'
Assert ($options.features -contains 'cpu150-spi62-1v10') 'explicit normalized Stock'
& $wrapper -Name wrapper280 -Profile '280-70'
Assert ((Options).requested.expert) 'established explicit280 compatibility'
& $wrapper -Name wrapper-options -Mode Vehicle -Profile '150-15' -Optimization s -FrameTrace $false -TeTrace $false -FullOutline -SerialDisplay -FullFrame
$options = Options
Assert ($options.requested.mode -eq 'vehicle') 'Vehicle'
Assert (-not $options.requested.simple_outline -and -not $options.requested.frame_trace -and -not $options.requested.te_trace) 'explicit false'
Assert ($options.requested.optimization -eq 'size_s' -and $options.requested.serial_display -and $options.requested.full_frame) 'alternate options'
& $wrapper -Name wrapper-custom -CpuHz '50000000' -SpiHz '25000000' -VselMv '1100' -Expert -Optimization 2
Assert ((Options).clock.actual_cpu_hz -eq 50000000) 'full-Hz custom clock'
$spacedConfig = Join-Path ([IO.Path]::GetTempPath()) ('task2 config with spaces ' + [guid]::NewGuid().ToString('N') + '.eav')
try {
    [IO.File]::WriteAllBytes($spacedConfig,[byte[]]@(1,2,3))
    & $wrapper -Name wrapper-spaces -Config $spacedConfig
    $at = [Array]::IndexOf($global:task2Captured,'--dashboard-config')
    Assert ($at -ge 0 -and $global:task2Captured[$at + 1] -ceq (Resolve-Path -LiteralPath $spacedConfig).Path) 'Config path with spaces changed'
} finally { if (Test-Path -LiteralPath $spacedConfig) { Remove-Item -LiteralPath $spacedConfig } }
foreach ($request in @(
    @{Name='bad'; Profile='132-66'},
    @{Name='bad'; CpuHz='50000000'; SpiHz='25000000'; VselMv='1100'},
    @{Name='bad'; Profile='stock'; CpuHz='50000000'; SpiHz='25000000'; VselMv='1100'; Expert=$true},
    @{Name='bad'; CpuHz='50000000'; Expert=$true},
    @{Name='bad'; Profile='made-up'},
    @{Name='bad'; CpuHz='5e7'; SpiHz='25000000'; VselMv='1100'; Expert=$true}
)) {
    $global:task2Captured = @()
    $rejected = $false
    try { & $wrapper @request } catch { $rejected = $true }
    Assert $rejected 'invalid options must fail before capture'
    Assert ($global:task2Captured.Count -eq 0) 'invalid options dispatched capture'
}
$beforeLocation = (Get-Location).Path
$global:task2CaptureExit = 7
$failed = $false
try {
    & $wrapper -Name wrapper-capture-failure
} catch {
    $failed = $_.Exception.Message -match 'Dashboard build failed \(exit 7\)'
} finally {
    $global:task2CaptureExit = 0
}
Assert $failed 'capture failure must propagate its exact exit status'
Assert ((Get-Location).Path -ceq $beforeLocation) 'capture failure changed caller location'
'Wrapper normalization, compatibility and rejection contracts passed; no capture process or device opened.'
