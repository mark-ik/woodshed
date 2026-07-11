[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$')]
    [string]$Version,

    [string]$OutputDir = "dist"
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$target = if ($env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR
} else {
    Join-Path $root "target"
}
$binary = Join-Path $target "release\woodshed-serval.exe"
if (-not (Test-Path -LiteralPath $binary)) {
    throw "Release binary not found: $binary. Run 'cargo build --release -p woodshed-serval' first."
}

$output = Join-Path $root $OutputDir
$name = "Woodshed-$Version-windows-x86_64"
$stage = Join-Path $output $name
$archive = Join-Path $output "$name.zip"
$checksum = "$archive.sha256"

if (Test-Path -LiteralPath $stage) {
    throw "Staging directory already exists: $stage. Choose a clean output directory."
}

New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $stage "Woodshed.exe")
Copy-Item -LiteralPath (Join-Path $root "README.md") -Destination $stage
Copy-Item -LiteralPath (Join-Path $root "LICENSE-MIT") -Destination $stage
Copy-Item -LiteralPath (Join-Path $root "LICENSE-APACHE") -Destination $stage
Copy-Item -LiteralPath (Join-Path $root "Cargo.lock") -Destination $stage

@"
Woodshed $Version — Windows x86_64 portable build

Run Woodshed.exe. This alpha build stores its session in your local application
data directory. It needs an available audio output device; microphone access is
only used when you turn the tuner or latency calibration on.

This is a portable ZIP, not an installer. Windows SmartScreen may warn because
the binary is not code-signed yet.

The source, project licenses, and Cargo.lock dependency inventory are included
in the repository: https://github.com/mark-ik/woodshed
"@ | Set-Content -LiteralPath (Join-Path $stage "RELEASE-README.txt") -NoNewline

Compress-Archive -Path $stage -DestinationPath $archive -Force
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
"$hash  $([IO.Path]::GetFileName($archive))" | Set-Content -LiteralPath $checksum -NoNewline

Write-Output $archive
Write-Output $checksum
