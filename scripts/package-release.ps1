param(
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$ValidateVersionOnly
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ($Version -notmatch '^v\d+\.\d+\.\d+$') {
    throw "Expected a release tag such as v0.1.0, got: $Version"
}

Push-Location $ProjectRoot
try {
    $metadataJson = & cargo metadata --no-deps --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not read locked Cargo package metadata.' }
    $metadata = $metadataJson | ConvertFrom-Json
    $package = @($metadata.packages | Where-Object { $_.name -eq 'cli-batch-runner' })
    if ($package.Count -ne 1 -or $Version -cne "v$($package[0].version)") {
        throw "Release tag $Version does not match the cli-batch-runner Cargo.toml version."
    }
    $executableName = 'cli-batch-runner.exe'
    $binaryTargets = @($package[0].targets | Where-Object { $_.kind -contains 'bin' })
    if ($binaryTargets.Count -ne 1 -or $binaryTargets[0].name -cne 'cli-batch-runner') {
        throw 'Cargo binary name does not match the distribution executable name.'
    }
    if ($ValidateVersionOnly) {
        Write-Output "Release version and binary name verified: $Version / $executableName"
        return
    }

    $executable = Join-Path $ProjectRoot "target\release\$executableName"
    & (Join-Path $PSScriptRoot 'verify-release.ps1') -Executable $executable

    # Public distribution allowlist. Build scripts, config, caches, logs, and sources are excluded.
    $files = @(
        'cli-batch-runner.exe',
        'README.md',
        'CHANGELOG.md',
        'LICENSE',
        'THIRDPARTY_LICENSES.md',
        'assets/themes/LICENSE-APACHE',
        'assets/themes/NOTICE.md',
        'docs/USER_GUIDE.md',
        'docs/screenshots/ready-to-run.png',
        'docs/screenshots/completed.png'
    )
    $packageName = "cli-batch-runner-$Version-windows-x64"
    $outputDirectory = Join-Path $ProjectRoot 'target\package'
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
    $staging = Join-Path $outputDirectory ('.staging-' + [guid]::NewGuid().ToString('N'))
    $staging = [System.IO.Path]::GetFullPath($staging)
    $outputPrefix = [System.IO.Path]::GetFullPath($outputDirectory) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $staging.StartsWith($outputPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Staging directory escaped the package output directory.'
    }
    $archivePath = Join-Path $outputDirectory "$packageName.zip"
    $temporaryZip = Join-Path $staging 'package.zip'
    try {
        $packageRoot = Join-Path $staging $packageName
        New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
        foreach ($relativePath in $files) {
            $source = if ($relativePath -ceq $executableName) { $executable } else { Join-Path $ProjectRoot $relativePath }
            if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
                throw "Distribution file is missing: $relativePath"
            }
            $destination = Join-Path $packageRoot $relativePath
            New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
            Copy-Item -LiteralPath $source -Destination $destination
        }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [System.IO.Compression.ZipFile]::CreateFromDirectory(
            $packageRoot, $temporaryZip, [System.IO.Compression.CompressionLevel]::Optimal, $true
        )
        $archive = [System.IO.Compression.ZipFile]::OpenRead($temporaryZip)
        try {
            $expected = @($files | ForEach-Object { "$packageName/$_" } | Sort-Object)
            $actual = @($archive.Entries | Where-Object { -not $_.FullName.EndsWith('/') } |
                ForEach-Object { $_.FullName } | Sort-Object)
            if ($actual.Count -ne $files.Count -or
                @(Compare-Object -ReferenceObject $expected -DifferenceObject $actual -CaseSensitive).Count -ne 0 -or
                @($actual | Select-Object -Unique).Count -ne $files.Count) {
                throw "Release ZIP contents differ from the public allowlist: $($actual -join ', ')"
            }
            foreach ($entry in $archive.Entries) {
                if (-not $entry.FullName.StartsWith("$packageName/", [System.StringComparison]::Ordinal) -or
                    $entry.FullName.Contains('..') -or $entry.FullName.Contains('\')) {
                    throw "Invalid release ZIP entry: $($entry.FullName)"
                }
                if (-not $entry.FullName.EndsWith('/') -and $entry.Length -eq 0) {
                    throw "Empty distribution file: $($entry.FullName)"
                }
            }
        }
        finally { $archive.Dispose() }
        Move-Item -LiteralPath $temporaryZip -Destination $archivePath -Force
        if ([System.IO.Path]::GetFileName($archivePath) -cne "$packageName.zip") {
            throw 'Unexpected release archive name.'
        }
        Write-Output "Release ZIP verified ($($files.Count) files): $archivePath"
    }
    finally {
        # This exact absolute path was checked against target/package before it was created.
        if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
    }
}
finally { Pop-Location }
