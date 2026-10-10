#Requires -Version 7.2

[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$CandidateDirectory,
  [Parameter(Mandatory = $true)][string]$VerifierPath,
  [Parameter(Mandatory = $true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$TrustedVerifierSha256,
  [Parameter(Mandatory = $true)][string]$ReleaseNotesPath,
  [switch]$Publish
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

$repository = "Undermori/M2Shelf"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

function Assert-ExactProperties {
  param(
    [Parameter(Mandatory = $true)]$Object,
    [Parameter(Mandatory = $true)][string[]]$Expected,
    [Parameter(Mandatory = $true)][string]$Label
  )

  if ($null -eq $Object -or $Object -isnot [System.Management.Automation.PSCustomObject]) {
    throw "$Label must be a JSON object."
  }
  $actual = @($Object.PSObject.Properties.Name | Sort-Object -CaseSensitive)
  $expectedSorted = @($Expected | Sort-Object -CaseSensitive)
  if (($actual -join "`n") -cne ($expectedSorted -join "`n")) {
    throw "$Label does not match the frozen schema."
  }
}

function Read-JsonFile {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][long]$MaximumBytes,
    [Parameter(Mandatory = $true)][string]$Label
  )

  $item = Get-Item -LiteralPath $Path -Force
  if ($item.Length -lt 2 -or $item.Length -gt $MaximumBytes) {
    throw "$Label has an invalid size."
  }
  try {
    return Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json -Depth 16
  } catch {
    throw "$Label is not valid JSON."
  }
}

function Assert-JsonPositiveInteger {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string]$Label
  )

  if ($Value -isnot [long] -or $Value -le 0) {
    throw "$Label must be a positive JSON integer."
  }
}

function Invoke-GitText {
  param(
    [Parameter(Mandatory = $true)][string[]]$Arguments,
    [Parameter(Mandatory = $true)][string]$Label
  )

  $output = @(& git -C $repoRoot @Arguments 2>&1)
  $exitCode = $LASTEXITCODE
  if ($exitCode -ne 0) {
    throw "Git could not $Label."
  }
  return (($output | ForEach-Object { [string]$_ }) -join "`n").Trim()
}

function Assert-HashSidecar {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$ExpectedFileName,
    [Parameter(Mandatory = $true)][string]$ExpectedSha256
  )

  $text = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
  $match = [System.Text.RegularExpressions.Regex]::Match(
    $text,
    '\A(?<hash>[0-9A-Fa-f]{64}) {2}(?<file>[^\r\n]+)\r?\n\z',
    [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
  )
  if (-not $match.Success -or
      $match.Groups["file"].Value -cne $ExpectedFileName -or
      $match.Groups["hash"].Value.ToLowerInvariant() -cne $ExpectedSha256) {
    throw "$([System.IO.Path]::GetFileName($Path)) does not exactly describe its release asset."
  }
}

