#Requires -Version 5.1
<#
.SYNOPSIS
 Audit PE import dependencies without trusting the developer machine's PATH.
#>
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Directory)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Directory = [System.IO.Path]::GetFullPath($Directory)
if ($env:OS -ne 'Windows_NT') { throw 'Windows only.' }
if (-not (Test-Path -LiteralPath $Directory)) { throw "Directory not found: $Directory" }

function Find-Dumpbin {
    $command = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($null -ne $command) { return $command.Source }
    $base = [System.Environment]::GetFolderPath('ProgramFilesX86')
    $vswhere = Join-Path $base 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'vswhere.exe not found (MSVC required)' }
    $vs = (& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'MSVC installation not found' }
    $tools = Join-Path $vs 'VC\Tools\MSVC'
    $candidates = @(Get-ChildItem -LiteralPath $tools -Directory |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'bin\Hostx64\x64\dumpbin.exe' } |
        Where-Object { Test-Path -LiteralPath $_ })
    if ($candidates.Count -eq 0) { throw 'Could not locate x64 dumpbin.exe' }
    return $candidates[0]
}

$dumpbin = Find-Dumpbin
$localNames = @{}
$peFiles = @(Get-ChildItem -LiteralPath $Directory -File | Where-Object { $_.Extension -in @('.exe', '.dll') })
if ($peFiles.Count -eq 0) { throw 'No PE files to audit.' }
foreach ($entry in $peFiles) { $localNames[$entry.Name.ToLowerInvariant()] = $true }

foreach ($file in $peFiles) {
    $lines = @( & $dumpbin /dependents $file.FullName )
    if ($LASTEXITCODE -ne 0) { throw "dumpbin /dependents failed: $($file.Name)" }
    $dependencies = @($lines | ForEach-Object {
        if ($_ -match '^\s*([A-Za-z0-9_.-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() }
    } | Select-Object -Unique)
    foreach ($dep in $dependencies) {
        if ($localNames.ContainsKey($dep)) { continue }
        if ($dep -match '^(api-ms-win-|ext-ms-)') { continue }
        if ($dep -match '^(msvcp140.*|vcruntime140.*|concrt140|msvcp_win)\.dll$') {
            throw "$($file.Name): MSVC runtime $dep is absent from the portable distribution"
        }
        # System DLLs must be provided by Windows itself, not accidentally
        # satisfied by the developer/CI PATH or by files elsewhere on the runner.
        $system32 = Join-Path $env:WINDIR ('System32\' + $dep)
        if (-not (Test-Path -LiteralPath $system32)) {
            throw "$($file.Name): unbundled non-system dependency: $dep"
        }
    }
    Write-Host "$($file.Name): $($dependencies.Count) imports audited"
}
