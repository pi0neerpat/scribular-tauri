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
        let link = std::path::Path::new("target");
        let current = std::fs::read_link(link).ok();
        if current.as_deref() != Some(dir.as_ref()) {
            // A stale link from an older CARGO_TARGET_DIR is replaced; a real
            // directory is left alone and surfaces as the warning below.
            if current.is_some() {
                let _ = std::fs::remove_file(link);
            }
            if let Err(e) = std::os::unix::fs::symlink(&dir, link) {
                println!("cargo:warning=could not link src-tauri/target -> {dir}: {e}; remove src-tauri/target or bundling misses the llama dylibs");
            }
        }
    }
    tauri_build::build()
}
