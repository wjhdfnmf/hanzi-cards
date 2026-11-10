# Run the app in development mode with GPU support, or build the Windows installer:
#   scripts\dev-gpu.ps1          (dev)
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
  npm run tauri dev -- --features gpu
}
