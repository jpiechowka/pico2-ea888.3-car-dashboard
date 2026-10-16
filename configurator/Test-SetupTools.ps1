$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'SetupTools.ps1')
foreach ($name in @('Studio.ps1', 'SetupTools.ps1')) {
    $tokens = $null; $errors = $null
    [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name), [ref]$tokens, [ref]$errors) | Out-Null
    if ($errors.Count) { throw ($errors | Out-String) }
}
# Exercise the actual launcher with no executable search path; no installer can run.
$originalPath = $env:PATH
$missingRustup = $false
try {
    $env:PATH = ''
    try { & (Join-Path $PSScriptRoot 'Studio.ps1') -Setup }
    catch { $missingRustup = $_.Exception.Message -like '*https://rustup.rs*rerun*Setup*' }
} finally { $env:PATH = $originalPath }
if (-not $missingRustup) { throw 'Cold Setup did not give actionable missing-rustup guidance' }
$fixture = [IO.Path]::GetTempFileName()
try {
    [IO.File]::WriteAllText($fixture, 'verified fixture')
    $hash = (Get-FileHash -LiteralPath $fixture).Hash
    # A valid cached archive must not attempt network access.
    Get-StudioReleaseArchive -Uri 'invalid://must-not-download' -Sha256 $hash -Path $fixture
    $rejected = $false
    try { Get-StudioReleaseArchive -Uri 'invalid://must-not-download' -Sha256 ('0' * 64) -Path $fixture }
    catch { $rejected = $_.Exception.Message -like '*hash/size mismatch*' }
    if (-not $rejected) { throw 'Mismatched archive was accepted' }
    $stream = [IO.File]::OpenWrite($fixture)
    try { $stream.SetLength(134217729) } finally { $stream.Dispose() }
    $rejected = $false
    try { Get-StudioReleaseArchive -Uri 'invalid://must-not-download' -Sha256 $hash -Path $fixture }
    catch { $rejected = $_.Exception.Message -like '*hash/size mismatch*' }
    if (-not $rejected) { throw 'Oversized archive was accepted' }
} finally { Remove-Item -LiteralPath $fixture }
Write-Output 'Setup parser and cached-release hash/size guards passed.'
