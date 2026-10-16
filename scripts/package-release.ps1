#Requires -Version 5.1
<#
.SYNOPSIS
 Produce TMsgPicker-Setup-x64.exe, TMsgPicker-Portable-x64.zip and SHA256SUMS.txt.
.DESCRIPTION
 Run after stage-native.ps1 and "pnpm tauri build --bundles nsis --config ...".
#>
[CmdletBinding()]
param([string]$NativeRoot = '')

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($NativeRoot)) { $NativeRoot = Join-Path $repo 'native' }
$NativeRoot = [System.IO.Path]::GetFullPath($NativeRoot)
$stage = Join-Path $NativeRoot 'release'
$dist = Join-Path $NativeRoot 'dist'
$portableParent = Join-Path $dist 'portable-work'
$portable = Join-Path $portableParent 'TMsgPicker'
$exe = Join-Path $repo 'src-tauri\target\release\tmsgpicker.exe'
$config = Get-Content -LiteralPath (Join-Path $repo 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$manifest = Get-Content -LiteralPath (Join-Path $stage 'licenses\native-dependencies.json') -Raw | ConvertFrom-Json
$pin = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-versions.json') -Raw | ConvertFrom-Json
if ($env:OS -ne 'Windows_NT') { throw 'Windows only.' }
if (-not (Test-Path -LiteralPath $exe)) { throw "Release executable missing: $exe" }
if ($manifest.tdlib_commit -ne $pin.tdlib.commit) { throw 'Native manifest is not pinned to this release.' }
$runtimeNames = @($manifest.files.PSObject.Properties.Name | Where-Object { $_ -like '*.dll' })
foreach ($name in $runtimeNames) {
    if (-not (Test-Path -LiteralPath (Join-Path $stage $name))) { throw "Native runtime not staged: $name" }
}

$nsisFolder = Join-Path $repo 'src-tauri\target\release\bundle\nsis'
$candidates = @(Get-ChildItem -LiteralPath $nsisFolder -File -Filter '*.exe' |
    Where-Object { $_.Name -like "*$($config.version)*x64-setup.exe" })
if ($candidates.Count -ne 1) { throw "Expected exactly one v$($config.version) x64 NSIS installer, found $($candidates.Count)." }
$sevenZip = Get-Command '7z.exe' -ErrorAction SilentlyContinue
$sevenZipPath = if ($null -ne $sevenZip) { $sevenZip.Source } else { Join-Path $env:ProgramFiles '7-Zip\7z.exe' }
if (-not (Test-Path -LiteralPath $sevenZipPath)) { throw '7-Zip is needed for NSIS archive inspection.' }

# Inspect the installer itself rather than trusting tauri.conf / cargo staging.
$listing = @( & $sevenZipPath l -slt $candidates[0].FullName )
if ($LASTEXITCODE -ne 0) { throw 'Could not inspect built NSIS installer.' }
$installerPaths = @($listing | ForEach-Object {
    if ($_ -match '^Path = (.+)$') { $Matches[1].Trim() }
})
foreach ($name in ($runtimeNames + @('licenses\TDLib.txt', 'licenses\OpenSSL.txt', 'licenses\zlib.txt'))) {
    $regex = '(?i)(^|[\\/])' + [regex]::Escape($name.Replace('\', '/')).Replace('/', '[\\/]') + '$'
    if (-not @($installerPaths | Where-Object { $_ -match $regex }).Count) {
        throw "NSIS installer does not contain required release file: $name"
    }
}

$null = New-Item -ItemType Directory -Force -Path $dist
if (Test-Path -LiteralPath $portableParent) { Remove-Item -LiteralPath $portableParent -Recurse -Force }
$null = New-Item -ItemType Directory -Force -Path $portable
Copy-Item -LiteralPath $exe -Destination (Join-Path $portable 'tmsgpicker.exe')
foreach ($name in $runtimeNames) {
    Copy-Item -LiteralPath (Join-Path $stage $name) -Destination (Join-Path $portable $name)
}
Copy-Item -LiteralPath (Join-Path $stage 'licenses') -Destination (Join-Path $portable 'licenses') -Recurse

# Tauri's NSIS bundleVCRuntime adds the MSVC CRT DLLs for the installer.
# A portable directory needs its own copies of that same redistributable CRT.
$vswhere = Join-Path ([System.Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'MSVC vswhere.exe missing; portable CRT cannot be verified.' }
$vs = (& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Visual Studio C++ tools missing.' }
$redistBase = Join-Path $vs 'VC\Redist\MSVC'
$crtDirs = @(Get-ChildItem -LiteralPath $redistBase -Directory | Sort-Object Name -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64\Microsoft.VC143.CRT' } |
    Where-Object { Test-Path -LiteralPath $_ })
if ($crtDirs.Count -eq 0) { throw 'MSVC x64 redistributable CRT directory missing.' }
$crt = @(Get-ChildItem -LiteralPath $crtDirs[0] -File -Filter '*.dll')
if ($crt.Count -eq 0) { throw 'No MSVC CRT DLLs found.' }
foreach ($dll in $crt) { Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $portable $dll.Name) -Force }

# Explicitly check that no PE import is satisfied only by CI-installed dev tools.
& (Join-Path $PSScriptRoot 'audit-pe-imports.ps1') -Directory $portable
if (-not $?) { throw 'PE dependency audit failed.' }

$setupOutput = Join-Path $dist 'TMsgPicker-Setup-x64.exe'
$zipOutput = Join-Path $dist 'TMsgPicker-Portable-x64.zip'
Copy-Item -LiteralPath $candidates[0].FullName -Destination $setupOutput -Force
if (Test-Path -LiteralPath $zipOutput) { Remove-Item -LiteralPath $zipOutput -Force }
Compress-Archive -LiteralPath $portable -DestinationPath $zipOutput -CompressionLevel Optimal

$hashLines = @()
foreach ($file in @($setupOutput, $zipOutput)) {
    $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    $hashLines += "$hash  $([System.IO.Path]::GetFileName($file))"
}
$checksums = Join-Path $dist 'SHA256SUMS.txt'
[System.IO.File]::WriteAllText($checksums, ($hashLines -join [Environment]::NewLine) + [Environment]::NewLine,
    [System.Text.UTF8Encoding]::new($false))
Remove-Item -LiteralPath $portableParent -Recurse -Force
Write-Host "Packaged: $setupOutput"
Write-Host "Packaged: $zipOutput"
Write-Host "Checksums: $checksums"
