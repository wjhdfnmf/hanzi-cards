//! NVIDIA GPU support for Whisper, installed on demand.
//!
//! The GPU build needs three of NVIDIA's CUDA libraries (cuBLAS, cuBLASLt, cuRAND, ~660 MB).
//! They aren't in the installer: Settings offers them when an NVIDIA card is found, downloads
//! NVIDIA's official redistributable archives, checks their SHA-256 and keeps just the DLLs
//! in `<app data>/cuda`. On Windows the app delay-loads them (see build.rs), so it starts
//! without them; `problem()` makes sure they're present before any GPU code runs.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The CUDA release the libraries come from (tested with this app's build).
const CUDA_RELEASE: &str = "13.5.0";
const REDIST: &str = "https://developer.download.nvidia.com/compute/cuda/redist";
/// (component in NVIDIA's manifest, DLLs to keep from it)
const COMPONENTS: [(&str, &[&str]); 2] =
    [("libcublas", &["cublas64_13.dll", "cublasLt64_13.dll"]), ("libcurand", &["curand64_10.dll"])];
/// Approximate download size, for Settings (the archives also hold headers and static libraries).
pub const DOWNLOAD_MB: u64 = 505;

pub fn cuda_dir(app_data: &Path) -> PathBuf {
    app_data.join("cuda")
}

/// Whether this build can use a GPU at all.
pub const fn built_with_gpu() -> bool {
    cfg!(all(feature = "gpu", windows))
}

/// Make the downloaded libraries findable (call once at startup and after a download).
pub fn init(app_data: &Path) {
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
        let dir = cuda_dir(app_data);
        // SAFETY: plain Win32 call with a valid, NUL-terminated wide string.
        unsafe {
            let _ = SetDllDirectoryW(&HSTRING::from(dir.as_os_str()));
        }
    }
    let _ = app_data;
    reset();
}

/// Whether a DLL can be loaded (Windows); elsewhere, always false.
fn loadable(name: &str) -> bool {
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        // SAFETY: loading a library by name; the handle is kept (the app uses it next).
        unsafe { LoadLibraryW(&HSTRING::from(name)).is_ok() }
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        false
    }
}

/// An NVIDIA display driver is installed (so an NVIDIA card is present).
pub fn has_nvidia() -> bool {
    loadable("nvcuda.dll")
}

/// NVIDIA's libraries are installed (downloaded, or from a CUDA Toolkit on PATH).
pub fn libs_installed() -> bool {
    COMPONENTS.iter().flat_map(|(_, dlls)| dlls.iter()).all(|d| loadable(d))
}

static PROBLEM: Mutex<Option<Option<String>>> = Mutex::new(None);

