param(
  [switch]$SkipBuild,
  [string]$Architecture = "x64",
  [string]$UpdaterPath = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$config = Get-Content -LiteralPath $tauriConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
$version = [string]$config.version
if ([string]::IsNullOrWhiteSpace($version)) { throw "Tauri version is missing" }

$publicKeyPath = Join-Path $repoRoot "src-tauri\update-public-key.txt"
if (-not (Test-Path -LiteralPath $publicKeyPath -PathType Leaf)) {
  throw "Updater public key file is missing."
}
$publicKeyText = (Get-Content -LiteralPath $publicKeyPath -Raw -Encoding UTF8).Trim()
try {
  $publicKeyBytes = [Convert]::FromBase64String($publicKeyText)
} catch {
  throw "Updater public key must be canonical Base64, not UNCONFIGURED."
}
if ($publicKeyText -notmatch '^[A-Za-z0-9+/]{43}=$' -or $publicKeyBytes.Length -ne 32) {
  throw "Updater public key must contain exactly 32 Ed25519 bytes."
}
if ([Convert]::ToBase64String($publicKeyBytes) -cne $publicKeyText) {
  throw "Updater public key must use canonical standard Base64."
}

$package = Get-Content -LiteralPath (Join-Path $repoRoot "package.json") -Raw -Encoding UTF8 | ConvertFrom-Json
$cargoManifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$cargoManifest = Get-Content -LiteralPath $cargoManifestPath -Raw -Encoding UTF8
$cargoVersionMatch = [regex]::Match($cargoManifest, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $cargoVersionMatch.Success) { throw "Cargo package version is missing" }
$cargoLockPath = Join-Path $repoRoot "src-tauri\Cargo.lock"
$cargoLock = Get-Content -LiteralPath $cargoLockPath -Raw -Encoding UTF8
$cargoLockVersionMatch = [regex]::Match($cargoLock, '(?ms)^\[\[package\]\]\s*\r?\nname\s*=\s*"m2shelf"\s*\r?\nversion\s*=\s*"([^"]+)"')
if (-not $cargoLockVersionMatch.Success) { throw "Cargo lockfile package version is missing" }
$boundVersions = @(
  [string]$package.version,
  [string]$cargoVersionMatch.Groups[1].Value,
  [string]$cargoLockVersionMatch.Groups[1].Value,
  $version
) | Select-Object -Unique
if ($boundVersions.Count -ne 1) {
  throw "package.json, Cargo.toml, Cargo.lock, and tauri.conf.json versions must match: $($boundVersions -join ', ')"
}

$machineByArchitecture = @{
  "x64" = [uint16]0x8664
  "ARM64" = [uint16]0xAA64
  "x86" = [uint16]0x014C
}
if (-not $machineByArchitecture.ContainsKey($Architecture)) {
  throw "Unsupported Portable architecture: $Architecture"
}

function Get-PeMachine {
  param([Parameter(Mandatory = $true)][string]$Path)

  $stream = [System.IO.File]::OpenRead($Path)
  $reader = [System.IO.BinaryReader]::new($stream)
  try {
    if ($stream.Length -lt 0x40) { throw "PE file is too small: $([System.IO.Path]::GetFileName($Path))" }
    $stream.Position = 0x3C
    $peOffset = $reader.ReadInt32()
    if ($peOffset -lt 0 -or ($peOffset + 6) -gt $stream.Length) {
      throw "PE header offset is invalid: $([System.IO.Path]::GetFileName($Path))"
    }
    $stream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x00004550) {
      throw "PE signature is invalid: $([System.IO.Path]::GetFileName($Path))"
    }
    return $reader.ReadUInt16()
  } finally {
    $reader.Dispose()
    $stream.Dispose()
  }
}

function Assert-PeArchitecture {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$ExpectedArchitecture
  )

  $actualMachine = Get-PeMachine -Path $Path
  $expectedMachine = $machineByArchitecture[$ExpectedArchitecture]
  if ($actualMachine -ne $expectedMachine) {
    throw "$([System.IO.Path]::GetFileName($Path)) architecture does not match $ExpectedArchitecture (PE machine 0x$($actualMachine.ToString('X4')))."
  }
}

