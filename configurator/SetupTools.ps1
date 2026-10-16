# Explicit Setup support. Dot-sourcing declares functions and performs no I/O.
function Get-StudioReleaseArchive {
    param([string]$Uri, [string]$Sha256, [string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Invoke-WebRequest -Uri $Uri -OutFile $Path
    }
    if ((Get-Item -LiteralPath $Path).Length -gt 134217728 -or
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Sha256) {
        throw "Release archive hash/size mismatch: $Path. Remove this cached file and rerun -Setup. No executable was installed."
    }
}

function Install-StudioReleaseTools {
    param([Parameter(Mandatory)][string]$Repository)
    $toolRoot = Join-Path $Repository 'target/configurator-tools'
    $bin = Join-Path $toolRoot 'bin'
    $stage = Join-Path $toolRoot 'release-staging'
    $picoBin = $bin
    foreach ($directory in @($toolRoot, $bin, $stage, $picoBin)) {
        [IO.Directory]::CreateDirectory($directory) | Out-Null
    }
    $binaryen = Join-Path $toolRoot 'binaryen-version_133-x86_64-windows.tar.gz'
    Get-StudioReleaseArchive -Uri 'https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-windows.tar.gz' -Sha256 '17a2cbeac6b5693c5fbafab3838d3c65fd9c1eb38b05f5baec6c657e8c84995b' -Path $binaryen
    # Only the named member is extracted; no archive paths are used as destinations.
    & tar -xzf $binaryen -C $stage 'binaryen-version_133/bin/wasm-opt.exe'
    if ($LASTEXITCODE -ne 0) { throw 'Could not extract official Binaryen; Windows tar.exe is required.' }
    $optimizer = Join-Path $stage 'binaryen-version_133/bin/wasm-opt.exe'
    $version = & $optimizer --version
    if ($LASTEXITCODE -ne 0 -or ($version -join "`n").Trim() -ne 'wasm-opt version 133 (version_133)') { throw 'Extracted Binaryen version mismatch' }
    Copy-Item -LiteralPath $optimizer -Destination (Join-Path $bin 'wasm-opt.exe') -Force

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
    $version = & $stagedPico version -s
    if ($LASTEXITCODE -ne 0 -or ($version -join "`n").Trim() -ne '2.3.1') { throw 'Extracted picotool version mismatch' }
    $help = & $stagedPico help load 2>&1
    if ($LASTEXITCODE -ne 0 -or ($help -join "`n") -notmatch '--ignore-partitions') { throw 'Official picotool load contract is unavailable' }
    Copy-Item -LiteralPath $stagedPico -Destination (Join-Path $picoBin 'picotool.exe') -Force
    Write-Host 'Installed SHA256-verified official Binaryen 133 and picotool 2.3.1. No USB operation was performed.'
}