fn reset() {
    *PROBLEM.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Why the GPU can't be used right now (None = it can). Checked once, again after a download.
pub fn problem() -> Option<String> {
    let mut cached = PROBLEM.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = cached.as_ref() {
        return p.clone();
    }
    let p = check();
    *cached = Some(p.clone());
    p
}

fn check() -> Option<String> {
    if !built_with_gpu() {
        return Some("This build of the app has no GPU support.".into());
    }
    if !has_nvidia() {
        return Some("No NVIDIA graphics card (or driver) found.".into());
    }
    if !libs_installed() {
        return Some("The graphics card libraries aren't installed yet.".into());
    }
    #[cfg(feature = "gpu")]
    {
        match candle_core::Device::new_cuda(0) {
            Ok(_) => None,
            Err(e) => Some(format!("The graphics card couldn't be used ({e}). Updating the NVIDIA driver may help.")),
        }
    }
    #[cfg(not(feature = "gpu"))]
    None
}

/// Download NVIDIA's libraries into `<app data>/cuda`, reporting (bytes so far, total).
pub fn download(app_data: &Path, mut progress: impl FnMut(u64, u64)) -> AppResult<()> {
    let text = ureq::get(&format!("{REDIST}/redistrib_{CUDA_RELEASE}.json"))
        .call()
        .map_err(|e| AppError::msg(format!("Couldn't reach NVIDIA's download site: {e}")))?
        .into_string()?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| AppError::msg(format!("Unexpected answer from NVIDIA's download site: {e}")))?;
    let platform = "windows-x86_64";
    let mut parts: Vec<(String, String, u64, &[&str])> = Vec::new();
    for (component, dlls) in COMPONENTS {
        let entry = &manifest[component][platform];
        let (Some(path), Some(sha)) = (entry["relative_path"].as_str(), entry["sha256"].as_str()) else {
            return Err(AppError::msg(format!("NVIDIA's list has no {component} for Windows.")));
        };
        let size = entry["size"].as_str().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        parts.push((format!("{REDIST}/{path}"), sha.to_lowercase(), size, dlls));
    }
    let total: u64 = parts.iter().map(|p| p.2).sum();
    let dir = cuda_dir(app_data);
    std::fs::create_dir_all(&dir)?;
    let mut done = 0u64;
    for (url, sha, _, dlls) in parts {
        // download to a temporary file, hashing as it goes
        let tmp = dir.join("download.part");
        let resp = ureq::get(&url).call().map_err(|e| AppError::msg(format!("Download failed: {e}")))?;
        let mut reader = resp.into_reader();
        let mut file = std::fs::File::create(&tmp)?;
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            sha2::Digest::update(&mut hasher, &buf[..n]);
            file.write_all(&buf[..n])?;
            done += n as u64;
            progress(done, total.max(done));
        }
        drop(file);
        let got: String = sha2::Digest::finalize(hasher).iter().map(|b| format!("{b:02x}")).collect();
        if got != sha {
            let _ = std::fs::remove_file(&tmp);
            return Err(AppError::msg("The download was damaged (checksum mismatch). Please try again."));
        }
        // keep only the DLLs
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&tmp)?)
            .map_err(|e| AppError::msg(format!("Couldn't open the downloaded archive: {e}")))?;
        for &dll in dlls.iter() {
            let index = (0..zip.len())
                .find(|&i| zip.by_index(i).is_ok_and(|f| f.name().rsplit('/').next() == Some(dll)))
                .ok_or_else(|| AppError::msg(format!("{dll} is missing from NVIDIA's archive.")))?;
            let mut entry = zip.by_index(index).map_err(|e| AppError::msg(e.to_string()))?;
            let part = dir.join(format!("{dll}.part"));
            let mut out = std::fs::File::create(&part)?;
            std::io::copy(&mut entry, &mut out)?;
            drop(out);
            std::fs::rename(&part, dir.join(dll))?;
        }
        drop(zip);
        let _ = std::fs::remove_file(&tmp);
    }
    init(app_data);
    Ok(())
}

/// Remove the downloaded libraries (only possible while they aren't loaded).
pub fn remove(app_data: &Path) -> AppResult<()> {
    let dir = cuda_dir(app_data);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|_| AppError::msg("They're in use: switch Whisper to the processor, restart the app, then remove them."))?;
    }
    reset();
    Ok(())
}

/// Whether the libraries were downloaded into the app's folder (as opposed to a CUDA Toolkit).
pub fn downloaded(app_data: &Path) -> bool {
    COMPONENTS.iter().flat_map(|(_, dlls)| dlls.iter()).all(|d| cuda_dir(app_data).join(d).is_file())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuLibs {
    pub nvidia: bool,
    pub installed: bool,
    pub downloaded: bool,
    pub download_mb: u64,
}

pub fn status(app_data: &Path) -> GpuLibs {
    GpuLibs {
        nvidia: built_with_gpu() && has_nvidia(),
        installed: built_with_gpu() && libs_installed(),
        downloaded: downloaded(app_data),
        download_mb: DOWNLOAD_MB,
    }
}

#[cfg(test)]
mod tests {
    /// Downloads ~505 MB from NVIDIA. GPU_DL_DIR=<empty folder> cargo test --lib gpu_download -- --ignored --nocapture
    #[test]
    #[ignore]
    fn gpu_download() {
        let Ok(dir) = std::env::var("GPU_DL_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let t = std::time::Instant::now();
        let mut last = 0;
        super::download(dir, |got, total| {
            let pct = got * 100 / total.max(1);
            if pct >= last + 20 {
                last = pct;
                println!("{pct}% of {} MB", total / 1_000_000);
            }
        })
        .unwrap();
        println!("done in {:.0}s", t.elapsed().as_secs_f32());
        for f in std::fs::read_dir(super::cuda_dir(dir)).unwrap() {
            let f = f.unwrap();
            println!("{} {} MB", f.file_name().to_string_lossy(), f.metadata().unwrap().len() / 1_000_000);
        }
        assert!(super::downloaded(dir));
    }
}
