param(
  [ValidateSet("none", "nsis")]
  [string]$Bundles = "nsis",
  [string]$TargetDirectory
)

$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
$version = [string]$tauriConfig.version
$productName = [string]$tauriConfig.productName
if ($version -notmatch '^\d+\.\d+\.\d+$' -or [string]::IsNullOrWhiteSpace($productName)) {
  throw "Tauri product name or stable version is invalid."
}
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
$separator = [char]0x1F
$remapArguments = [System.Collections.Generic.List[string]]::new()

if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
  $userProfile = [System.IO.Path]::GetFullPath($env:USERPROFILE).TrimEnd('\')
  $remapArguments.Add("--remap-path-prefix=$userProfile=<USERPROFILE>")
}
$remapArguments.Add("--remap-path-prefix=$($repoRoot.TrimEnd('\'))=<SOURCE>")

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
    foreach ($variant in @($privatePath, $privatePath.Replace('\', '/')) | Select-Object -Unique) {
      if ($utf8.IndexOf($variant, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or
          $utf16.IndexOf($variant, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
        throw "$([System.IO.Path]::GetFileName($Path)) contains a private builder path."
      }
    }
  }
}

function Assert-PlainFile {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Label
  )

  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "$Label was not produced: $Path"
  }
  $item = Get-Item -LiteralPath $Path -Force
  if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "$Label must be a plain file, not a reparse point."
  }
  return $item
}

function Assert-X64Pe {
  param([Parameter(Mandatory = $true)][string]$Path)

  $stream = [System.IO.File]::OpenRead($Path)
  $reader = [System.IO.BinaryReader]::new($stream)
  try {
    if ($stream.Length -lt 0x40) { throw "Release executable is too small." }
    $stream.Position = 0x3C
    $peOffset = $reader.ReadInt32()
    if ($peOffset -lt 0 -or ($peOffset + 6) -gt $stream.Length) {
      throw "Release executable PE header offset is invalid."
    }
    $stream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) {
      throw "Release executable is not an x64 PE."
    }
  } finally {
    $reader.Dispose()
    $stream.Dispose()
  }
}

function Assert-PlainDestinationOrMissing {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Label
  )

  $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  if ($null -ne $item) {
    Assert-PlainFile -Path $Path -Label $Label | Out-Null
  } elseif (Test-Path -LiteralPath $Path) {
    throw "$Label exists but cannot be validated as a plain file."
  }
}

$previousEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
$previousTargetDirectory = $env:CARGO_TARGET_DIR
$previousBuildDate = $env:M2SHELF_BUILD_DATE
$buildDate = if ([string]::IsNullOrWhiteSpace($previousBuildDate)) {
  [DateTime]::Now.ToString("yyyy-MM-dd", [Globalization.CultureInfo]::InvariantCulture)
} else {
  $previousBuildDate
}
$parsedBuildDate = [DateTime]::MinValue
if (-not [DateTime]::TryParseExact($buildDate, "yyyy-MM-dd", [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]::None, [ref]$parsedBuildDate)) {
  throw "M2SHELF_BUILD_DATE must be a valid yyyy-MM-dd calendar date."
}
$env:M2SHELF_BUILD_DATE = $buildDate
$buildTargetDirectory = if (-not [string]::IsNullOrWhiteSpace($TargetDirectory)) {
  if ([System.IO.Path]::IsPathRooted($TargetDirectory)) { [System.IO.Path]::GetFullPath($TargetDirectory) }
  else { [System.IO.Path]::GetFullPath((Join-Path $repoRoot $TargetDirectory)) }
} elseif (-not [string]::IsNullOrWhiteSpace($previousTargetDirectory)) {
  if ([System.IO.Path]::IsPathRooted($previousTargetDirectory)) { [System.IO.Path]::GetFullPath($previousTargetDirectory) }
  else { [System.IO.Path]::GetFullPath((Join-Path $repoRoot $previousTargetDirectory)) }
} else {
  Join-Path $repoRoot "src-tauri\target"
}
$env:CARGO_TARGET_DIR = $buildTargetDirectory
$encodedRemaps = $remapArguments -join $separator
$env:CARGO_ENCODED_RUSTFLAGS = if ([string]::IsNullOrWhiteSpace($previousEncodedFlags)) {
  $encodedRemaps
} else {
  "$previousEncodedFlags$separator$encodedRemaps"
}

