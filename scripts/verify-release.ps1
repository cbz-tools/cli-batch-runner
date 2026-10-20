param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\target\release\cli-batch-runner.exe')
)

$ErrorActionPreference = 'Stop'
$Executable = [System.IO.Path]::GetFullPath($Executable)
if ([System.IO.Path]::GetFileName($Executable) -cne 'cli-batch-runner.exe') {
    throw "Unexpected Release executable name: $Executable"
}
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    throw "Release executable not found: $Executable"
}

$bytes = [System.IO.File]::ReadAllBytes($Executable)
if ($bytes.Length -lt 256 -or [System.Text.Encoding]::ASCII.GetString($bytes, 0, 2) -ne 'MZ') {
    throw 'Release executable has no valid DOS/PE header.'
}
$peOffset = [System.BitConverter]::ToInt32($bytes, 0x3c)
if ($peOffset -lt 0 -or $peOffset + 24 -ge $bytes.Length -or
    [System.Text.Encoding]::ASCII.GetString($bytes, $peOffset, 4) -ne "PE`0`0") {
    throw 'Release executable has no valid PE signature.'
}
$optionalHeaderOffset = $peOffset + 24
$machine = [System.BitConverter]::ToUInt16($bytes, $peOffset + 4)
if ($machine -ne 0x8664) {
    throw "Expected Windows x64 machine (0x8664), found 0x$($machine.ToString('x'))."
}
$optionalHeaderMagic = [System.BitConverter]::ToUInt16($bytes, $optionalHeaderOffset)
if ($optionalHeaderMagic -ne 0x20b) {
    throw "Unsupported PE optional-header magic: 0x$($optionalHeaderMagic.ToString('x'))"
}
$subsystem = [System.BitConverter]::ToUInt16($bytes, $optionalHeaderOffset + 68)
if ($subsystem -ne 2) {
    throw "Expected Windows GUI subsystem (2), found $subsystem."
}

Add-Type -AssemblyName System.Drawing
$applicationIcon = [System.Drawing.Icon]::ExtractAssociatedIcon($Executable)
if ($null -eq $applicationIcon) {
    throw 'Release executable has no extractable application icon.'
}
$applicationIcon.Dispose()

$mt = Get-Command mt.exe -ErrorAction SilentlyContinue
if (-not $mt) {
    $sdkBinRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $mt = Get-ChildItem -LiteralPath $sdkBinRoot -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64\mt.exe' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
}
if (-not $mt) {
    throw 'mt.exe was not found in PATH or the Windows 10 SDK x64 bin directories.'
}
$mtPath = if ($mt -is [string]) { $mt } else { $mt.Source }
$manifestPath = Join-Path ([System.IO.Path]::GetTempPath()) ("cli-batch-runner-manifest-{0}.xml" -f [guid]::NewGuid())
try {
    & $mtPath "-inputresource:$Executable;#1" "-out:$manifestPath"
    if ($LASTEXITCODE -ne 0) {
        throw "Could not extract the embedded application manifest (mt.exe exit code $LASTEXITCODE)."
    }
    [xml]$manifest = [System.IO.File]::ReadAllText($manifestPath)
    $longPathAware = $manifest.SelectSingleNode("//*[local-name()='longPathAware']")
    if ($null -eq $longPathAware -or $longPathAware.InnerText.Trim() -ne 'true') {
        throw 'Embedded manifest does not enable longPathAware.'
    }
    $dpiAwareness = $manifest.SelectSingleNode("//*[local-name()='dpiAwareness']")
    if ($null -eq $dpiAwareness -or $dpiAwareness.InnerText.Trim() -ne 'PerMonitorV2') {
        throw 'Embedded manifest does not declare PerMonitorV2 DPI awareness.'
    }
    $executionLevel = $manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    if ($null -eq $executionLevel -or $executionLevel.GetAttribute('level') -ne 'asInvoker') {
        throw 'Embedded manifest does not preserve the asInvoker execution level.'
    }
    $commonControls = $manifest.SelectSingleNode("//*[local-name()='assemblyIdentity' and @name='Microsoft.Windows.Common-Controls' and @version='6.0.0.0']")
    if ($null -eq $commonControls) {
        throw 'Embedded manifest does not include Common Controls version 6.0.'
    }
}
finally {
    Remove-Item -LiteralPath $manifestPath -ErrorAction SilentlyContinue
}

Write-Output 'Release executable verified: cli-batch-runner.exe, Windows x64, GUI subsystem, application icon, longPathAware, PerMonitorV2, asInvoker, and Common Controls 6.0.'
