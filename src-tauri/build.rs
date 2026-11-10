fn main() {
    // The GPU build links NVIDIA's cuBLAS and cuRAND. They aren't shipped with the app (the user
    // downloads them in Settings), so on Windows they're delay-loaded: the app starts without
    // them and they're loaded on first use, which only happens after `gpu.rs` has checked
    // that they're present.
    if std::env::var_os("CARGO_FEATURE_GPU").is_some() && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for dll in ["cublas64_13.dll", "curand64_10.dll"] {
            println!("cargo:rustc-link-arg=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-arg=delayimp.lib");
    }
    tauri_build::build()
}