Push-Location $repoRoot
try {
  if ($Bundles -eq "none") {
    & npm run tauri build -- --no-bundle
  } else {
    & npm run tauri build -- --bundles $Bundles
  }
  if ($LASTEXITCODE -ne 0) {
    throw "Tauri release build failed with exit code $LASTEXITCODE"
  }
  # The updater is a separate PE and must receive the same path remapping as the Tauri binary.
  # Build it after Tauri's beforeBuild step updates dist, so freshness validation covers every
  # frontend and Rust input without accidentally accepting an older helper.
  & cargo build --manifest-path src-tauri/Cargo.toml --release --locked --bin M2ShelfUpdater
  if ($LASTEXITCODE -ne 0) {
    throw "M2ShelfUpdater release build failed with exit code $LASTEXITCODE"
  }
  $releaseExecutable = Join-Path $buildTargetDirectory "release\m2shelf.exe"
  $releaseUpdater = Join-Path $buildTargetDirectory "release\M2ShelfUpdater.exe"
  Assert-PlainFile -Path $releaseExecutable -Label "M2Shelf release executable" | Out-Null
  Assert-PlainFile -Path $releaseUpdater -Label "M2ShelfUpdater release executable" | Out-Null
  Assert-X64Pe -Path $releaseExecutable
  Assert-X64Pe -Path $releaseUpdater
  Assert-NoPrivateBuildPath -Path $releaseExecutable
  Assert-NoPrivateBuildPath -Path $releaseUpdater
  $releaseMobi = Join-Path $buildTargetDirectory 'release\M2ShelfMobi.exe'
  Assert-PlainFile -Path $releaseMobi -Label 'M2ShelfMobi worker' | Out-Null
  Assert-X64Pe -Path $releaseMobi
  Assert-NoPrivateBuildPath -Path $releaseMobi
  if ($Bundles -eq "nsis") {
    $installerName = "${productName}_${version}_x64-setup.exe"
    $installerPath = Join-Path $buildTargetDirectory "release\bundle\nsis\$installerName"
    $installer = Assert-PlainFile -Path $installerPath -Label "x64 NSIS installer"
    if ($installer.VersionInfo.ProductName -cne $productName -or
        $installer.VersionInfo.ProductVersion -cne $version) {
      throw "NSIS installer product metadata does not match $productName $version."
    }
    Assert-NoPrivateBuildPath -Path $installer.FullName

    $bundleDirectory = Join-Path $repoRoot "bundle"
    [System.IO.Directory]::CreateDirectory($bundleDirectory) | Out-Null
    $bundleItem = Get-Item -LiteralPath $bundleDirectory -Force
    if (-not $bundleItem.PSIsContainer -or
        ($bundleItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Release bundle directory must be a plain application-owned directory."
    }
    $resolvedBundle = [System.IO.Path]::GetFullPath($bundleDirectory).TrimEnd('\')
    $stableInstaller = [System.IO.Path]::GetFullPath(
      (Join-Path $resolvedBundle "M2Shelf-Setup-$version-x64.exe")
    )
    $stableChecksum = "$stableInstaller.sha256"
    if ([System.IO.Path]::GetDirectoryName($stableInstaller) -cne $resolvedBundle) {
      throw "Stable NSIS destination escaped the application-owned bundle directory."
    }
    Assert-PlainDestinationOrMissing -Path $stableInstaller -Label "stable NSIS installer"
    Assert-PlainDestinationOrMissing -Path $stableChecksum -Label "stable NSIS checksum"
    if (Test-Path -LiteralPath $stableInstaller -PathType Leaf) {
      Remove-Item -LiteralPath $stableInstaller -Force
    }
    if (Test-Path -LiteralPath $stableChecksum -PathType Leaf) {
      Remove-Item -LiteralPath $stableChecksum -Force
    }
    Copy-Item -LiteralPath $installer.FullName -Destination $stableInstaller
    Assert-PlainFile -Path $stableInstaller -Label "stable NSIS installer" | Out-Null
    Assert-NoPrivateBuildPath -Path $stableInstaller
    $installerHash = (Get-FileHash -LiteralPath $stableInstaller -Algorithm SHA256).Hash
    [System.IO.File]::WriteAllText(
      $stableChecksum,
      "$installerHash  $([System.IO.Path]::GetFileName($stableInstaller))`r`n",
      [System.Text.UTF8Encoding]::new($false)
    )
    Assert-PlainFile -Path $stableChecksum -Label "stable NSIS checksum" | Out-Null
    Write-Output "NSIS installer: $stableInstaller"
    Write-Output "SHA-256: $installerHash"
  }
} finally {
  Pop-Location
  if ($null -eq $previousEncodedFlags) {
    Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
  } else {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags
  }
  $env:CARGO_TARGET_DIR = $previousTargetDirectory
  if ($null -eq $previousBuildDate) {
    Remove-Item Env:M2SHELF_BUILD_DATE -ErrorAction SilentlyContinue
  } else {
    $env:M2SHELF_BUILD_DATE = $previousBuildDate
  }
}
