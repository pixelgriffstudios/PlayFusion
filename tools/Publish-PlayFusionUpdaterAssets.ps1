[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$AssetDirectory,

    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,

    [string]$Repository = 'pixelgriffstudios/PlayFusion',
    [string]$GitExe = 'git.exe'
)

$ErrorActionPreference = 'Stop'
$assetRoot = [IO.Path]::GetFullPath($AssetDirectory)
$tag = "v$Version"
$requiredNames = @(
    "PlayFusion-update-v$Version.pfu",
    "PlayFusion-update-v$Version.pfu.sha256",
    "PlayFusion-update-v$Version.pfu.sig",
    "PlayFusion-legacy-update-v$Version.zip",
    "SHA256SUMS-v$Version.txt"
)
$files = foreach ($name in $requiredNames) {
    $path = Join-Path $assetRoot $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required updater asset is missing: $path"
    }
    Get-Item -LiteralPath $path
}

$credentialInput = "protocol=https`nhost=github.com`n`n"
# PowerShell pipelines can add encoding/record separators that `git credential
# fill` interprets as a malformed first record. Feed the protocol exactly over
# redirected standard input and never echo the returned secret.
$credentialFile = [IO.Path]::GetTempFileName()
[IO.File]::WriteAllText($credentialFile, $credentialInput, [Text.Encoding]::ASCII)
$startInfo = New-Object Diagnostics.ProcessStartInfo
$startInfo.FileName = $env:ComSpec
$startInfo.Arguments = '/d /s /c ""' + $GitExe + '" credential fill < "' + $credentialFile + '""'
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$process = New-Object Diagnostics.Process
$process.StartInfo = $startInfo
$null = $process.Start()
$credentialOutput = $process.StandardOutput.ReadToEnd()
$credentialError = $process.StandardError.ReadToEnd()
$process.WaitForExit()
Remove-Item -LiteralPath $credentialFile -Force -ErrorAction SilentlyContinue
if ($process.ExitCode -ne 0) { throw "Git Credential Manager did not return GitHub credentials: $credentialError" }
$credentialLines = @($credentialOutput -split "`r?`n")
$passwordLine = $credentialLines | Where-Object { $_ -like 'password=*' } | Select-Object -First 1
if (-not $passwordLine) { throw 'No GitHub token was returned by Git Credential Manager.' }
$token = $passwordLine.Substring('password='.Length)

$headers = @{
    Authorization = "Bearer $token"
    Accept = 'application/vnd.github+json'
    'X-GitHub-Api-Version' = '2022-11-28'
    'User-Agent' = 'PlayFusion-updater-publisher'
}
$release = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$Repository/releases/tags/$tag"
$uploadBase = $release.upload_url -replace '\{.*$', ''

function Get-Assets {
    $response = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$Repository/releases/$($release.id)/assets?per_page=100"
    foreach ($asset in $response) { Write-Output $asset }
}
function Remove-Asset([object]$Asset) {
    Invoke-RestMethod -Headers $headers -Method Delete -Uri "https://api.github.com/repos/$Repository/releases/assets/$($Asset.id)" | Out-Null
}
function Send-Asset([IO.FileInfo]$File, [string]$RemoteName) {
    $encoded = [Uri]::EscapeDataString($RemoteName)
    $curlConfig = @"
header = "Authorization: Bearer $token"
header = "Accept: application/vnd.github+json"
header = "X-GitHub-Api-Version: 2022-11-28"
user-agent = "PlayFusion-updater-publisher"
fail-with-body
silent
show-error
"@
    $curlConfigFile = [IO.Path]::GetTempFileName()
    try {
        [IO.File]::WriteAllText($curlConfigFile, $curlConfig, [Text.Encoding]::ASCII)
        $response = & curl.exe --config $curlConfigFile --request POST `
            --header 'Content-Type: application/octet-stream' `
            --upload-file $File.FullName "$uploadBase`?name=$encoded"
        if ($LASTEXITCODE -ne 0) { throw "GitHub upload failed for $($File.Name)." }
    }
    finally {
        Remove-Item -LiteralPath $curlConfigFile -Force -ErrorAction SilentlyContinue
    }
    $asset = ($response -join "`n") | ConvertFrom-Json
    $digest = 'sha256:' + (Get-FileHash -LiteralPath $File.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($asset.size -ne $File.Length) { throw "Uploaded size mismatch for $($File.Name)." }
    if ($asset.digest -and $asset.digest -ne $digest) { throw "Uploaded digest mismatch for $($File.Name)." }
    $asset
}

# Upload every replacement under a temporary name. Existing working assets
# remain available until the complete replacement set has uploaded and passed
# remote size/digest validation.
$suffix = '.uploading-verified'
$assets = Get-Assets
foreach ($file in $files) {
    $temporaryName = $file.Name + $suffix
    $digest = 'sha256:' + (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $existing = $assets | Where-Object name -eq $temporaryName | Select-Object -First 1
    if ($existing -and $existing.size -eq $file.Length -and
        (-not $existing.digest -or $existing.digest -eq $digest)) {
        Write-Host "Reusing verified staged asset $($file.Name)"
        continue
    }
    foreach ($stale in @($assets | Where-Object name -eq $temporaryName)) { Remove-Asset $stale }
    Write-Host "Staging $($file.Name)..."
    Send-Asset $file $temporaryName | Out-Null
}

$assets = Get-Assets
foreach ($file in $files) {
    $temporaryName = $file.Name + $suffix
    $staged = $assets | Where-Object name -eq $temporaryName | Select-Object -First 1
    if (-not $staged -or $staged.size -ne $file.Length) { throw "Staged asset validation failed: $temporaryName" }
}

foreach ($file in $files) {
    $temporaryName = $file.Name + $suffix
    $currentAssets = @(Get-Assets)
    $staged = $currentAssets | Where-Object name -eq $temporaryName | Select-Object -First 1
    if (-not $staged) { throw "Verified staged asset disappeared: $temporaryName" }
    foreach ($old in @($currentAssets | Where-Object name -eq $file.Name)) { Remove-Asset $old }
    $body = @{ name = $file.Name } | ConvertTo-Json -Compress
    Invoke-RestMethod -Headers $headers -Method Patch -ContentType 'application/json' -Body $body `
        -Uri "https://api.github.com/repos/$Repository/releases/assets/$($staged.id)" | Out-Null
    Write-Host "Published $($file.Name)"
}

$final = Get-Assets
foreach ($file in $files) {
    $published = $final | Where-Object name -eq $file.Name | Select-Object -First 1
    if (-not $published -or $published.size -ne $file.Length) { throw "Final release validation failed: $($file.Name)" }
}
Write-Host "PLAYFUSION_UPDATER_ASSETS_OK $tag"
