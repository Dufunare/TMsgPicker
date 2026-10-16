#Requires -Version 5.1
<#
.SYNOPSIS
 Verify a pinned TDLib build, stage redistributable DLLs and licenses, and
 generate a Tauri NSIS release config (without changing development settings).
#>
[CmdletBinding()]
param([string]$NativeRoot = '')

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($NativeRoot)) { $NativeRoot = Join-Path $repo 'native' }
$NativeRoot = [System.IO.Path]::GetFullPath($NativeRoot)
$pin = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-versions.json') -Raw | ConvertFrom-Json
$td = Join-Path $NativeRoot 'tdlib'
$out = Join-Path $NativeRoot 'release'
$licenseDir = Join-Path $out 'licenses'
$vcpkgRoot = Join-Path $NativeRoot 'sources\vcpkg'
$sourceTd = Join-Path $NativeRoot 'sources\td'

if ($env:OS -ne 'Windows_NT') { throw 'Windows x64 required for native staging.' }
$manifestFile = Join-Path $td 'build-manifest.json'
if (-not (Test-Path -LiteralPath $manifestFile)) { throw 'Missing build-manifest.json; run build-tdlib.ps1 first.' }
$manifest = Get-Content -LiteralPath $manifestFile -Raw | ConvertFrom-Json
if ($manifest.tdlib_version -ne $pin.tdlib.version -or
    $manifest.tdlib_commit -ne $pin.tdlib.commit -or
    $manifest.vcpkg_commit -ne $pin.vcpkg.commit -or
    $manifest.openssl -ne $pin.native_dependencies.openssl -or
    $manifest.zlib -ne $pin.native_dependencies.zlib -or
    $manifest.gperf -ne $pin.native_dependencies.gperf -or
    $manifest.triplet -ne 'x64-windows') {
    throw 'Native build manifest does not match scripts/native-versions.json. Rebuild with pinned sources.'
}

$names = @($manifest.files.PSObject.Properties.Name)
foreach ($required in @('tdjson.lib', 'tdjson.dll', 'libssl-3-x64.dll', 'libcrypto-3-x64.dll')) {
    if ($names -cnotcontains $required) { throw "Native manifest missing $required" }
}
$z = @('z.dll', 'zlib1.dll') | Where-Object { $names -ccontains $_ }
if (@($z).Count -ne 1) { throw 'Native build must contain exactly one zlib DLL name.' }
$expectedNames = @('tdjson.lib', 'tdjson.dll', 'libssl-3-x64.dll', 'libcrypto-3-x64.dll') + @($z)
if ($names.Count -ne $expectedNames.Count) { throw 'Native manifest contains unexpected files.' }

$null = New-Item -ItemType Directory -Force -Path $out, $licenseDir
$runtime = @($expectedNames | Where-Object { $_ -ne 'tdjson.lib' })
foreach ($file in $expectedNames) {
    $source = Join-Path $td $file
    if (-not (Test-Path -LiteralPath $source)) { throw "Native input missing $source" }
    $actual = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $manifest.files.PSObject.Properties[$file].Value) {
        throw "Hash mismatch for pinned $file"
    }
    if ($file -ne 'tdjson.lib') { Copy-Item -LiteralPath $source -Destination (Join-Path $out $file) -Force }
}

# Copy the actual license notices from the exact checked-out/installed packages.
$licenseSources = [ordered]@{
    'TDLib.txt' = (Join-Path $sourceTd 'LICENSE_1_0.txt')
    'OpenSSL.txt' = (Join-Path $vcpkgRoot 'installed\x64-windows\share\openssl\copyright')
    'zlib.txt' = (Join-Path $vcpkgRoot 'installed\x64-windows\share\zlib\copyright')
}
foreach ($name in $licenseSources.Keys) {
    $src = $licenseSources[$name]
    if (-not (Test-Path -LiteralPath $src)) { throw "Third-party copyright notice missing: $src" }
    Copy-Item -LiteralPath $src -Destination (Join-Path $licenseDir $name) -Force
}
# The application's own MIT copyright notice is a mandatory part of both
# NSIS and portable distribution, separate from third-party licenses.
$appLicense = Join-Path $repo 'LICENSE'
if (-not (Test-Path -LiteralPath $appLicense)) { throw 'Missing application MIT LICENSE' }
$appLicenseTarget = 'TMsgPicker-MIT.txt'
Copy-Item -LiteralPath $appLicense -Destination (Join-Path $licenseDir $appLicenseTarget) -Force
Copy-Item -LiteralPath $manifestFile -Destination (Join-Path $licenseDir 'native-dependencies.json') -Force
$note = @"
TMsgPicker Windows native dependency notices
TDLib $($pin.tdlib.version): $($pin.tdlib.commit)
OpenSSL: $($pin.native_dependencies.openssl)
zlib: $($pin.native_dependencies.zlib)
vcpkg baseline: $($pin.vcpkg.commit)
TMsgPicker application source and binaries are licensed under the MIT License.
See TMsgPicker-MIT.txt for copyright, grant, conditions and disclaimer.
The third-party dependency notices belong to their respective copyright holders.
"@
[System.IO.File]::WriteAllText((Join-Path $licenseDir 'NOTICE.txt'), $note, [System.Text.UTF8Encoding]::new($false))

# Tauri v2 on Windows puts resources in $INSTDIR; a relative target filename
# is adjacent to the app exe. Never put native-load-time DLLs under "resources/".
$resources = [ordered]@{}
foreach ($dll in $runtime) {
    $resources["../native/release/$dll"] = $dll
}
foreach ($name in (@($licenseSources.Keys) + @($appLicenseTarget, 'native-dependencies.json', 'NOTICE.txt'))) {
    $resources["../native/release/licenses/$name"] = "licenses/$name"
}
$overlay = [ordered]@{
    bundle = [ordered]@{
        targets = @('nsis')
        resources = $resources
        windows = [ordered]@{
            bundleVCRuntime = $true
            webviewInstallMode = @{ type = 'downloadBootstrapper'; silent = $true }
            nsis = @{ installMode = 'currentUser'; languages = @('English', 'SimpChinese') }
        }
    }
}
$config = Join-Path $repo 'src-tauri\tauri.release.generated.json'
$json = $overlay | ConvertTo-Json -Depth 12
[System.IO.File]::WriteAllText($config, $json + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
Write-Host "Staged runtime: $out"
Write-Host "Generated release-only Tauri config: $config"
Write-Host "Runtime files: $($runtime -join ', ')"
Write-Host 'Use pnpm.cmd tauri build --bundles nsis --config src-tauri/tauri.release.generated.json'
