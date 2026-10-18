$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'SetupTools.ps1')
# The stock Windows shell must fail before setup probes or installs tools.
$legacyShell = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
$originalPath = $env:PATH
try {
    $env:PATH = ''
    $legacyOutput = (& $legacyShell -NoProfile -NonInteractive -File (Join-Path $PSScriptRoot 'Studio.ps1') -Setup 2>&1) -join "`n"
    if ($LASTEXITCODE -eq 0 -or $legacyOutput -notlike '*PowerShell 7.2 or newer*pwsh*') {
        throw 'Legacy shell reached setup without an actionable shell prerequisite failure'
    }
} finally { $env:PATH = $originalPath }
$global:LASTEXITCODE = 0
foreach ($name in @('Studio.ps1', 'SetupTools.ps1')) {
    $tokens = $null; $errors = $null
    [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name), [ref]$tokens, [ref]$errors) | Out-Null
    if ($errors.Count) { throw ($errors | Out-String) }
}
# Exercise the actual launcher with no executable search path; no installer can run.
$originalPath = $env:PATH
$missingPython = $false
try {
    $env:PATH = ''
    try { & (Join-Path $PSScriptRoot 'Studio.ps1') -Setup }
    catch { $missingPython = $_.Exception.Message -like '*Python 3.11*https://www.python.org/*Nothing was installed*' }
} finally { $env:PATH = $originalPath }
if (-not $missingPython) { throw 'Cold Setup did not reject missing Python before installation' }
Assert-StudioSetupPython
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$pythonExecutable = (Get-Command python -CommandType Application | Select-Object -First 1).Source
$probe = Invoke-StudioSetupProcess $repo @($pythonExecutable,'-c','print("owned setup fixture")')
if ($probe -ne 'owned setup fixture') { throw 'Setup supervisor changed probe output' }
$rejected = $false
try { Invoke-StudioSetupProcess $repo @($pythonExecutable,'-c','import time; time.sleep(30)') -Timeout 1 }
catch { $rejected = $_.Exception.Message -like '*Setup process failed*' }
if (-not $rejected) { throw 'Setup process deadline was not propagated' }
Invoke-StudioSetupProcess $repo @($pythonExecutable,'-c','raise SystemExit(4)') -AllowFailure
if ($LASTEXITCODE -ne 4) { throw 'Setup changed prerequisite checker exit status' }
$global:LASTEXITCODE = 0
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
} finally {
    Remove-Item -LiteralPath $fixture
    if (Test-Path -LiteralPath ($fixture + '.lock')) { Remove-Item -LiteralPath ($fixture + '.lock') }
}
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('studio-setup-test-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
try {
    $lockPath = Join-Path $testRoot 'install.lock'
    $owner = Open-StudioSetupLock $lockPath
    try {
        $rejected = $false
        try { $other = Open-StudioSetupLock $lockPath; $other.Dispose() }
        catch { $rejected = $_.Exception.Message -like '*busy*' }
        if (-not $rejected) { throw 'Concurrent setup acquired the same installation lock' }
    } finally { $owner.Dispose() }
    $retry = Open-StudioSetupLock $lockPath
    $retry.Dispose()

    # Inject an interrupted download without network or executing any release tool.
    function Receive-StudioArchive {
        param([string]$Uri, [string]$Path)
        [IO.File]::WriteAllText($Path, 'partial fixture')
        throw 'injected interrupted download'
    }
    $cache = Join-Path $testRoot 'archive.zip'
    $rejected = $false
    try { Get-StudioReleaseArchive 'invalid://test' ('0' * 64) $cache }
    catch { $rejected = $_.Exception.Message -eq 'injected interrupted download' }
    if (-not $rejected -or (Test-Path -LiteralPath $cache)) { throw 'Interrupted download published a cache entry' }
    if (@(Get-ChildItem -LiteralPath $testRoot -Filter '*.partial').Count) { throw 'Interrupted download left staging data' }

    $source = Join-Path $testRoot 'new.exe'
    $destination = Join-Path $testRoot 'installed.exe'
    [IO.File]::WriteAllText($source, 'new complete tool')
    [IO.File]::WriteAllText($destination, 'previous tool')
    # Deny delete/replacement as an executing Windows tool would; preserve old bytes.
    $held = [IO.File]::Open($destination, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $rejected = $false
        try { Publish-StudioTool $source $destination } catch { $rejected = $true }
        if (-not $rejected -or [IO.File]::ReadAllText($destination) -ne 'previous tool') { throw 'Failed publication changed the installed tool' }
    } finally { $held.Dispose() }
    Publish-StudioTool $source $destination
    if ([IO.File]::ReadAllText($destination) -ne 'new complete tool') { throw 'Explicit publication retry failed' }
    if (@(Get-ChildItem -LiteralPath $testRoot -Filter '*.partial').Count) { throw 'Publication left staging data' }
} finally {
    $resolvedTest = [IO.Path]::GetFullPath($testRoot)
    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar)
    if ([IO.Path]::GetDirectoryName($resolvedTest) -cne $temporaryRoot -or [IO.Path]::GetFileName($resolvedTest) -notlike 'studio-setup-test-*') { throw 'Unexpected test cleanup path' }
    Remove-Item -LiteralPath $resolvedTest -Recurse
}
Write-Output 'Setup parser and cached-release hash/size guards passed.'
