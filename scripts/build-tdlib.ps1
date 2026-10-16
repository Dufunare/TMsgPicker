#Requires -Version 5.1
<#
.SYNOPSIS
 Build the pinned TDLib JSON DLL and its vcpkg dependencies for Windows x64.
.DESCRIPTION
 All checked-out sources and build outputs live below the ignored native/ folder.
 This does not install or run the TMsgPicker application and never touches user Sessions.
#>
[CmdletBinding()]
param(
    [ValidateRange(1, 16)][int]$Jobs = 2,
    [string]$NativeRoot = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($NativeRoot)) {
    $NativeRoot = Join-Path $repo 'native'
}
$NativeRoot = [System.IO.Path]::GetFullPath($NativeRoot)
$pin = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-versions.json') -Raw | ConvertFrom-Json
$sourceRoot = Join-Path $NativeRoot 'sources'
$tdRoot = Join-Path $sourceRoot 'td'
$vcpkgRoot = Join-Path $sourceRoot 'vcpkg'
$buildRoot = Join-Path $NativeRoot 'build\tdlib'
$output = Join-Path $NativeRoot 'tdlib'

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Program exited with code $LASTEXITCODE; arguments: $($Arguments -join ' ')"
    }
}

function Checkout-Pin {
    param([string]$Url, [string]$Directory, [string]$Commit)
    if (-not (Test-Path -LiteralPath $Directory)) {
        Invoke-Checked 'git' @('clone', '--filter=blob:none', $Url, $Directory)
    }
    if (-not (Test-Path -LiteralPath (Join-Path $Directory '.git'))) {
        throw "Existing directory is not a Git checkout: $Directory"
    }
    $dirty = @( & git -C $Directory status --porcelain )
    if ($LASTEXITCODE -ne 0 -or $dirty.Count -gt 0) {
        throw "Refusing to replace modified native source checkout: $Directory"
    }
    Invoke-Checked 'git' @('-C', $Directory, 'fetch', '--depth=1', 'origin', $Commit)
    Invoke-Checked 'git' @('-C', $Directory, 'checkout', '--detach', $Commit)
    $actual = (& git -C $Directory rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $actual -ne $Commit) {
        throw "Pinned commit verification failed for $Directory"
    }
}

function Check-Port {
    param([string]$Port, [string]$Expected)
    $info = Get-Content -LiteralPath (Join-Path $vcpkgRoot "ports\$Port\vcpkg.json") -Raw | ConvertFrom-Json
    $release = if ($null -ne $info.'port-version') { [int]$info.'port-version' } else { 0 }
    $version = if ($null -ne $info.version) { $info.version } else { $info.'version-string' }
    $found = "$version#$release"
    if ($found -ne $Expected) {
        throw "Pinned vcpkg port changed: $Port expected $Expected, found $found"
    }
}

if ($env:OS -ne 'Windows_NT') { throw 'This script requires Windows x64.' }
if ([Environment]::Is64BitOperatingSystem -ne $true) { throw 'x64 Windows is required.' }
foreach ($tool in @('git.exe', 'cmake.exe')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Missing build tool: $tool" }
}
$null = New-Item -ItemType Directory -Force -Path $sourceRoot, $output, $buildRoot

Write-Host "Pinned TDLib $($pin.tdlib.version) at $($pin.tdlib.commit)"
Checkout-Pin $pin.tdlib.repository $tdRoot $pin.tdlib.commit
Checkout-Pin $pin.vcpkg.repository $vcpkgRoot $pin.vcpkg.commit

$cmakeInfo = Get-Content -LiteralPath (Join-Path $tdRoot 'CMakeLists.txt') -Raw
if ($cmakeInfo -notmatch ('project\(TDLib VERSION ' + [regex]::Escape($pin.tdlib.version) + '\b')) {
    throw 'Pinned TDLib commit does not declare the expected CMake version.'
}
Check-Port 'openssl' $pin.native_dependencies.openssl
Check-Port 'zlib' $pin.native_dependencies.zlib
Check-Port 'gperf' $pin.native_dependencies.gperf

