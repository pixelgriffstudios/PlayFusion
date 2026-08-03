[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$AssetDirectory,

    [string]$Repository = 'pixelgriffstudios/PlayFusion',
    [string]$Tag = 'v1.0.2',
    [string]$GitExe = 'git.exe'
)

$ErrorActionPreference = 'Stop'
$assetRoot = [System.IO.Path]::GetFullPath($AssetDirectory)
$requiredNames = @(
    'PlayFusion-1.0.2-Public-Installer.img.part001',
    'PlayFusion-1.0.2-Public-Installer.img.part002',
    'PlayFusion-1.0.2-Public-Installer.img.part003',
    'PlayFusion-1.0.2-Public-Installer.img.part004',
    'PlayFusion-1.0.2-Public-Installer.img.part005',
    'README-FIRST.txt',
    'SHA256SUMS.txt',
    'Rebuild-PlayFusion-Installer.ps1',
    'Rebuild-PlayFusion-Installer.cmd',
    'Rebuild-PlayFusion-Installer-Linux.sh'
)

$files = foreach ($name in $requiredNames) {
    $path = Join-Path $assetRoot $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required release asset is missing: $path"
    }
    Get-Item -LiteralPath $path
}

$credentialInput = "protocol=https`nhost=github.com`n`n"
$credentialLines = @($credentialInput | & $GitExe credential fill)
if ($LASTEXITCODE -ne 0) { throw 'Git Credential Manager did not return GitHub credentials.' }
$passwordLine = $credentialLines | Where-Object { $_ -like 'password=*' } | Select-Object -First 1
if (-not $passwordLine) { throw 'No GitHub token was returned by Git Credential Manager.' }
$token = $passwordLine.Substring('password='.Length)

$apiHeaders = @{
    Authorization = "Bearer $token"
    Accept = 'application/vnd.github+json'
    'X-GitHub-Api-Version' = '2022-11-28'
    'User-Agent' = 'PlayFusion-release-maintenance'
}
$release = Invoke-RestMethod -Headers $apiHeaders -Uri "https://api.github.com/repos/$Repository/releases/tags/$Tag"
$uploadBase = $release.upload_url -replace '\{.*$', ''

function Get-ReleaseAssets {
    $response = Invoke-RestMethod -Headers $apiHeaders -Uri "https://api.github.com/repos/$Repository/releases/$($release.id)/assets?per_page=100"
    foreach ($asset in $response) {
        Write-Output $asset
    }
}

function Remove-ReleaseAsset([object]$Asset) {
    Invoke-RestMethod -Headers $apiHeaders -Method Delete -Uri "https://api.github.com/repos/$Repository/releases/assets/$($Asset.id)"
}

function Send-ReleaseAsset([System.IO.FileInfo]$File, [string]$RemoteName) {
    $encodedName = [System.Uri]::EscapeDataString($RemoteName)
    $curlConfig = @"
header = "Authorization: Bearer $token"
header = "Accept: application/vnd.github+json"
header = "X-GitHub-Api-Version: 2022-11-28"
user-agent = "PlayFusion-release-maintenance"
fail-with-body
silent
show-error
"@
    $response = $curlConfig | & curl.exe --config - --request POST `
        --header 'Content-Type: application/octet-stream' `
        --upload-file $File.FullName `
        "$uploadBase`?name=$encodedName"
    if ($LASTEXITCODE -ne 0) { throw "GitHub upload failed for $($File.Name)." }
    $asset = ($response -join "`n") | ConvertFrom-Json
    if ($asset.size -ne $File.Length) {
        throw "Uploaded size mismatch for $($File.Name): $($asset.size) instead of $($File.Length)."
    }
    $localDigest = 'sha256:' + (Get-FileHash -LiteralPath $File.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($asset.digest -and $asset.digest -ne $localDigest) {
        throw "Uploaded digest mismatch for $($File.Name)."
    }
    $asset
}

# Stage every replacement under a temporary name first. The current release
# remains usable until all large uploads have succeeded and verified.
$temporarySuffix = '.uploading-v102-fixed'
$assets = Get-ReleaseAssets
foreach ($file in $files) {
    $temporaryName = $file.Name + $temporarySuffix
    $existing = $assets | Where-Object name -eq $temporaryName | Select-Object -First 1
    $localDigest = 'sha256:' + (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($existing -and $existing.size -eq $file.Length -and
        (-not $existing.digest -or $existing.digest -eq $localDigest)) {
        Write-Host "Reusing verified staged asset $($file.Name)"
        continue
    }
    foreach ($stale in @($assets | Where-Object name -eq $temporaryName)) {
        Remove-ReleaseAsset $stale
    }
    Write-Host "Uploading and verifying $($file.Name)..."
    $null = Send-ReleaseAsset $file $temporaryName
}

$assets = Get-ReleaseAssets
foreach ($file in $files) {
    $temporaryName = $file.Name + $temporarySuffix
    $temporaryAsset = $assets | Where-Object name -eq $temporaryName | Select-Object -First 1
    if (-not $temporaryAsset) { throw "Staged asset disappeared: $temporaryName" }
    if ($temporaryAsset.size -ne $file.Length) { throw "Staged asset size changed: $temporaryName" }
}

# Atomically switch names only after the complete replacement set exists.
foreach ($file in $files) {
    foreach ($old in @(Get-ReleaseAssets | Where-Object name -eq $file.Name)) {
        Remove-ReleaseAsset $old
    }
    $temporaryName = $file.Name + $temporarySuffix
    $temporaryAsset = Get-ReleaseAssets | Where-Object name -eq $temporaryName | Select-Object -First 1
    $body = @{ name = $file.Name } | ConvertTo-Json -Compress
    $null = Invoke-RestMethod -Headers $apiHeaders -Method Patch -ContentType 'application/json' `
        -Body $body -Uri "https://api.github.com/repos/$Repository/releases/assets/$($temporaryAsset.id)"
    Write-Host "Published $($file.Name)"
}

$finalAssets = Get-ReleaseAssets
foreach ($file in $files) {
    $published = $finalAssets | Where-Object name -eq $file.Name | Select-Object -First 1
    if (-not $published -or $published.size -ne $file.Length) {
        throw "Final release validation failed for $($file.Name)."
    }
}
Write-Host 'INSTALLER_RELEASE_ASSETS_OK'
