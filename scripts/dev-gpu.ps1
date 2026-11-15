# Run the app in development mode with GPU support, or build the Windows installer:
#   scripts\dev-gpu.ps1          (dev: on a fresh copy of your data, never the real one; see below)
#   scripts\dev-gpu.ps1 build    (installer in src-tauri\target\release\bundle\nsis)
# Needs the CUDA Toolkit (tested with 13.5) and the Visual Studio C++ build tools.
# The first build compiles candle's CUDA kernels (a few minutes). The installer does NOT
# contain NVIDIA's libraries: users download them in Settings when they have an NVIDIA card.

$mode = if ($args.Count -gt 0) { $args[0] } else { "dev" }

$cuda = $env:CUDA_PATH
if (-not $cuda) { $cuda = (Get-ChildItem "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA" | Sort-Object Name | Select-Object -Last 1).FullName }
if (-not (Test-Path "$cuda\bin\nvcc.exe")) { throw "CUDA Toolkit not found. Install it, or set CUDA_PATH." }
$env:CUDA_PATH = $cuda
$env:PATH = "$cuda\bin;$cuda\bin\x64;$env:PATH"

# cudarc 0.19 knows CUDA up to 13.4; its 13.4 bindings work with newer 13.x toolkits.
$version = (& "$cuda\bin\nvcc.exe" --version | Select-String "release (\d+)\.(\d+)").Matches[0].Groups
if ([int]$version[1].Value -eq 13 -and [int]$version[2].Value -gt 4) { $env:CUDARC_CUDA_VERSION = "13040" }

# CUDA 13's headers need MSVC's standard-conforming preprocessor.
$env:NVCC_APPEND_FLAGS = "-Xcompiler /Zc:preprocessor"

# nvcc needs Microsoft's C++ compiler (cl.exe); prefer Visual Studio 2022 (version 17).
if (-not $env:NVCC_CCBIN) {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  $cl = $null
  if (Test-Path $vswhere) {
    foreach ($range in @("[17.0,18.0)", "")) {
      $vsArgs = @("-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath")
      if ($range) { $vsArgs = @("-version", $range) + $vsArgs }
      foreach ($vs in (& $vswhere @vsArgs)) {
        $found = Get-ChildItem (Join-Path $vs "VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe") -ErrorAction SilentlyContinue |
          Sort-Object FullName | Select-Object -Last 1
        if ($found) { $cl = $found; break }
      }
      if ($cl) { break }
    }
  }
  if (-not $cl) { throw "Microsoft's C++ compiler (cl.exe) wasn't found: install Visual Studio Build Tools with C++." }
  $env:NVCC_CCBIN = $cl.DirectoryName
}
"Using CUDA $($version[1].Value).$($version[2].Value), C++ compiler $env:NVCC_CCBIN"

Set-Location (Join-Path $PSScriptRoot "..")
if ($mode -eq "build") {
  npm run tauri build -- --features gpu
} else {
  # Development never runs on the real data: it runs as "com.hanzicards.app.dev", whose data
  # folder gets a fresh copy of the cards and the stories each time this starts. What is tried
  # here (answers, edits, a sync set up by mistake) stays in the copy and is thrown away by
  # the next start.
  $real = Join-Path $env:APPDATA "com.hanzicards.app"
  $dev = Join-Path $env:APPDATA "com.hanzicards.app.dev"
  New-Item -ItemType Directory -Force $dev | Out-Null
  # (the last copy, and anything that would make the copy sync as the real device)
  Get-ChildItem $dev -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "hanzi-cards.db*" -or $_.Name -like "stories.db*" } | Remove-Item -Force
  if (Test-Path (Join-Path $dev "sync")) { Remove-Item -Recurse -Force (Join-Path $dev "sync") }
  foreach ($db in @(@("hanzi-cards.db", "backups\decks\auto"), @("stories.db", "backups\reading\auto"))) {
    # the newest of the file itself and the app's own automatic backups of it; the file can't
    # be copied while the installed app has it open, and then the newest backup is the copy
    $from = @(Get-Item (Join-Path $real $db[0]) -ErrorAction SilentlyContinue) +
      @(Get-ChildItem (Join-Path $real $db[1]) -Filter "$($db[0]).backup-*" -ErrorAction SilentlyContinue) |
      Sort-Object LastWriteTime -Descending
    $copied = $false
    foreach ($f in $from) {
      try { Copy-Item $f.FullName (Join-Path $dev $db[0]) -Force -ErrorAction Stop; $copied = $true; "Copy of $($db[0]): from $($f.Name) ($($f.LastWriteTime))"; break } catch {}
    }
    if (-not $copied) { "No $($db[0]) to copy: the development app starts with an empty one." }
  }
  # Big things that are downloaded, not written: shared with the real folder, not copied
  # (speech models, voices, fonts). Only what the real folder already has: nothing is made
  # in the real folder from here. Stories' made sound is the copy's own (it stays from one
  # start to the next: it is paid for by the character).
  foreach ($shared in @("whisper", "voices", "fonts")) {
    $link = Join-Path $dev $shared
    if (-not (Test-Path $link) -and (Test-Path (Join-Path $real $shared))) {
      New-Item -ItemType Junction -Path $link -Target (Join-Path $real $shared) | Out-Null
    }
  }
  $conf = (Resolve-Path "src-tauri\tauri.dev.conf.json").Path
  npm run tauri dev -- --features gpu --config "$conf"
}