$bootstrap = Join-Path $vcpkgRoot 'bootstrap-vcpkg.bat'
Invoke-Checked $bootstrap @('-disableMetrics')
$vcpkgExe = Join-Path $vcpkgRoot 'vcpkg.exe'
Push-Location $vcpkgRoot
try {
    Invoke-Checked $vcpkgExe @('install', 'openssl:x64-windows', 'zlib:x64-windows', 'gperf:x64-windows')
} finally {
    Pop-Location
}

$toolDir = Join-Path $vcpkgRoot 'installed\x64-windows\tools\gperf'
$gperf = Get-ChildItem -LiteralPath $toolDir -File -Filter 'gperf.exe' -ErrorAction Stop | Select-Object -First 1
if ($null -eq $gperf) { throw "gperf.exe not found in $toolDir" }
$env:PATH = "$($gperf.DirectoryName);$env:PATH"

$toolchain = Join-Path $vcpkgRoot 'scripts\buildsystems\vcpkg.cmake'
$cmakeArgs = @(
    '-S', $tdRoot, '-B', $buildRoot, '-G', 'Visual Studio 17 2022', '-A', 'x64',
    "-DCMAKE_TOOLCHAIN_FILE=$toolchain", '-DVCPKG_TARGET_TRIPLET=x64-windows',
    '-DTD_ENABLE_JNI=OFF', '-DTD_ENABLE_DOTNET=OFF',
    '-DTD_INSTALL_STATIC_LIBRARIES=OFF', '-DTD_ENABLE_LTO=OFF', '-DBUILD_TESTING=OFF'
)
Invoke-Checked 'cmake' $cmakeArgs
Invoke-Checked 'cmake' @('--build', $buildRoot, '--config', 'Release', '--target', 'tdjson', '--parallel', "$Jobs")

foreach ($file in @('tdjson.dll', 'tdjson.lib')) {
    $candidates = @(Get-ChildItem -LiteralPath $buildRoot -Recurse -File -Filter $file |
        Sort-Object @{ Expression = { if ($_.FullName -match '[\\/]Release[\\/]') { 0 } else { 1 } } }, FullName)
    if ($candidates.Count -eq 0) { throw "$file not produced by TDLib build" }
    Copy-Item -LiteralPath $candidates[0].FullName -Destination (Join-Path $output $file) -Force
}

$dynamicDir = Join-Path $vcpkgRoot 'installed\x64-windows\bin'
$dlls = @('libssl-3-x64.dll', 'libcrypto-3-x64.dll')
$zlib = @('z.dll', 'zlib1.dll') | Where-Object { Test-Path -LiteralPath (Join-Path $dynamicDir $_) } | Select-Object -First 1
if (-not $zlib) { throw "Neither z.dll nor zlib1.dll found at $dynamicDir" }
$dlls += $zlib
foreach ($file in $dlls) {
    $path = Join-Path $dynamicDir $file
    if (-not (Test-Path -LiteralPath $path)) { throw "Missing pinned vcpkg DLL: $path" }
    Copy-Item -LiteralPath $path -Destination (Join-Path $output $file) -Force
}

$hashes = [ordered]@{}
foreach ($file in (@('tdjson.lib', 'tdjson.dll') + $dlls)) {
    $hashes[$file] = (Get-FileHash -LiteralPath (Join-Path $output $file) -Algorithm SHA256).Hash.ToLowerInvariant()
}
$manifest = [ordered]@{
    tdlib_version = $pin.tdlib.version
    tdlib_commit = $pin.tdlib.commit
    vcpkg_commit = $pin.vcpkg.commit
    openssl = $pin.native_dependencies.openssl
    zlib = $pin.native_dependencies.zlib
    gperf = $pin.native_dependencies.gperf
    triplet = $pin.native_dependencies.triplet
    files = $hashes
}
[System.IO.File]::WriteAllText((Join-Path $output 'build-manifest.json'), ($manifest | ConvertTo-Json -Depth 8) + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
Write-Host "Pinned TDLib build produced: $output"
Write-Host 'Next: ./scripts/stage-native.ps1'
