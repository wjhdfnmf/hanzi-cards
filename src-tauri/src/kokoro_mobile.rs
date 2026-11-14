//! The phone build's stand-in for kokoro.rs: the built-in voice isn't offered there (a phone
//! has Chinese system voices of its own, and sherpa-onnx isn't built for it). The same
//! functions, so commands.rs is the same on every platform.

use crate::error::{AppError, AppResult};
use std::path::Path;

const UNAVAILABLE: &str = "The built-in voice isn't available on this device. The system voice is used instead.";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installed: bool,
    pub download_mb: u64,
}

pub fn status(_app_data: &Path) -> Status {
    Status { installed: false, download_mb: 0 }
}

pub fn download(_app_data: &Path, _progress: impl FnMut(u64, u64)) -> AppResult<()> {
    Err(AppError::msg(UNAVAILABLE))
}

pub fn remove(_app_data: &Path) -> AppResult<()> {
    Ok(())
}

pub fn synthesize(_app_data: &Path, _text: &str, _rate: f64) -> Result<Vec<u8>, String> {
    Err(UNAVAILABLE.into())
}
