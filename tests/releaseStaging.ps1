#Requires -Version 5.1
<#
.SYNOPSIS
 Offline contract test: exercises release staging using fake native files.
 It NEVER launches Telegram/TDLib; must not overwrite an existing native tree.
#>
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$native=Join-Path $root 'native'
$generated=Join-Path $root 'src-tauri\tauri.release.generated.json'
if(Test-Path -LiteralPath $native) { throw 'Refusing to overwrite an existing native directory.' }
if(Test-Path -LiteralPath $generated) { throw 'Refusing to overwrite an existing Tauri release configuration.' }
$pin=Get-Content -LiteralPath (Join-Path $root 'scripts\native-versions.json') -Raw | ConvertFrom-Json

try {
    $tdlib=Join-Path $native 'tdlib'
    $upstream=Join-Path $native 'sources\td'
    $openssl=Join-Path $native 'sources\vcpkg\installed\x64-windows\share\openssl'
    $zlib=Join-Path $native 'sources\vcpkg\installed\x64-windows\share\zlib'
    foreach($dir in @($tdlib,$upstream,$openssl,$zlib)) {
        $null=New-Item -ItemType Directory -Force -Path $dir
    }
    $hashes=[ordered]@{}
    foreach($name in @('tdjson.lib','tdjson.dll','libssl-3-x64.dll','libcrypto-3-x64.dll','zlib1.dll')) {
        $file=Join-Path $tdlib $name
        [System.IO.File]::WriteAllText($file,"mock_$name",[System.Text.UTF8Encoding]::new($false))
        $hashes[$name]=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    [System.IO.File]::WriteAllText((Join-Path $upstream 'LICENSE_1_0.txt'),'MOCK TDLib LICENSE')
    [System.IO.File]::WriteAllText((Join-Path $openssl 'copyright'),'MOCK OpenSSL COPYRIGHT')
    [System.IO.File]::WriteAllText((Join-Path $zlib 'copyright'),'MOCK zlib COPYRIGHT')
    $manifest=[ordered]@{
        tdlib_version=$pin.tdlib.version
        tdlib_commit=$pin.tdlib.commit
        vcpkg_commit=$pin.vcpkg.commit
        openssl=$pin.native_dependencies.openssl
        zlib=$pin.native_dependencies.zlib
        gperf=$pin.native_dependencies.gperf
        triplet=$pin.native_dependencies.triplet
        files=$hashes
    }
    [System.IO.File]::WriteAllText((Join-Path $tdlib 'build-manifest.json'),
        ($manifest | ConvertTo-Json -Depth 8),[System.Text.UTF8Encoding]::new($false))

    & (Join-Path $root 'scripts\stage-native.ps1')
    $overlay=Get-Content -LiteralPath $generated -Raw | ConvertFrom-Json
    $resourceNames=@($overlay.bundle.resources.PSObject.Properties.Name)
    foreach($asset in @('tdjson.dll','libssl-3-x64.dll','libcrypto-3-x64.dll','zlib1.dll','licenses/TDLib.txt','licenses/TMsgPicker-MIT.txt','licenses/native-dependencies.json')) {
        $dest=$overlay.bundle.resources.PSObject.Properties.Value | Where-Object { $_ -eq $asset }
        if(-not $dest) { throw "Release resource mapping missing: $asset" }
    }
    if($overlay.bundle.windows.bundleVCRuntime -ne $true -or
       $overlay.bundle.targets -cnotcontains 'nsis') {
        throw 'Release configuration did not enable NSIS and VC Runtime.'
    }
    if(-not (Test-Path -LiteralPath (Join-Path $native 'release\licenses\NOTICE.txt'))) {
        throw 'License notice missing from staged release.'
    }
    $stagedAppLicense=Join-Path $native 'release\licenses\TMsgPicker-MIT.txt'
    if(-not (Test-Path -LiteralPath $stagedAppLicense)) {
        throw 'Application MIT license missing from staged release.'
    }
    $originalLicenseHash=(Get-FileHash -LiteralPath (Join-Path $root 'LICENSE') -Algorithm SHA256).Hash
    $stagedLicenseHash=(Get-FileHash -LiteralPath $stagedAppLicense -Algorithm SHA256).Hash
    if($originalLicenseHash -ne $stagedLicenseHash){
        throw 'Application MIT license differs from repository LICENSE.'
    }

    # A modified binary must never be accepted by staging using stale provenance.
    [System.IO.File]::AppendAllText((Join-Path $tdlib 'tdjson.dll'),'tampered')
    $rejected=$false
    try {
        & (Join-Path $root 'scripts\stage-native.ps1')
    }catch{
        if($_.Exception.Message -match 'Hash mismatch') { $rejected=$true }
        else { throw }
    }
    if(-not $rejected) { throw 'Staging accepted a DLL with an invalid SHA-256.' }
    Write-Host 'PASS: release config, MIT + third-party license staging, binary hash rejection'
} finally {
    if(Test-Path -LiteralPath $generated) { Remove-Item -LiteralPath $generated -Force }
    if(Test-Path -LiteralPath $native) { Remove-Item -LiteralPath $native -Force -Recurse }
}
