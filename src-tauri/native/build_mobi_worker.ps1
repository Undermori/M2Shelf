$ErrorActionPreference = 'Stop'
$workerRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $workerRoot
try {
  if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) { throw 'Use an x64 Native Tools Command Prompt for Visual Studio.' }
  New-Item -ItemType Directory -Path out -Force | Out-Null
  Set-Content -LiteralPath out/worker_config.h -Value '#define PACKAGE_VERSION "0.12"' -Encoding ascii
  $sources = @('buffer','compression','debug','index','memory','meta','parse_rawml','read','structure','util','write','miniz') | ForEach-Object { "vendor/libmobi/src/$_.c" }
  $objects = @()
  foreach ($source in @($sources) + @('native/mobi_worker.c')) {
    $object = 'out/' + [IO.Path]::GetFileNameWithoutExtension($source) + '.obj'
    & cl.exe /nologo /O2 /MT /Ivendor/libmobi/src /Iout /FIworker_config.h /DUSE_MINIZ /D_CRT_SECURE_NO_WARNINGS /DMINIZ_NO_STDIO /DMINIZ_NO_ZLIB_COMPATIBLE_NAMES /DMINIZ_NO_TIME /DMINIZ_NO_ARCHIVE_APIS /DMINIZ_NO_ARCHIVE_WRITING_APIS /c $source "/Fo$object"
    if ($LASTEXITCODE -ne 0) { throw "Compilation failed: $source" }
    $objects += $object
  }
  & cl.exe /nologo $objects /Feout/M2ShelfMobi.exe /link /Brepro
  if ($LASTEXITCODE -ne 0) { throw 'Worker link failed' }
} finally { Pop-Location }
