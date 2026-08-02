[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$InputImage,

    [Parameter(Mandatory)]
    [string]$OutputDirectory,

    [string]$Version = '1.0.2',

    # GitHub release assets are limited to 2 GiB. Two billion bytes is a
    # conservative "2 GB" part that remains safely below that hard ceiling.
    [long]$PartSizeBytes = 2000000000
)

$ErrorActionPreference = 'Stop'
$image = Get-Item -LiteralPath $InputImage
if ($image.Length -le 0) { throw 'The installer image is empty.' }
if ($PartSizeBytes -le 0 -or $PartSizeBytes -ge 2147483648) {
    throw 'PartSizeBytes must be positive and below GitHub''s 2 GiB limit.'
}

$output = [System.IO.Path]::GetFullPath($OutputDirectory)
[System.IO.Directory]::CreateDirectory($output) | Out-Null
$baseName = "PlayFusion-$Version-Public-Installer.img"

# Clean only this version's generated artifacts inside the explicitly named
# release directory. Other releases and arbitrary user files are untouched.
Get-ChildItem -LiteralPath $output -File -ErrorAction SilentlyContinue |
    Where-Object {
        $_.Name -like "$baseName.part*" -or
        $_.Name -in @(
            'README-FIRST.txt', 'SHA256SUMS.txt',
            'Rebuild-PlayFusion-Installer.ps1',
            'Rebuild-PlayFusion-Installer.cmd',
            'Rebuild-PlayFusion-Installer-Linux.sh'
        )
    } | Remove-Item -Force

$buffer = New-Object byte[] (8MB)
$input = [System.IO.File]::OpenRead($image.FullName)
$parts = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
try {
    $index = 1
    while ($input.Position -lt $input.Length) {
        $partName = '{0}.part{1:D3}' -f $baseName, $index
        $partPath = Join-Path $output $partName
        $part = [System.IO.File]::Create($partPath)
        try {
            $remaining = [Math]::Min($PartSizeBytes, $input.Length - $input.Position)
            while ($remaining -gt 0) {
                $wanted = [int][Math]::Min($buffer.Length, $remaining)
                $read = $input.Read($buffer, 0, $wanted)
                if ($read -le 0) { throw 'Unexpected end of installer image.' }
                $part.Write($buffer, 0, $read)
                $remaining -= $read
            }
        }
        finally {
            $part.Dispose()
        }
        $parts.Add((Get-Item -LiteralPath $partPath))
        Write-Host ("Created {0} ({1:N0} bytes)" -f $partName, $parts[-1].Length)
        $index++
    }
}
finally {
    $input.Dispose()
}

$imageHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $image.FullName).Hash.ToLowerInvariant()
$checksumLines = foreach ($part in $parts) {
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $part.FullName).Hash.ToLowerInvariant()
    "$hash  $($part.Name)"
}
$checksumLines += "$imageHash  $baseName"
[System.IO.File]::WriteAllLines(
    (Join-Path $output 'SHA256SUMS.txt'),
    $checksumLines,
    [System.Text.UTF8Encoding]::new($false)
)

$rebuildPs1 = @'
$ErrorActionPreference = 'Stop'
$folder = Split-Path -Parent $MyInvocation.MyCommand.Path
$parts = Get-ChildItem -LiteralPath $folder -Filter 'PlayFusion-*-Public-Installer.img.part*' |
    Sort-Object Name
if (-not $parts) { throw 'No numbered installer parts were found.' }
$outputName = $parts[0].Name -replace '\.part\d+$', ''
$outputPath = Join-Path $folder $outputName
$stream = [System.IO.File]::Create($outputPath)
try {
    foreach ($part in $parts) {
        $source = [System.IO.File]::OpenRead($part.FullName)
        try { $source.CopyTo($stream) } finally { $source.Dispose() }
    }
}
finally { $stream.Dispose() }
$expectedLine = Get-Content -LiteralPath (Join-Path $folder 'SHA256SUMS.txt') |
    Where-Object { $_ -match [regex]::Escape($outputName) + '$' } |
    Select-Object -First 1
if (-not $expectedLine) { throw 'The complete-image checksum is missing.' }
$expected = ($expectedLine -split '\s+')[0]
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $outputPath).Hash
if ($actual -ine $expected) { throw "Rebuilt image checksum failed: $actual" }
Write-Host "Rebuilt and verified: $outputPath"
'@
[System.IO.File]::WriteAllText(
    (Join-Path $output 'Rebuild-PlayFusion-Installer.ps1'),
    $rebuildPs1,
    [System.Text.UTF8Encoding]::new($false)
)

$rebuildCmd = @'
@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Rebuild-PlayFusion-Installer.ps1"
pause
'@
[System.IO.File]::WriteAllText(
    (Join-Path $output 'Rebuild-PlayFusion-Installer.cmd'),
    $rebuildCmd,
    [System.Text.ASCIIEncoding]::new()
)

$rebuildLinux = @'
#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
base=$(find . -maxdepth 1 -type f -name 'PlayFusion-*-Public-Installer.img.part001' -printf '%f\n' | head -n1)
test -n "$base"
output=${base%.part001}
grep '\.part[0-9][0-9][0-9]$' SHA256SUMS.txt | sha256sum -c -
cat -- "$output".part[0-9][0-9][0-9] > "$output"
expected=$(awk -v file="$output" '$2 == file { print $1 }' SHA256SUMS.txt)
actual=$(sha256sum "$output" | awk '{ print $1 }')
test -n "$expected" && test "$actual" = "$expected"
printf 'Rebuilt and verified: %s\n' "$output"
'@
[System.IO.File]::WriteAllText(
    (Join-Path $output 'Rebuild-PlayFusion-Installer-Linux.sh'),
    $rebuildLinux,
    [System.Text.UTF8Encoding]::new($false)
)

$readme = @"
PlayFusion $Version testing installer
=====================================

1. Download every numbered .part file plus SHA256SUMS.txt and the rebuild
   script for your operating system.
2. Windows: double-click Rebuild-PlayFusion-Installer.cmd.
   Linux: run: chmod +x Rebuild-PlayFusion-Installer-Linux.sh
               ./Rebuild-PlayFusion-Installer-Linux.sh
3. The script rebuilds $baseName and verifies its SHA-256 checksum.
4. Flash the verified IMG with BalenaEtcher or another raw-image writer.

WARNING: Installing PlayFusion erases the selected destination disk.
PlayFusion 1.0.1 remains available as a rollback while 1.0.2 is qualified on
real hardware. Use only games, firmware, keys, and media you are authorized to
use.

Part size: $PartSizeBytes bytes (below GitHub's 2 GiB asset limit)
Complete image SHA-256: $imageHash
"@
[System.IO.File]::WriteAllText(
    (Join-Path $output 'README-FIRST.txt'),
    $readme,
    [System.Text.UTF8Encoding]::new($false)
)

Write-Host "PART_COUNT=$($parts.Count)"
Write-Host "IMAGE_BYTES=$($image.Length)"
Write-Host "IMAGE_SHA256=$imageHash"
