# Run the app in development mode with Whisper on an NVIDIA GPU.
# Needs the CUDA Toolkit (tested with 13.5) and the Visual Studio C++ build tools.
# The first build compiles candle's CUDA kernels (a few minutes).

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

Set-Location (Join-Path $PSScriptRoot "..")
npm run tauri dev -- --features gpu
