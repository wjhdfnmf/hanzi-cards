//! Free Chinese fonts for the Reading page, downloaded on request into the app-data folder
//! ("fonts/") and loaded by the page itself, so nothing is installed into the system.
//! All are under the SIL Open Font License 1.1.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const GOOGLE: &str = "https://raw.githubusercontent.com/google/fonts/main/ofl";

/// (key, file name, URL path, size in bytes). Keys match the Reading font list (reading.ts).
const CATALOG: &[(&str, &str, &str, u64)] = &[
    ("wenkai", "LXGWWenKai-Regular.ttf", "https://github.com/lxgw/LxgwWenKai/releases/download/v1.530/LXGWWenKai-Regular.ttf", 25_928_940),
    ("notoserif", "NotoSerifSC-wght.ttf", "/notoserifsc/NotoSerifSC%5Bwght%5D.ttf", 25_125_512),
    ("mashanzheng", "MaShanZheng-Regular.ttf", "/mashanzheng/MaShanZheng-Regular.ttf", 5_857_936),
    ("zhimangxing", "ZhiMangXing-Regular.ttf", "/zhimangxing/ZhiMangXing-Regular.ttf", 4_063_532),
    ("longcang", "LongCang-Regular.ttf", "/longcang/LongCang-Regular.ttf", 5_162_508),
    ("caoshu", "LiuJianMaoCao-Regular.ttf", "/liujianmaocao/LiuJianMaoCao-Regular.ttf", 4_951_804),
    ("kuaile", "ZCOOLKuaiLe-Regular.ttf", "/zcoolkuaile/ZCOOLKuaiLe-Regular.ttf", 1_514_968),
    ("xiaowei", "ZCOOLXiaoWei-Regular.ttf", "/zcoolxiaowei/ZCOOLXiaoWei-Regular.ttf", 6_313_808),
    ("huangyou", "ZCOOLQingKeHuangYou-Regular.ttf", "/zcoolqingkehuangyou/ZCOOLQingKeHuangYou-Regular.ttf", 8_328_684),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFile {
    pub key: String,
    pub size_bytes: u64,
    pub downloaded: bool,
}

fn dir(app_data: &Path) -> PathBuf {
    app_data.join("fonts")
}

fn entry(key: &str) -> AppResult<&'static (&'static str, &'static str, &'static str, u64)> {
    CATALOG.iter().find(|e| e.0 == key).ok_or_else(|| AppError::msg(format!("Unknown font: {key}")))
}

fn url(path: &str) -> String {
    if path.starts_with("https://") { path.to_string() } else { format!("{GOOGLE}{path}") }
}

pub fn list(app_data: &Path) -> Vec<FontFile> {
    let d = dir(app_data);
    CATALOG
        .iter()
        .map(|(key, file, _, size)| FontFile { key: key.to_string(), size_bytes: *size, downloaded: d.join(file).is_file() })
        .collect()
}

/// TrueType or OpenType, by the first four bytes.
fn is_font(head: &[u8]) -> bool {
    matches!(head, [0, 1, 0, 0, ..] | [b'O', b'T', b'T', b'O', ..] | [b't', b'r', b'u', b'e', ..])
}

pub fn download(app_data: &Path, key: &str, mut progress: impl FnMut(u64, u64)) -> AppResult<()> {
    let (_, file, path, size) = *entry(key)?;
    let d = dir(app_data);
    std::fs::create_dir_all(&d)?;
    let resp = ureq::get(&url(path)).call().map_err(|e| AppError::msg(format!("Download failed: {e}")))?;
    let total = resp.header("content-length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(size);
    let part = d.join(format!("{file}.part"));
    let result = (|| -> AppResult<()> {
        let mut out = std::fs::File::create(&part)?;
        let mut reader = resp.into_reader();
        let mut buf = vec![0u8; 1 << 16];
        let mut done = 0u64;
        let mut head = Vec::new();
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if head.len() < 4 {
                head.extend_from_slice(&buf[..n.min(4)]);
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
            progress(done, total.max(done));
        }
        out.flush()?;
        if !is_font(&head) {
            return Err(AppError::msg("The download wasn't a font file"));
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, d.join(file))?;
    Ok(())
}

pub fn delete(app_data: &Path, key: &str) -> AppResult<()> {
    let (_, file, _, _) = *entry(key)?;
    let p = dir(app_data).join(file);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

/// A downloaded font's file, for the page to load.
pub fn read(app_data: &Path, key: &str) -> AppResult<Vec<u8>> {
    let (_, file, _, _) = *entry(key)?;
    std::fs::read(dir(app_data).join(file)).map_err(|_| AppError::msg("That font hasn't been downloaded"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog() {
        let tmp = std::env::temp_dir().join(format!("hc-fonts-{}", std::process::id()));
        std::fs::create_dir_all(dir(&tmp)).unwrap();
        std::fs::write(dir(&tmp).join("ZCOOLKuaiLe-Regular.ttf"), [0, 1, 0, 0, 9]).unwrap();
        let l = list(&tmp);
        assert_eq!(l.len(), CATALOG.len());
        assert!(l.iter().find(|f| f.key == "kuaile").unwrap().downloaded);
        assert!(!l.iter().find(|f| f.key == "wenkai").unwrap().downloaded);
        assert_eq!(read(&tmp, "kuaile").unwrap(), vec![0, 1, 0, 0, 9]);
        delete(&tmp, "kuaile").unwrap();
        assert!(read(&tmp, "kuaile").is_err());
        assert!(entry("nope").is_err());
        assert_eq!(url("/a/b.ttf"), format!("{GOOGLE}/a/b.ttf"));
        assert!(is_font(b"OTTO..") && !is_font(b"<!DO"));
        let _ = std::fs::remove_dir_all(tmp);
    }
}
