# Plan: Whisper on the NVIDIA GPU (RTX 3070)

Status: plan, written 2026-11-09. Nothing below is built yet.

## Why

Whisper small on the CPU (Ryzen 9 5900X) now takes about 0.8 s (tutor) and 1.1 s (drill)
per short answer, after the CPU work described in `ai-voice-study.md`. On an RTX 3070
(8 GB, compute capability 8.6) the same model should take well under 0.3 s, and the
larger, much more accurate **Whisper large-v3-turbo** (809 M parameters, about 1.6 GB in
half precision) becomes practical: its encoder is too slow for the CPU, but it fits the
3070's memory easily and its 4-layer decoder is fast.

## What you install

1. **NVIDIA CUDA Toolkit 12.9** (Windows › x86_64 › 11 or 10 › exe (local), about 3 GB)
   from developer.nvidia.com/cuda-downloads (or the archive for 12.9).
   - Choose **Custom** install and **untick "Driver components"**: the installed driver
     (591.86, CUDA 13.1) is newer than the one bundled with the toolkit.
   - Keep "CUDA › Development", "Runtime" and "Libraries". "Visual Studio Integration",
     Nsight and the samples are not needed.
   - CUDA 13.x should also work (candle's `cudarc` detects the version at build time), but
     12.9 is the most tested with candle and supports Visual Studio 2022 as-is.
2. Open a **new** terminal afterwards and check:
   ```bash
   nvcc --version
   ```
   and that `CUDA_PATH` points at `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.9`.
3. Nothing else: the C++ compiler `nvcc` needs (`cl.exe`) is already there in the
   Visual Studio 2022 Build Tools (MSVC 14.44). cuDNN is not needed.

## What I build

### 1. An optional `gpu` feature

- `src-tauri/Cargo.toml`: `[features] gpu = ["candle-core/cuda", "candle-nn/cuda", "candle-transformers/cuda"]`.
  Builds without the feature stay CPU-only, so the app still builds on machines without CUDA
  (macOS, AMD/Intel graphics, CI).
- Run with `npm run tauri dev -- --features gpu`; release with `npm run tauri build -- --features gpu`.
- nvcc must use the VS 2022 compiler, not the newer "Visual Studio 18" one also installed:
  set `NVCC_CCBIN` to `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64`
  (in `src-tauri/.cargo/config.toml` `[env]`, so it only affects this project). The
  C runtime workaround in `~/.cargo/config.toml` still applies.
- The first build compiles candle's CUDA kernels: expect 5-15 minutes once.

### 2. Device choice at run time

- `whisper.rs`: `Device::new_cuda(0)` when the feature is on and the setting allows it;
  if CUDA fails to start (no driver, DLLs missing), fall back to the CPU and report why.
- New setting `whisper_device`: "auto" (GPU if available), "cpu". Settings › Speech
  recognition shows the device in use and the GPU name.
- Weights in **F16** on the GPU (half the memory, uses the tensor cores); F32 on the CPU.
  `whisper_net.rs` builds its masks and suppress tensors in the model's dtype, and
  logits are converted to F32 before `log_softmax`.
- `speech_warmup` also allocates the GPU memory, so the first answer isn't slow.

### 3. Large-v3-turbo

- Add `("large-v3-turbo", "openai/whisper-large-v3-turbo", 1620)` to `whisper::MODELS`,
  marked "GPU recommended" in Settings (and hidden or warned about without a GPU).
- It uses 128 mel bins (our `mel_filters` already takes the count from `config.json`) and a
  tokenizer with one extra language token; special tokens are looked up by name, so no
  code assumes their ids.
- On the GPU the speech window can go back to the full 30 s (best accuracy) since the
  encoder cost no longer matters; keep `SPEECH_WINDOW` for the CPU.

### 4. Measure

`whisper_bench` with a new `WHISPER_DEVICE=cuda` switch, on the same 60 clips:

| | expect |
|---|---|
| small, GPU, F16 | same accuracy as CPU, < 0.3 s per clip |
| large-v3-turbo, GPU | clearly better on card words and noisy clips, ~0.3-0.5 s |

Then pick the default model for this PC from the numbers, and test with your own voice.

## Runtime notes

- With the toolkit installed, the CUDA DLLs (`cublas64_12.dll`, `cublasLt64_12.dll`,
  `nvrtc64_*.dll`, `curand64_*.dll`) are found through `CUDA_PATH\bin` on PATH.
- To give the app to someone without the toolkit, those DLLs (several hundred MB) would
  have to be bundled with the installer, or that person uses the CPU build. That decision
  can wait until the app is shared.
- Memory: large-v3-turbo in F16 needs about 2 GB of the 3070's 8 GB, leaving room for games
  or other apps.

## Risks

- nvcc may reject the compiler version; fix with `NVCC_CCBIN` (above) or
  `-allow-unsupported-compiler`.
- candle's CUDA kernels occasionally lag new toolkits; 12.9 avoids that.
- If anything blocks the GPU build, the CPU path keeps working unchanged.
