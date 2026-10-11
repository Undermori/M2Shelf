#Requires -Version 7.2

[CmdletBinding()]
param(
  [string]$UsbRoot = "",
  [string]$CandidateDirectory = "",
  [string]$ToolPath = "",
  [string]$NotesPath = "",
  [string]$OutputDirectory = "",
  [string]$PublishedAt = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

if (-not [System.OperatingSystem]::IsWindows()) {
  throw "USB release signing is supported only on Windows."
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$configPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$publicKeyPath = Join-Path $repoRoot "src-tauri\update-public-key.txt"
if (-not (Test-Path -LiteralPath $configPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $publicKeyPath -PathType Leaf)) {
  throw "Repository update configuration is incomplete."
}

$config = Get-Content -LiteralPath $configPath -Raw -Encoding UTF8 | ConvertFrom-Json
$version = [string]$config.version
if ($version -notmatch '^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$') {
  throw "Tauri version must be a stable canonical semantic version."
}

$portableName = "M2Shelf-Portable-$version-x64.zip"
$installerName = "M2Shelf-Setup-$version-x64.exe"
$provenanceName = "candidate-provenance.json"
$bundleRoot = Join-Path $repoRoot "bundle"

function Get-CanonicalExistingFile {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Label
  )

  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "$Label was not found."
  }
  $item = Get-Item -LiteralPath $Path -Force
  if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "$Label must be a plain file."
  }
  return $item.FullName
}

function Get-CanonicalExistingDirectory {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Label
  )

  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    throw "$Label was not found."
  }
  $item = Get-Item -LiteralPath $Path -Force
  if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "$Label must be a plain directory."
  }
  return $item.FullName
}

function Test-UnsignedCandidateDirectory {
  param([Parameter(Mandatory = $true)][string]$Path)

  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    return $false
  }
  if (-not (Test-Path -LiteralPath (Join-Path $Path $portableName) -PathType Leaf) -or
      -not (Test-Path -LiteralPath (Join-Path $Path $installerName) -PathType Leaf) -or
      -not (Test-Path -LiteralPath (Join-Path $Path $provenanceName) -PathType Leaf)) {
    return $false
  }
  if ((Test-Path -LiteralPath (Join-Path $Path "latest.json")) -or
      (Test-Path -LiteralPath (Join-Path $Path "$portableName.sig")) -or
      (Test-Path -LiteralPath (Join-Path $Path "$installerName.sig"))) {
    return $false
  }
  return $true
}

function Resolve-CandidateDirectory {
  if (-not [string]::IsNullOrWhiteSpace($CandidateDirectory)) {
    $resolved = Get-CanonicalExistingDirectory -Path ([System.IO.Path]::GetFullPath($CandidateDirectory)) -Label "CandidateDirectory"
    if (-not (Test-UnsignedCandidateDirectory -Path $resolved)) {
      throw "CandidateDirectory must contain the exact current-version Portable, NSIS, and candidate-provenance files."
    }
    return $resolved
  }

  if (-not (Test-Path -LiteralPath $bundleRoot -PathType Container)) {
    throw "No default release candidate was found. Pass -CandidateDirectory explicitly."
  }
  $matches = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
  foreach ($portable in @(Get-ChildItem -LiteralPath $bundleRoot -Filter $portableName -File -Recurse -Depth 4 -Force -ErrorAction SilentlyContinue)) {
    $directory = $portable.Directory.FullName
    if (Test-UnsignedCandidateDirectory -Path $directory) {
      [void]$matches.Add($directory)
    }
  }
  if ($matches.Count -ne 1) {
    throw "Expected exactly one unsigned current-version candidate directory, but found $($matches.Count). Pass -CandidateDirectory explicitly."
  }
  return @($matches)[0]
}

