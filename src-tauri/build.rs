fn main() {
    // llama.cpp ships as dylibs (plans/DEPENDENCY_REVIEW_ENGINES.md, fallback A):
    // the bundle carries them in Contents/Frameworks, and cargo leaves them
    // beside the binary in target/<profile>, so `target/release/bench` runs.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
    }
    tauri_build::build()
}