function Read-SignatureSidecar {
  param([Parameter(Mandatory = $true)][string]$Path)

  $text = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
  $match = [System.Text.RegularExpressions.Regex]::Match(
    $text,
    '\A(?<signature>[A-Za-z0-9+/]{86}==)\r?\n\z',
    [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
  )
  if (-not $match.Success) {
    throw "$([System.IO.Path]::GetFileName($Path)) is not a canonical Ed25519 signature sidecar."
  }
  $signature = $match.Groups["signature"].Value
  try {
    $signatureBytes = [System.Convert]::FromBase64String($signature)
  } catch {
    throw "$([System.IO.Path]::GetFileName($Path)) is not standard Base64."
  }
  try {
    if ($signatureBytes.Length -ne 64 -or [System.Convert]::ToBase64String($signatureBytes) -cne $signature) {
      throw "$([System.IO.Path]::GetFileName($Path)) must contain exactly 64 canonical Ed25519 bytes."
    }
  } finally {
    [System.Array]::Clear($signatureBytes, 0, $signatureBytes.Length)
  }
  return $signature
}

function Invoke-ArtifactVerifier {
  param(
    [Parameter(Mandatory = $true)][string]$Platform,
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Signature,
    [Parameter(Mandatory = $true)][string]$ExpectedFileName,
    [Parameter(Mandatory = $true)][long]$ExpectedSize,
    [Parameter(Mandatory = $true)][string]$ExpectedSha256
  )

  $verifierHashBefore = (Get-FileHash -LiteralPath $resolvedVerifier -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($verifierHashBefore -cne $TrustedVerifierSha256.ToLowerInvariant()) {
    throw "The verifier helper changed before cryptographic signature verification."
  }
  $stdout = @(& $resolvedVerifier verify --version $version --platform $Platform --file $Path --signature $Signature)
  if ($LASTEXITCODE -ne 0) {
    throw "M2ShelfUpdater rejected the Ed25519 signature for $ExpectedFileName."
  }
  $verifierHashAfter = (Get-FileHash -LiteralPath $resolvedVerifier -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($verifierHashAfter -cne $verifierHashBefore) {
    throw "The verifier helper changed during cryptographic signature verification."
  }

  $lines = @($stdout | ForEach-Object { [string]$_ } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
  if ($lines.Count -ne 1) {
    throw "M2ShelfUpdater verify must write exactly one JSON object to stdout."
  }
  try {
    $verified = $lines[0] | ConvertFrom-Json -Depth 8
  } catch {
    throw "M2ShelfUpdater verify returned invalid JSON."
  }
  Assert-ExactProperties -Object $verified -Expected @("fileName", "sha256", "signature", "size") -Label "M2ShelfUpdater verify result"
  if ([string]$verified.fileName -cne $ExpectedFileName -or
      $verified.size -isnot [long] -or [long]$verified.size -ne $ExpectedSize -or
      [string]$verified.sha256 -cne $ExpectedSha256 -or
      [string]$verified.signature -cne $Signature) {
    throw "M2ShelfUpdater verify result does not exactly match the signed release asset."
  }
}

function Invoke-GhJson {
  param(
    [Parameter(Mandatory = $true)][string[]]$Arguments,
    [Parameter(Mandatory = $true)][string]$Label
  )

  $output = @(& gh @Arguments 2>&1)
  $exitCode = $LASTEXITCODE
  if ($exitCode -ne 0) {
    throw "GitHub CLI could not $Label."
  }
  try {
    return (($output | ForEach-Object { [string]$_ }) -join "`n") | ConvertFrom-Json -Depth 16
  } catch {
    throw "GitHub returned invalid JSON while attempting to $Label."
  }
}

function Assert-RemoteTagMatches {
  param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [Parameter(Mandatory = $true)][string]$ExpectedTagObjectSha
  )

  $remoteRef = Invoke-GhJson -Arguments @(
    "api",
    "--method", "GET",
    "repos/$repository/git/ref/tags/$Tag"
  ) -Label "read the remote release tag"
  if ([string]$remoteRef.ref -cne "refs/tags/$Tag" -or
      [string]$remoteRef.object.sha -cne $ExpectedTagObjectSha) {
    throw "The remote release tag does not match the locally verified immutable tag object."
  }
}

function Get-RemoteRelease {
  param([Parameter(Mandatory = $true)][string]$Tag)

  return Invoke-GhJson -Arguments @(
    "api",
    "--method", "GET",
    "repos/$repository/releases/tags/$Tag"
  ) -Label "read the draft release"
}

if (-not (Get-Command git -CommandType Application -ErrorAction SilentlyContinue)) {
  throw "git was not found."
}
$repositoryRoot = Invoke-GitText -Arguments @("rev-parse", "--show-toplevel") -Label "locate the repository"
if ([System.IO.Path]::GetFullPath($repositoryRoot) -cne $repoRoot) {
  throw "This script must run from its own M2Shelf repository worktree."
}
$status = Invoke-GitText -Arguments @("status", "--porcelain=v1", "--untracked-files=normal") -Label "inspect the worktree"
if (-not [string]::IsNullOrWhiteSpace($status)) {
  throw "The worktree must be clean before publishing a release."
}

$config = Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri\tauri.conf.json") -Raw -Encoding UTF8 | ConvertFrom-Json
$version = [string]$config.version
if ($version -notmatch '^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$') {
  throw "Tauri version must be a stable canonical semantic version (major.minor.patch)."
}

$resolvedVerifier = [System.IO.Path]::GetFullPath($VerifierPath)
if (-not (Test-Path -LiteralPath $resolvedVerifier -PathType Leaf)) {
  throw "VerifierPath was not found."
}
$verifierItem = Get-Item -LiteralPath $resolvedVerifier -Force
if (($verifierItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
  throw "VerifierPath must not be a symbolic link or other reparse point."
}
$resolvedVerifier = $verifierItem.FullName
$actualVerifierSha256 = (Get-FileHash -LiteralPath $resolvedVerifier -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualVerifierSha256 -cne $TrustedVerifierSha256.ToLowerInvariant()) {
  throw "Verifier helper SHA-256 does not match the independently supplied fingerprint."
}
$publicKeyPath = Join-Path $repoRoot "src-tauri\update-public-key.txt"
$publicKeyText = (Get-Content -LiteralPath $publicKeyPath -Raw -Encoding UTF8).Trim()
try {
  $publicKeyBytes = [System.Convert]::FromBase64String($publicKeyText)
} catch {
  throw "Updater public key is invalid Base64."
}
if ($publicKeyBytes.Length -ne 32 -or [System.Convert]::ToBase64String($publicKeyBytes) -cne $publicKeyText) {
  throw "Updater public key must be canonical Base64 containing exactly 32 bytes."
}

$identityOutput = @(& $resolvedVerifier identity)
if ($LASTEXITCODE -ne 0) {
  throw "Trusted verifier identity check failed."
}
if ((Get-FileHash -LiteralPath $resolvedVerifier -Algorithm SHA256).Hash.ToLowerInvariant() -cne $actualVerifierSha256) {
  throw "The verifier helper changed during its identity check."
}
$identityLines = @($identityOutput | ForEach-Object { [string]$_ } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
if ($identityLines.Count -ne 1) {
  throw "Trusted verifier identity must be exactly one JSON object."
}
try {
  $identity = $identityLines[0] | ConvertFrom-Json -Depth 8
} catch {
  throw "Trusted verifier identity is invalid JSON."
}
Assert-ExactProperties -Object $identity -Expected @("appId", "publicKey", "schemaVersion", "version") -Label "trusted verifier identity"
if ($identity.schemaVersion -isnot [long] -or [long]$identity.schemaVersion -ne 1 -or
    [string]$identity.appId -cne "app.morimediashelf.desktop" -or
    [string]$identity.version -cne $version -or
    [string]$identity.publicKey -cne $publicKeyText) {
  throw "Trusted verifier identity, version, or embedded public key does not match this release."
}
$tag = "v$version"
$headCommit = (Invoke-GitText -Arguments @("rev-parse", "--verify", "HEAD") -Label "resolve HEAD").ToLowerInvariant()
if ($headCommit -notmatch '^[0-9a-f]{40}$') {
  throw "HEAD is not a full Git commit SHA."
}
$tagCommit = (Invoke-GitText -Arguments @("rev-parse", "--verify", "$tag^{commit}") -Label "resolve $tag to a commit").ToLowerInvariant()
if ($tagCommit -cne $headCommit) {
  throw "The current HEAD does not match the immutable release tag $tag."
}
$tagObjectSha = (Invoke-GitText -Arguments @("rev-parse", "--verify", "refs/tags/$tag") -Label "resolve the $tag tag object").ToLowerInvariant()
if ($tagObjectSha -notmatch '^[0-9a-f]{40}$') {
  throw "The release tag does not resolve to a full Git object SHA."
}

$candidatePath = [System.IO.Path]::GetFullPath($CandidateDirectory)
if (-not (Test-Path -LiteralPath $candidatePath -PathType Container)) {
  throw "CandidateDirectory was not found."
}
$candidatePath = (Resolve-Path -LiteralPath $candidatePath).ProviderPath
$portableName = "M2Shelf-Portable-$version-x64.zip"
$installerName = "M2Shelf-Setup-$version-x64.exe"
$filePaths = [ordered]@{
  portable = (Join-Path $candidatePath $portableName)
  portableHash = (Join-Path $candidatePath "$portableName.sha256")
  portableSignature = (Join-Path $candidatePath "$portableName.sig")
  nsis = (Join-Path $candidatePath $installerName)
  nsisHash = (Join-Path $candidatePath "$installerName.sha256")
  nsisSignature = (Join-Path $candidatePath "$installerName.sig")
  manifest = (Join-Path $candidatePath "latest.json")
  provenance = (Join-Path $candidatePath "candidate-provenance.json")
}
$expectedInputFiles = [ordered]@{}
foreach ($entry in $filePaths.GetEnumerator()) {
  if (-not (Test-Path -LiteralPath $entry.Value -PathType Leaf)) {
    throw "Signed release input is incomplete: $([System.IO.Path]::GetFileName($entry.Value)) was not found."
  }
  $item = Get-Item -LiteralPath $entry.Value -Force
  if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Signed release inputs must not be symbolic links or other reparse points."
  }
  $expectedInputFiles[$entry.Key] = [pscustomobject]@{
    Path = $entry.Value
    Size = [long]$item.Length
    Sha256 = (Get-FileHash -LiteralPath $entry.Value -Algorithm SHA256).Hash.ToLowerInvariant()
  }
}

$provenance = Read-JsonFile -Path $filePaths.provenance -MaximumBytes 65536 -Label "candidate-provenance.json"
Assert-ExactProperties -Object $provenance -Expected @(
  "artifacts",
  "commitSha",
  "repository",
  "runAttempt",
  "runId",
  "schemaVersion",
  "sourceRef",
  "tag",
  "tagExists",
  "version"
) -Label "candidate-provenance.json"
Assert-ExactProperties -Object $provenance.artifacts -Expected @("nsis", "portable") -Label "candidate-provenance.json artifacts"
Assert-ExactProperties -Object $provenance.artifacts.portable -Expected @("fileName", "sha256", "size") -Label "portable provenance"
Assert-ExactProperties -Object $provenance.artifacts.nsis -Expected @("fileName", "sha256", "size") -Label "NSIS provenance"
if ($provenance.schemaVersion -isnot [long] -or $provenance.schemaVersion -ne 1 -or
    $provenance.tagExists -isnot [bool] -or -not $provenance.tagExists -or
    [string]$provenance.repository -cne $repository -or
    [string]$provenance.version -cne $version -or
    [string]$provenance.tag -cne $tag -or
    [string]$provenance.commitSha -cne $headCommit -or
    [string]$provenance.sourceRef -cne "refs/tags/$tag") {
  throw "Candidate provenance does not match this repository, version, tag, commit, and tag-triggered source ref."
}
if ($provenance.runId -isnot [string] -or [string]$provenance.runId -notmatch '^[1-9]\d*$' -or
    $provenance.runAttempt -isnot [string] -or [string]$provenance.runAttempt -notmatch '^[1-9]\d*$') {
  throw "Candidate provenance must contain canonical GitHub Actions run identifiers."
}

$artifactContracts = @(
  [pscustomobject]@{
    Label = "portable"
    Provenance = $provenance.artifacts.portable
    Path = $filePaths.portable
    FileName = $portableName
    HashPath = $filePaths.portableHash
    SignaturePath = $filePaths.portableSignature
    Platform = "windows-x64-portable"
  },
  [pscustomobject]@{
    Label = "NSIS"
    Provenance = $provenance.artifacts.nsis
    Path = $filePaths.nsis
    FileName = $installerName
    HashPath = $filePaths.nsisHash
    SignaturePath = $filePaths.nsisSignature
    Platform = "windows-x64-nsis"
  }
)

$manifest = Read-JsonFile -Path $filePaths.manifest -MaximumBytes 262144 -Label "latest.json"
Assert-ExactProperties -Object $manifest -Expected @("notes", "platforms", "publishedAt", "schemaVersion", "version") -Label "latest.json"
Assert-ExactProperties -Object $manifest.notes -Expected @("en-US", "ja-JP", "ko-KR", "zh-CN") -Label "latest.json notes"
Assert-ExactProperties -Object $manifest.platforms -Expected @("windows-x64-nsis", "windows-x64-portable") -Label "latest.json platforms"
if ($manifest.schemaVersion -isnot [long] -or $manifest.schemaVersion -ne 1 -or [string]$manifest.version -cne $version) {
  throw "latest.json does not match the frozen schema and current version."
}
if ([string]$manifest.publishedAt -notmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$') {
  throw "latest.json publishedAt must be a canonical UTC timestamp."
}
try {
  [void][System.DateTimeOffset]::ParseExact(
    [string]$manifest.publishedAt,
    "yyyy-MM-dd'T'HH:mm:ss'Z'",
    [System.Globalization.CultureInfo]::InvariantCulture,
    [System.Globalization.DateTimeStyles]::AssumeUniversal
  )
} catch {
  throw "latest.json publishedAt is not a real UTC timestamp."
}
foreach ($locale in @("zh-CN", "en-US", "ja-JP", "ko-KR")) {
  if ($manifest.notes.$locale -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$manifest.notes.$locale)) {
    throw "latest.json release note $locale must be a non-empty string."
  }
}

foreach ($contract in $artifactContracts) {
  Assert-JsonPositiveInteger -Value $contract.Provenance.size -Label "$($contract.Label) provenance size"
  $provenanceSha256 = [string]$contract.Provenance.sha256
  if ([string]$contract.Provenance.fileName -cne $contract.FileName -or $provenanceSha256 -notmatch '^[0-9a-f]{64}$') {
    throw "$($contract.Label) provenance does not contain the frozen filename and lowercase SHA-256."
  }

  $file = Get-Item -LiteralPath $contract.Path
  $actualSha256 = (Get-FileHash -LiteralPath $contract.Path -Algorithm SHA256).Hash.ToLowerInvariant()
  if ([long]$file.Length -ne [long]$contract.Provenance.size -or $actualSha256 -cne $provenanceSha256) {
    throw "$($contract.Label) asset does not match candidate provenance."
  }
  Assert-HashSidecar -Path $contract.HashPath -ExpectedFileName $contract.FileName -ExpectedSha256 $actualSha256
  $sidecarSignature = Read-SignatureSidecar -Path $contract.SignaturePath

  $manifestAsset = $manifest.platforms.($contract.Platform)
  Assert-ExactProperties -Object $manifestAsset -Expected @("fileName", "sha256", "signature", "size", "url") -Label "$($contract.Label) manifest asset"
  Assert-JsonPositiveInteger -Value $manifestAsset.size -Label "$($contract.Label) manifest size"
  $expectedUrl = "https://github.com/$repository/releases/download/$tag/$($contract.FileName)"
  if ([string]$manifestAsset.url -cne $expectedUrl -or
      [string]$manifestAsset.fileName -cne $contract.FileName -or
      [long]$manifestAsset.size -ne [long]$file.Length -or
      [string]$manifestAsset.sha256 -cne $actualSha256 -or
      [string]$manifestAsset.signature -cne $sidecarSignature) {
    throw "$($contract.Label) manifest record does not match its asset, hash, signature sidecar, and immutable release URL."
  }
  Invoke-ArtifactVerifier `
    -Platform $contract.Platform `
    -Path $contract.Path `
    -Signature $sidecarSignature `
    -ExpectedFileName $contract.FileName `
    -ExpectedSize ([long]$file.Length) `
    -ExpectedSha256 $actualSha256
}

$uploadPaths = @(
  $filePaths.portable,
  $filePaths.nsis,
  $filePaths.manifest
)
$expectedUploadFiles = [ordered]@{}
$expectedUploadNames = [System.Collections.Generic.List[string]]::new()
foreach ($path in $uploadPaths) {
  $name = [System.IO.Path]::GetFileName($path)
  if (@($expectedUploadNames) -ccontains $name) {
    throw "Release asset names must be unique."
  }
  $expectedUploadNames.Add($name)
  $expectedUploadFiles[$name] = [pscustomobject]@{
    Path = $path
    Size = [long](Get-Item -LiteralPath $path).Length
    Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
  }
}

$resolvedReleaseNotes = [System.IO.Path]::GetFullPath($ReleaseNotesPath)
if (-not (Test-Path -LiteralPath $resolvedReleaseNotes -PathType Leaf)) {
  throw "ReleaseNotesPath was not found."
}
$notesItem = Get-Item -LiteralPath $resolvedReleaseNotes -Force
if (($notesItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0 -or
    $notesItem.Length -lt 1 -or $notesItem.Length -gt 1048576) {
  throw "ReleaseNotesPath must be a regular non-empty file no larger than 1 MiB."
}

if (-not $Publish) {
  Write-Output "Signed release verification passed for $tag at $headCommit."
  Write-Output "No GitHub changes were made. Pass -Publish to create and publish the release."
  return
}

if (-not (Get-Command gh -CommandType Application -ErrorAction SilentlyContinue)) {
  throw "GitHub CLI (gh) was not found."
}
$authOutput = @(& gh auth status --hostname github.com 2>&1)
if ($LASTEXITCODE -ne 0) {
  throw "GitHub CLI is not authenticated for github.com."
}

$lockedFiles = [System.Collections.Generic.List[System.IO.FileStream]]::new()
try {
  # All eight inputs remain verified and locked; only the three client assets are public.
  $pathsToLock = @($filePaths.Values)
  $pathsToLock += $resolvedReleaseNotes
  foreach ($path in $pathsToLock) {
    $lockedFiles.Add([System.IO.File]::Open(
      $path,
      [System.IO.FileMode]::Open,
      [System.IO.FileAccess]::Read,
      [System.IO.FileShare]::Read
    ))
  }
  foreach ($entry in $expectedInputFiles.GetEnumerator()) {
    $lockedHash = (Get-FileHash -LiteralPath $entry.Value.Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($lockedHash -cne $entry.Value.Sha256 -or
        [long](Get-Item -LiteralPath $entry.Value.Path).Length -ne $entry.Value.Size) {
      throw "A signed release input changed while it was being locked for upload."
    }
  }

  Assert-RemoteTagMatches -Tag $tag -ExpectedTagObjectSha $tagObjectSha

  $releaseProbe = @(& gh api --method GET "repos/$repository/releases/tags/$tag" --include 2>&1)
  $releaseProbeExitCode = $LASTEXITCODE
  if ($releaseProbeExitCode -eq 0) {
    throw "A GitHub release already exists for $tag; immutable releases are never modified."
  }
  $releaseProbeText = ($releaseProbe | ForEach-Object { [string]$_ }) -join "`n"
  if ($releaseProbeText -notmatch '(?im)(?:^HTTP/\S+\s+404\b|\(HTTP 404\))') {
    throw "GitHub did not conclusively report that the release tag is unused."
  }

  $createArguments = @(
    "release", "create", $tag,
    "--repo", $repository,
    "--draft",
    "--verify-tag",
    "--title", "M²Shelf $version"
  )
  $createArguments += @("--notes-file", $resolvedReleaseNotes)
  $createOutput = @(& gh @createArguments 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "GitHub CLI failed to create the draft release."
  }

  $uploadOutput = @(& gh release upload $tag @uploadPaths --repo $repository 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "GitHub CLI failed to upload the complete signed release asset set; the release remains a draft."
  }

  $draft = Get-RemoteRelease -Tag $tag
  if ($draft.draft -isnot [bool] -or -not $draft.draft -or [string]$draft.tag_name -cne $tag) {
    throw "The newly created GitHub release is not the expected draft."
  }
  $remoteAssets = @($draft.assets)
  if ($remoteAssets.Count -ne $expectedUploadFiles.Count) {
    throw "The draft release does not contain exactly the expected signed asset set."
  }
  foreach ($remoteAsset in $remoteAssets) {
    $remoteName = [string]$remoteAsset.name
    if (-not (@($expectedUploadNames) -ccontains $remoteName)) {
      throw "The draft release contains an unexpected asset."
    }
    $expectedAsset = $expectedUploadFiles[$remoteName]
    if ($remoteAsset.size -isnot [long] -or [long]$remoteAsset.size -ne $expectedAsset.Size) {
      throw "The uploaded size does not match local input for $remoteName."
    }
    if ($null -eq $remoteAsset.digest -or
        [string]::IsNullOrWhiteSpace([string]$remoteAsset.digest) -or
        [string]$remoteAsset.digest -cne "sha256:$($expectedAsset.Sha256)") {
      throw "The uploaded digest does not match local input for $remoteName."
    }
  }

  Assert-RemoteTagMatches -Tag $tag -ExpectedTagObjectSha $tagObjectSha
  $publishOutput = @(& gh release edit $tag --repo $repository --draft=false 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "GitHub CLI could not publish the verified draft release."
  }
  $publishedRelease = Get-RemoteRelease -Tag $tag
  if ($publishedRelease.draft -isnot [bool] -or $publishedRelease.draft -or
      [string]$publishedRelease.tag_name -cne $tag) {
    throw "GitHub did not confirm the release as published."
  }
  Write-Output "Published immutable GitHub release $tag from $headCommit with $($expectedUploadFiles.Count) verified assets."
} finally {
  foreach ($stream in $lockedFiles) {
    $stream.Dispose()
  }
}
