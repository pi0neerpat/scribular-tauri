fn main() {
    // llama.cpp ships as dylibs (plans/DEPENDENCY_REVIEW_ENGINES.md, fallback A):
    // the bundle carries them in Contents/Frameworks, and cargo leaves them
    // beside the binary in target/<profile>, so `target/release/bench` runs.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
    }
    // tauri.conf.json bundles those dylibs from src-tauri/target/release; with
    // CARGO_TARGET_DIR elsewhere, a fresh checkout lacks that path, so link it.
    #[cfg(unix)]
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        if std::fs::symlink_metadata("target").is_err() {
            let _ = std::os::unix::fs::symlink(dir, "target");
        }
    }
    tauri_build::build()
}