function Resolve-NotesPath {
  param([Parameter(Mandatory = $true)][string]$ResolvedCandidateDirectory)

  if (-not [string]::IsNullOrWhiteSpace($NotesPath)) {
    return Get-CanonicalExistingFile -Path ([System.IO.Path]::GetFullPath($NotesPath)) -Label "NotesPath"
  }

  $releaseDirectory = Join-Path $bundleRoot "release-v$version"
  $possiblePaths = @(
    (Join-Path $ResolvedCandidateDirectory "release-notes.json"),
    (Join-Path $ResolvedCandidateDirectory "update-notes.json"),
    (Join-Path $releaseDirectory "release-notes.json"),
    (Join-Path $releaseDirectory "update-notes.json")
  )
  $matches = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
  foreach ($path in $possiblePaths) {
    if (Test-Path -LiteralPath $path -PathType Leaf) {
      [void]$matches.Add((Get-CanonicalExistingFile -Path $path -Label "Release notes"))
    }
  }
  if ($matches.Count -ne 1) {
    throw "Expected exactly one release-notes JSON file, but found $($matches.Count). Pass -NotesPath explicitly."
  }
  return @($matches)[0]
}

function Resolve-KeyPath {
  $relativeKeyPath = "M2Shelf-Production-Key\encrypted-private-key.m2key"
  $matches = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

  if (-not [string]::IsNullOrWhiteSpace($UsbRoot)) {
    $normalizedUsbRoot = if ($UsbRoot.Trim() -match '^([A-Za-z])(?::[\\/]?)?$') {
      $Matches[1].ToUpperInvariant() + ':\'
    } else {
      $UsbRoot
    }
    $root = [System.IO.Path]::GetFullPath($normalizedUsbRoot)
    $keyCandidate = Join-Path $root $relativeKeyPath
    if (Test-Path -LiteralPath $keyCandidate -PathType Leaf) {
      [void]$matches.Add((Get-CanonicalExistingFile -Path $keyCandidate -Label "USB encrypted private key"))
    }
  } else {
    foreach ($drive in @([System.IO.DriveInfo]::GetDrives())) {
      try {
        if (-not $drive.IsReady -or $drive.DriveType -ne [System.IO.DriveType]::Removable) {
          continue
        }
        $keyCandidate = Join-Path $drive.RootDirectory.FullName $relativeKeyPath
        if (Test-Path -LiteralPath $keyCandidate -PathType Leaf -ErrorAction SilentlyContinue) {
          [void]$matches.Add((Get-CanonicalExistingFile -Path $keyCandidate -Label "USB encrypted private key"))
        }
      } catch {
        continue
      }
    }
  }

  if ($matches.Count -ne 1) {
    throw "Expected exactly one mounted M2Shelf production key, but found $($matches.Count). For a USB reported as Fixed by Windows, pass its drive explicitly (for example -UsbRoot H:)."
  }
  $resolvedKey = @($matches)[0]
  $repoPrefix = $repoRoot.TrimEnd(
    [System.IO.Path]::DirectorySeparatorChar,
    [System.IO.Path]::AltDirectorySeparatorChar
  ) + [System.IO.Path]::DirectorySeparatorChar
  if ($resolvedKey.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "The encrypted production key must remain outside the source repository."
  }
  return $resolvedKey
}

$resolvedCandidateDirectory = Resolve-CandidateDirectory
$resolvedNotesPath = Resolve-NotesPath -ResolvedCandidateDirectory $resolvedCandidateDirectory
$resolvedProvenancePath = Get-CanonicalExistingFile -Path (Join-Path $resolvedCandidateDirectory $provenanceName) -Label "Candidate provenance"
$resolvedPublicKeyPath = Get-CanonicalExistingFile -Path $publicKeyPath -Label "Updater public key"

$resolvedToolPath = if ([string]::IsNullOrWhiteSpace($ToolPath)) {
  Get-CanonicalExistingFile `
    -Path (Join-Path $bundleRoot "portable-key-tool-v$version\M2ShelfPortableKeyTool.exe") `
    -Label "Portable key tool (run scripts/build_portable_key_tool.ps1 first)"
} else {
  Get-CanonicalExistingFile -Path ([System.IO.Path]::GetFullPath($ToolPath)) -Label "ToolPath"
}

