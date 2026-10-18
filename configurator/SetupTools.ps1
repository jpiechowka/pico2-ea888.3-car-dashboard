# Explicit Setup support. Dot-sourcing declares functions and performs no I/O.
function Assert-StudioSetupPython {
    $python = Get-Command python -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -ne $python) {
        & $python.Source -c 'import sys; raise SystemExit(0 if sys.version_info >= (3,11) else 1)'
        if ($LASTEXITCODE -eq 0) { return }
    }
    throw 'Python 3.11 or newer is required before Setup. Install Python for Windows from https://www.python.org/downloads/windows/, enable Add python.exe to PATH, reopen PowerShell, then rerun configurator/Studio.ps1 -Setup. Nothing was installed.'
}

function Invoke-StudioSetupProcess {
    param([string]$Repository, [string[]]$Command, [int]$Timeout = 120, [switch]$Stream, [switch]$AllowFailure)
    $arguments = @((Join-Path $Repository 'host/scripts/owned_process.py'), '--cwd', $Repository,
        '--timeout', $Timeout.ToString(), '--stdout-limit', '8388608')
    if (-not $Stream) { $arguments += '--json' }
    $arguments += @('--') + $Command
    & python @arguments
    if ($LASTEXITCODE -ne 0 -and -not $AllowFailure) { throw "Setup process failed (exit $LASTEXITCODE): $([IO.Path]::GetFileName($Command[0]))" }
}

function Open-StudioSetupLock {
    param([string]$Path)
    try {
        # Keep the lock file: deleting it after close creates a pathname race.
        return [IO.File]::Open($Path, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    } catch {
        throw "Studio setup is busy or its lock is inaccessible: $Path"
    }
}

function Receive-StudioArchive {
    param([string]$Uri, [string]$Path)
    $client = [Net.Http.HttpClient]::new()
    $cancellation = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(120))
    $response = $null; $inputStream = $null; $outputStream = $null
    try {
        $response = $client.GetAsync($Uri, [Net.Http.HttpCompletionOption]::ResponseHeadersRead, $cancellation.Token).GetAwaiter().GetResult()
        $response.EnsureSuccessStatusCode() | Out-Null
        if ($response.Content.Headers.ContentLength -gt 134217728) { throw 'Release archive exceeds 128 MiB bound' }
        $inputStream = $response.Content.ReadAsStreamAsync($cancellation.Token).GetAwaiter().GetResult()
        $outputStream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $buffer = [byte[]]::new(65536)
        $total = 0L
        while (($count = $inputStream.ReadAsync($buffer, 0, $buffer.Length, $cancellation.Token).GetAwaiter().GetResult()) -gt 0) {
            $total += $count
            if ($total -gt 134217728) { throw 'Release archive exceeds 128 MiB bound' }
            $outputStream.Write($buffer, 0, $count)
        }
        $outputStream.Flush($true)
    } finally {
        if ($null -ne $outputStream) { $outputStream.Dispose() }
        if ($null -ne $inputStream) { $inputStream.Dispose() }
        if ($null -ne $response) { $response.Dispose() }
        $cancellation.Dispose(); $client.Dispose()
    }
}

function Assert-StudioArchive {
    param([string]$Path, [string]$Sha256)
    if ((Get-Item -LiteralPath $Path).Length -gt 134217728 -or
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Sha256) {
        throw "Release archive hash/size mismatch: $Path. No executable was installed."
    }
}

function Get-StudioReleaseArchive {
    param([string]$Uri, [string]$Sha256, [string]$Path)
    $lock = Open-StudioSetupLock ($Path + '.lock')
    $temporary = $Path + '.' + [guid]::NewGuid().ToString('N') + '.partial'
    try {
        if (Test-Path -LiteralPath $Path -PathType Leaf) {
            Assert-StudioArchive $Path $Sha256
            return
        }
        Receive-StudioArchive $Uri $temporary
        Assert-StudioArchive $temporary $Sha256
        [IO.File]::Move($temporary, $Path)
    } finally {
        try { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary } }
        finally { $lock.Dispose() }
    }
}