function Assert-NoPrivateBuildPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $utf8 = [System.Text.Encoding]::UTF8.GetString($bytes)
  $utf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
  $privatePaths = @($repoRoot, $env:USERPROFILE) |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
    ForEach-Object { [System.IO.Path]::GetFullPath($_).TrimEnd('\') } |
    Select-Object -Unique
  foreach ($privatePath in $privatePaths) {
    foreach ($pathVariant in @($privatePath, $privatePath.Replace('\', '/')) | Select-Object -Unique) {
      if ($utf8.IndexOf($pathVariant, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or
          $utf16.IndexOf($pathVariant, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
        throw "$([System.IO.Path]::GetFileName($Path)) contains a private builder path. Rebuild through build_windows_release.ps1."
      }
    }
  }
}

if (-not $SkipBuild) {
  & (Join-Path $PSScriptRoot "build_windows_release.ps1") -Bundles none
  if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE" }
}

$releaseDirectory = Join-Path $repoRoot "src-tauri\target\release"
$workerPayloadFiles = @('M2ShelfMobi.exe', 'M2ShelfMobi-source.zip', 'THIRD-PARTY-NOTICES.txt')
foreach ($name in $workerPayloadFiles) {
  $workerFile = Get-Item -LiteralPath (Join-Path $releaseDirectory $name)
  if (-not ($workerFile -is [System.IO.FileInfo]) -or ($workerFile.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { throw "Invalid worker payload: $name" }
}
Assert-PeArchitecture -Path (Join-Path $releaseDirectory 'M2ShelfMobi.exe') -ExpectedArchitecture $Architecture
Assert-NoPrivateBuildPath -Path (Join-Path $releaseDirectory 'M2ShelfMobi.exe')
$binaryCandidates = @(
  (Join-Path $releaseDirectory "m2shelf.exe"),
  (Join-Path $releaseDirectory "M2Shelf.exe")
)
$sourceBinary = $binaryCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $sourceBinary) { throw "Release executable was not found. Run the Tauri release build first." }

$updaterCandidates = if ([string]::IsNullOrWhiteSpace($UpdaterPath)) {
  @(
    (Join-Path $releaseDirectory "M2ShelfUpdater.exe"),
    (Join-Path $releaseDirectory "m2shelf-updater.exe"),
    (Join-Path $releaseDirectory "m2shelf_updater.exe")
  )
} else {
  @([System.IO.Path]::GetFullPath($UpdaterPath))
}
$sourceUpdater = $updaterCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $sourceUpdater) {
  throw "M2ShelfUpdater release helper was not found. Build the updater helper before packaging."
}

$identityOutput = @(& $sourceUpdater identity)
if ($LASTEXITCODE -ne 0) { throw "M2ShelfUpdater identity check failed." }
$identityLines = @($identityOutput | ForEach-Object { [string]$_ } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
if ($identityLines.Count -ne 1) { throw "M2ShelfUpdater identity must be exactly one JSON object." }
try { $identity = $identityLines[0] | ConvertFrom-Json } catch { throw "M2ShelfUpdater identity is invalid JSON." }
$identityProperties = @($identity.PSObject.Properties.Name | Sort-Object)
$expectedIdentityProperties = @("appId", "publicKey", "schemaVersion", "version") | Sort-Object
if (($identityProperties -join "`n") -cne ($expectedIdentityProperties -join "`n") -or
    [uint32]$identity.schemaVersion -ne 1 -or
    [string]$identity.appId -cne "app.morimediashelf.desktop" -or
    [string]$identity.version -cne $version -or
    [string]$identity.publicKey -cne $publicKeyText) {
  throw "M2ShelfUpdater identity, version, or embedded public key does not match this release."
}

$expectedProductName = "M$([char]0x00B2)Shelf"
$binaryVersion = (Get-Item -LiteralPath $sourceBinary).VersionInfo
if ($binaryVersion.ProductVersion -ne $version -or $binaryVersion.ProductName -ne $expectedProductName) {
  throw "Release executable metadata does not match $expectedProductName $version. Rebuild before using -SkipBuild."
}
Assert-PeArchitecture -Path $sourceBinary -ExpectedArchitecture $Architecture
Assert-PeArchitecture -Path $sourceUpdater -ExpectedArchitecture $Architecture

$helperVersionInfo = (Get-Item -LiteralPath $sourceUpdater).VersionInfo
$helperProductVersion = [string]$helperVersionInfo.ProductVersion
$helperProductName = [string]$helperVersionInfo.ProductName
$meaningfulHelperVersion = -not [string]::IsNullOrWhiteSpace($helperProductVersion) -and
  $helperProductVersion -notin @("0.0.0.0", "0.0.0")
if ($meaningfulHelperVersion -and
    $helperProductVersion -ne $version -and
    $helperProductVersion -ne "$version.0") {
  throw "M2ShelfUpdater version metadata does not match $version. Rebuild the helper."
}
if (-not [string]::IsNullOrWhiteSpace($helperProductName) -and
    $helperProductName -notin @("M2ShelfUpdater", "M²Shelf Updater", $expectedProductName)) {
  throw "M2ShelfUpdater product metadata is unexpected. Refusing to package an unrelated helper."
}
$sourceUpdaterHash = Get-FileHash -LiteralPath $sourceUpdater -Algorithm SHA256

$freshnessFiles = @(
  (Join-Path $repoRoot "package.json"),
  (Join-Path $repoRoot "package-lock.json"),
  (Join-Path $repoRoot "index.html"),
  (Join-Path $repoRoot "vite.config.ts"),
  (Join-Path $repoRoot "tsconfig.json"),
  (Join-Path $repoRoot "tsconfig.node.json"),
  $cargoManifestPath,
  $cargoLockPath,
  (Join-Path $repoRoot "src-tauri\build.rs"),
  $tauriConfigPath,
  (Join-Path $repoRoot "src-tauri\update-public-key.txt"),
  (Join-Path $repoRoot "src-tauri\windows-app-manifest.xml"),
  (Join-Path $repoRoot "src-tauri\icons\icon.ico")
)
$freshnessDirectories = @(
  (Join-Path $repoRoot "dist"),
  (Join-Path $repoRoot "src"),
  (Join-Path $repoRoot "src-tauri\capabilities"),
  (Join-Path $repoRoot "src-tauri\migrations"),
  (Join-Path $repoRoot "src-tauri\src"),
  (Join-Path $repoRoot "src-tauri\native"),
  (Join-Path $repoRoot "src-tauri\vendor")
)
foreach ($directory in $freshnessDirectories) {
  if (Test-Path -LiteralPath $directory -PathType Container) {
    $freshnessFiles += Get-ChildItem -LiteralPath $directory -File -Recurse | Select-Object -ExpandProperty FullName
  }
}
$newestInput = $freshnessFiles |
  Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
  ForEach-Object { Get-Item -LiteralPath $_ } |
  Sort-Object LastWriteTimeUtc -Descending |
  Select-Object -First 1
$releaseBinary = Get-Item -LiteralPath $sourceBinary
if ($newestInput -and $releaseBinary.LastWriteTimeUtc -lt $newestInput.LastWriteTimeUtc) {
  throw "Release executable is older than $($newestInput.FullName). Run a fresh Tauri build before packaging."
}
$releaseUpdater = Get-Item -LiteralPath $sourceUpdater
if ($newestInput -and $releaseUpdater.LastWriteTimeUtc -lt $newestInput.LastWriteTimeUtc) {
  throw "M2ShelfUpdater is older than a release input. Run a fresh helper build before packaging."
}
Assert-NoPrivateBuildPath -Path $sourceBinary
Assert-NoPrivateBuildPath -Path $sourceUpdater

$bundleDirectory = Join-Path $repoRoot "bundle"
[System.IO.Directory]::CreateDirectory($bundleDirectory) | Out-Null
$stageDirectory = Join-Path $bundleDirectory (".portable-stage-" + $version)
$expectedStageRoot = [System.IO.Path]::GetFullPath($bundleDirectory).TrimEnd('\') + '\'
$resolvedStage = [System.IO.Path]::GetFullPath($stageDirectory)
if (-not $resolvedStage.StartsWith($expectedStageRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Unsafe staging directory: $resolvedStage"
}
if (Test-Path -LiteralPath $resolvedStage) { Remove-Item -LiteralPath $resolvedStage -Recurse -Force }
[System.IO.Directory]::CreateDirectory($resolvedStage) | Out-Null

$archiveName = "M2Shelf-Portable-$version-$Architecture.zip"
$archivePath = Join-Path $bundleDirectory $archiveName
$archiveHashPath = "$archivePath.sha256"
try {
  $portableExe = Join-Path $resolvedStage "M2Shelf.exe"
  $portableUpdater = Join-Path $resolvedStage "M2ShelfUpdater.exe"
  $portableMarker = Join-Path $resolvedStage "M2Shelf.portable.json"
  $portableReadme = Join-Path $resolvedStage "README_zh-CN.txt"
  Copy-Item -LiteralPath $sourceBinary -Destination $portableExe -Force
  Copy-Item -LiteralPath $sourceUpdater -Destination $portableUpdater -Force
  foreach ($name in $workerPayloadFiles) { Copy-Item -LiteralPath (Join-Path $releaseDirectory $name) -Destination (Join-Path $resolvedStage $name) }

  $marker = [ordered]@{
    schemaVersion = 1
    appId = "app.morimediashelf.desktop"
    distribution = "portable"
  } | ConvertTo-Json -Depth 3
  [System.IO.File]::WriteAllText($portableMarker, "$marker`n", [System.Text.UTF8Encoding]::new($false))

  $readmeTemplate = Get-Content -LiteralPath (Join-Path $repoRoot "docs\PORTABLE_README_zh-CN.txt") -Raw -Encoding UTF8
  $readme = $readmeTemplate.Replace("{{VERSION}}", $version)
  [System.IO.File]::WriteAllText($portableReadme, $readme, [System.Text.UTF8Encoding]::new($true))

  $checksumLines = @(
    "$(Get-FileHash -LiteralPath $portableExe -Algorithm SHA256 | Select-Object -ExpandProperty Hash)  M2Shelf.exe",
    "$(Get-FileHash -LiteralPath $portableUpdater -Algorithm SHA256 | Select-Object -ExpandProperty Hash)  M2ShelfUpdater.exe",
    "$(Get-FileHash -LiteralPath $portableMarker -Algorithm SHA256 | Select-Object -ExpandProperty Hash)  M2Shelf.portable.json",
    "$(Get-FileHash -LiteralPath $portableReadme -Algorithm SHA256 | Select-Object -ExpandProperty Hash)  README_zh-CN.txt"
  )
  $checksumPath = Join-Path $resolvedStage "SHA256SUMS.txt"
  foreach ($name in $workerPayloadFiles) { $checksumLines += "$(Get-FileHash -LiteralPath (Join-Path $resolvedStage $name) -Algorithm SHA256 | Select-Object -ExpandProperty Hash)  $name" }
  [System.IO.File]::WriteAllLines($checksumPath, $checksumLines, [System.Text.UTF8Encoding]::new($false))

  $requiredStageFiles = @(
    "M2Shelf.exe",
    "M2ShelfUpdater.exe",
    "M2Shelf.portable.json",
    "README_zh-CN.txt",
    "SHA256SUMS.txt",
    "M2ShelfMobi.exe",
    "M2ShelfMobi-source.zip",
    "THIRD-PARTY-NOTICES.txt"
  )
  $actualStageFiles = Get-ChildItem -LiteralPath $resolvedStage -File | Select-Object -ExpandProperty Name
  $missingStageFiles = $requiredStageFiles | Where-Object { $_ -notin $actualStageFiles }
  $unexpectedStageFiles = $actualStageFiles | Where-Object { $_ -notin $requiredStageFiles }
  if ($missingStageFiles.Count -gt 0 -or $unexpectedStageFiles.Count -gt 0) {
    throw "Portable stage contents are invalid. Missing: $($missingStageFiles -join ', '); unexpected: $($unexpectedStageFiles -join ', ')."
  }

  if (Test-Path -LiteralPath $archivePath) { Remove-Item -LiteralPath $archivePath -Force }
  Compress-Archive -Path (Join-Path $resolvedStage "*") -DestinationPath $archivePath -CompressionLevel Optimal
  $archiveHash = Get-FileHash -LiteralPath $archivePath -Algorithm SHA256
  [System.IO.File]::WriteAllText($archiveHashPath, "$($archiveHash.Hash)  $archiveName`r`n", [System.Text.UTF8Encoding]::new($false))
} finally {
  if (Test-Path -LiteralPath $resolvedStage) { Remove-Item -LiteralPath $resolvedStage -Recurse -Force }
}

Write-Output "Portable archive: $archivePath"
Write-Output "SHA-256: $($archiveHash.Hash)"
Write-Output "Updater SHA-256: $($sourceUpdaterHash.Hash)"
