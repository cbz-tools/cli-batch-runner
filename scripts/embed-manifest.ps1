param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\target\release\cli-batch-runner.exe'),
    [string]$Manifest = (Join-Path $PSScriptRoot '..\assets\cli-batch-runner.manifest')
)

$ErrorActionPreference = 'Stop'
$Executable = [System.IO.Path]::GetFullPath($Executable)
$Manifest = [System.IO.Path]::GetFullPath($Manifest)
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    throw "Release executable not found: $Executable"
}
if (-not (Test-Path -LiteralPath $Manifest -PathType Leaf)) {
    throw "Manifest not found: $Manifest"
}
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
& $mtPath -manifest $Manifest "-outputresource:$Executable;#1"
if ($LASTEXITCODE -ne 0) {
    throw "mt.exe failed with exit code $LASTEXITCODE"
}
