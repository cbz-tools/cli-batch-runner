$ErrorActionPreference = 'Stop'
$ProjectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

Push-Location $ProjectRoot
try {
    $originalEncodedRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
    $rustFlags = @()
    if (-not [string]::IsNullOrWhiteSpace($originalEncodedRustFlags)) {
        $rustFlags = @($originalEncodedRustFlags.Split([char]31, [System.StringSplitOptions]::RemoveEmptyEntries))
    }
    elseif (-not [string]::IsNullOrWhiteSpace($env:RUSTFLAGS)) {
        $rustFlags = @($env:RUSTFLAGS -split '\s+' | Where-Object { $_ })
    }
    elseif (-not [string]::IsNullOrWhiteSpace($env:CARGO_BUILD_RUSTFLAGS)) {
        $rustFlags = @($env:CARGO_BUILD_RUSTFLAGS -split '\s+' | Where-Object { $_ })
    }
    elseif (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS)) {
        $rustFlags = @($env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -split '\s+' | Where-Object { $_ })
    }

    $userProfile = [System.Environment]::GetFolderPath([System.Environment+SpecialFolder]::UserProfile)
    if ([string]::IsNullOrWhiteSpace($userProfile)) {
        throw 'Could not resolve the current user profile for Release path remapping.'
    }
    $pathRemapFlags = @(
        "--remap-path-prefix=$([System.IO.Path]::GetFullPath($userProfile))=~"
    )
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_HOME)) {
        $cargoHome = [System.IO.Path]::GetFullPath($env:CARGO_HOME)
        $pathRemapFlags += "--remap-path-prefix=$cargoHome=~/.cargo"
    }
    $pathRemapFlags += "--remap-path-prefix=$ProjectRoot=."

    # Remap compiler source paths in the public binary; these flags do not change lint levels.
    $env:CARGO_ENCODED_RUSTFLAGS = [string]::Join(
        [string][char]31,
        [string[]]($rustFlags + $pathRemapFlags)
    )
    try {
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release --locked failed with exit code $LASTEXITCODE"
        }

        & (Join-Path $PSScriptRoot 'embed-manifest.ps1')
        if ($LASTEXITCODE -ne 0) {
            throw "Manifest embedding failed with exit code $LASTEXITCODE"
        }

        & (Join-Path $PSScriptRoot 'verify-release.ps1') `
            -Executable (Join-Path $ProjectRoot 'target\release\cli-batch-runner.exe')
    }
    finally {
        if ($null -eq $originalEncodedRustFlags) {
            Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CARGO_ENCODED_RUSTFLAGS = $originalEncodedRustFlags
        }
    }
}
finally {
    Pop-Location
}
