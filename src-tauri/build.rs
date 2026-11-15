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
    // `appstore`: the build for Apple's App Store (every iPhone build; HANZI_APPSTORE=1 turns it
    // on anywhere, to check that it compiles). It leaves out the AI tutor with everything that
    // talks to Anthropic, and checking tones by pitch (tones.rs and what uses it, whose numbers
    // were fitted to recordings that aren't ours to hand on): see lib.rs and commands.rs.
    println!("cargo:rustc-check-cfg=cfg(appstore)");
    println!("cargo:rerun-if-env-changed=HANZI_APPSTORE");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") || std::env::var_os("HANZI_APPSTORE").is_some() {
        println!("cargo:rustc-cfg=appstore");
    }
    tauri_build::build()
}