$resolvedKeyPath = Resolve-KeyPath
$resolvedOutputDirectory = if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
  Join-Path $bundleRoot "release-v$version"
} else {
  [System.IO.Path]::GetFullPath($OutputDirectory)
}
if (Test-Path -LiteralPath $resolvedOutputDirectory) {
  [void](Get-CanonicalExistingDirectory -Path $resolvedOutputDirectory -Label "OutputDirectory")
} else {
  $outputDirectoryItem = New-Item -ItemType Directory -Path $resolvedOutputDirectory -ErrorAction Stop
  if (($outputDirectoryItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "OutputDirectory must be a plain directory."
  }
  $resolvedOutputDirectory = $outputDirectoryItem.FullName
}

$arguments = [System.Collections.Generic.List[string]]::new()
foreach ($argument in @(
    "sign-release",
    "--key", $resolvedKeyPath,
    "--public-key", $resolvedPublicKeyPath,
    "--version", $version,
    "--candidate-directory", $resolvedCandidateDirectory,
    "--notes", $resolvedNotesPath,
    "--provenance", $resolvedProvenancePath,
    "--output-directory", $resolvedOutputDirectory
  )) {
  $arguments.Add($argument)
}
if (-not [string]::IsNullOrWhiteSpace($PublishedAt)) {
  $arguments.Add("--published-at")
  $arguments.Add($PublishedAt)
}

Write-Output "Signing M2Shelf v$version. Enter the USB key password when prompted."
& $resolvedToolPath @arguments
if ($LASTEXITCODE -ne 0) {
  throw "Portable key tool failed with exit code $LASTEXITCODE."
}

$signedDirectory = Join-Path $resolvedOutputDirectory "M2Shelf-v$version-SIGNED-RETURN"
$returnArchive = Join-Path $resolvedOutputDirectory "M2Shelf-v$version-SIGNED-RETURN.zip"
$returnHash = "$returnArchive.sha256"
$expectedNames = @(
  $portableName,
  "$portableName.sha256",
  "$portableName.sig",
  $installerName,
  "$installerName.sha256",
  "$installerName.sig",
  "latest.json",
  $provenanceName
) | Sort-Object
if (-not (Test-Path -LiteralPath $signedDirectory -PathType Container)) {
  throw "Portable key tool did not produce the signed return directory."
}
$actualItems = @(Get-ChildItem -LiteralPath $signedDirectory -Force)
if (@($actualItems | Where-Object { $_.PSIsContainer }).Count -ne 0) {
  throw "Signed return directory must not contain subdirectories."
}
$actualNames = @($actualItems | Where-Object { -not $_.PSIsContainer } | ForEach-Object Name | Sort-Object)
if (($actualNames -join "`n") -cne ($expectedNames -join "`n")) {
  throw "Signed return directory does not contain the exact eight Release files."
}
[void](Get-CanonicalExistingFile -Path $returnArchive -Label "SIGNED-RETURN archive")
[void](Get-CanonicalExistingFile -Path $returnHash -Label "SIGNED-RETURN SHA-256 sidecar")
$expectedArchiveHash = (Get-FileHash -LiteralPath $returnArchive -Algorithm SHA256).Hash.ToLowerInvariant()
$sidecarLine = (Get-Content -LiteralPath $returnHash -Raw -Encoding UTF8).Trim()
if ($sidecarLine -cne "$expectedArchiveHash  $([System.IO.Path]::GetFileName($returnArchive))") {
  throw "SIGNED-RETURN SHA-256 sidecar does not match the archive."
}

Write-Output "Signing and verification completed. Remove the key USB before publishing."
Write-Output "SIGNED-RETURN: $returnArchive"
Write-Output "SHA-256: $expectedArchiveHash"
Write-Output "Checksum file: $returnHash"