function Publish-StudioTool {
    param([string]$Source, [string]$Destination)
    $temporary = $Destination + '.' + [guid]::NewGuid().ToString('N') + '.partial'
    try {
        [IO.File]::Copy($Source, $temporary, $false)
        [IO.File]::Move($temporary, $Destination, $true)
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
    }
}

function Install-StudioReleaseTools {
    param([Parameter(Mandatory)][string]$Repository)
    $toolRoot = Join-Path $Repository 'target/configurator-tools'
    $bin = Join-Path $toolRoot 'bin'
    $stage = Join-Path $toolRoot ('release-staging-' + [guid]::NewGuid().ToString('N'))
    $picoBin = $bin
    foreach ($directory in @($toolRoot, $bin)) {
        [IO.Directory]::CreateDirectory($directory) | Out-Null
    }
    $installLock = Open-StudioSetupLock (Join-Path $toolRoot 'install.lock')
    try {
    [IO.Directory]::CreateDirectory($stage) | Out-Null
    $binaryen = Join-Path $toolRoot 'binaryen-version_133-x86_64-windows.tar.gz'
    Get-StudioReleaseArchive -Uri 'https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-windows.tar.gz' -Sha256 '17a2cbeac6b5693c5fbafab3838d3c65fd9c1eb38b05f5baec6c657e8c84995b' -Path $binaryen
    # Only the named member is extracted; no archive paths are used as destinations.
    Invoke-StudioSetupProcess $Repository @('tar','-xzf',$binaryen,'-C',$stage,'binaryen-version_133/bin/wasm-opt.exe')
    $optimizer = Join-Path $stage 'binaryen-version_133/bin/wasm-opt.exe'
    $version = Invoke-StudioSetupProcess $Repository @($optimizer,'--version')
    if (($version -join "`n").Trim() -ne 'wasm-opt version 133 (version_133)') { throw 'Extracted Binaryen version mismatch' }

    $picotool = Join-Path $toolRoot 'picotool-2.3.1-x64-win.zip'
    Get-StudioReleaseArchive -Uri 'https://github.com/raspberrypi/pico-sdk-tools/releases/download/v2.3.1-0/picotool-2.3.1-x64-win.zip' -Sha256 '68730be0813f8f35be2cca147cf7f1572662d5dcf0f5ba468e02a6dd9e85db2b' -Path $picotool
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($picotool)
    try {
        $entry = $archive.GetEntry('picotool/picotool.exe')
        if ($null -eq $entry -or $entry.Length -gt 16777216) { throw 'Official picotool executable missing or oversized' }
        $stagedPico = Join-Path $stage 'picotool.exe'
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $stagedPico, $true)
    } finally { $archive.Dispose() }
    $version = Invoke-StudioSetupProcess $Repository @($stagedPico,'version','-s')
    if (($version -join "`n").Trim() -ne '2.3.1') { throw 'Extracted picotool version mismatch' }
    $help = Invoke-StudioSetupProcess $Repository @($stagedPico,'help','load')
    if (($help -join "`n") -notmatch '--ignore-partitions') { throw 'Official picotool load contract is unavailable' }
    # Validate both tools before publication. Each file replacement is atomic;
    # the two executable versions are not a filesystem transaction.
    Publish-StudioTool $optimizer (Join-Path $bin 'wasm-opt.exe')
    Publish-StudioTool $stagedPico (Join-Path $picoBin 'picotool.exe')
    Write-Host 'Installed SHA256-verified official Binaryen 133 and picotool 2.3.1. No USB operation was performed.'
    } finally {
        # This exact GUID directory was created here under the checked tool root.
        try {
            $stagePath = [IO.Path]::GetFullPath($stage)
            $parentPath = [IO.Path]::GetFullPath($toolRoot).TrimEnd([IO.Path]::DirectorySeparatorChar)
            if ([IO.Path]::GetDirectoryName($stagePath) -cne $parentPath) { throw 'Unexpected setup staging path' }
            if (Test-Path -LiteralPath $stagePath) { Remove-Item -LiteralPath $stagePath -Recurse }
        }
        finally { $installLock.Dispose() }
    }
}
