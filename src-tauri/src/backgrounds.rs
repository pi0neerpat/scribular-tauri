//! Theme R6, R7 and R10: app-owned background pictures.
//!
//! Rust checks the source content, decodes and downsizes it with macOS `sips`
//! into the app-owned Backgrounds directory, and commits the new My Theme
//! selection. Settings keep only the owned ID. The webview reads pictures only
//! through the `scribular-bg` scheme, which serves this directory's files.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use sha2::{Digest, Sha256};

use crate::store::{Appearance, Background, Selection, Store, StoreError};

pub const SCHEME: &str = "scribular-bg";
pub const UNSUPPORTED: &str =
    "Scribular can't open this picture. Choose a JPEG, PNG, HEIC, or WebP picture.";
pub const NEEDS_MY_THEME: &str = "Customize a theme before choosing a picture.";
pub const SAVE_FAILED: &str = "Could not save the picture. Try again.";
pub const MISSING: &str = "The background picture is missing. Choose it again.";
pub const APPEARANCE_SAVE_FAILED: &str = "The settings could not be written.";
/// Fallback display size when no monitor is reported (5K-class retina).
pub const FALLBACK_DISPLAY: (u32, u32) = (5120, 2880);
const MAX_BYTES: u64 = 100 * 1024 * 1024;
const MAX_SIDE: u32 = 20_000;
const SIPS: &str = "/usr/bin/sips";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Jpeg,
    Png,
    Heic,
    Webp,
}

/// Identifies allowed content from its first bytes; the filename is untrusted.
fn sniff(head: &[u8]) -> Option<Format> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Format::Png);
    }
    if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return Some(Format::Webp);
    }
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        let brand = &head[8..12];
        let heic: [&[u8]; 8] = [
            b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"mif1", b"msf1",
        ];
        if heic.contains(&brand) {
            return Some(Format::Heic);
        }
    }
    None
}

/// The pixel size that still covers `display` while keeping aspect ratio;
/// `None` when the picture is already no larger than that.
fn cover(width: u32, height: u32, display: (u32, u32)) -> Option<(u32, u32)> {
    let scale = (display.0 as f64 / width as f64).max(display.1 as f64 / height as f64);
    (scale < 1.0).then(|| {
        (
            ((width as f64 * scale).round() as u32).clamp(1, width),
            ((height as f64 * scale).round() as u32).clamp(1, height),
        )
    })
}

fn sips() -> Command {
    let mut command = Command::new(SIPS);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

/// Decoded pixel size, or `None` when macOS cannot decode the file.
fn dimensions(path: &Path) -> Option<(u32, u32)> {
    let output = sips()
        .args(["-g", "pixelWidth", "-g", "pixelHeight"])
        .arg(path)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key))
            .and_then(|v| v.trim().parse::<u32>().ok())
    };
    Some((value("pixelWidth:")?, value("pixelHeight:")?)).filter(|(w, h)| *w > 0 && *h > 0)
}

fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let digest = Sha256::digest(format!(
        "{nanos}:{}:{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

pub fn owned_id(appearance: &Appearance) -> Option<&str> {
    match &appearance.my_theme.as_ref()?.values.background {
        Background::Owned(id) => Some(id),
        _ => None,
    }
}

pub struct Backgrounds {
    dir: PathBuf,
    // Held across copy, commit and cleanup so a sweep never sees an
    // uncommitted copy.
    lock: Mutex<()>,
}

impl Backgrounds {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn file(&self, id: &str) -> Option<PathBuf> {
        if !crate::store::valid_owned_id(id) {
            return None;
        }
        ["jpg", "png"]
            .iter()
            .map(|ext| self.dir.join(format!("{id}.{ext}")))
            .find(|path| path.is_file())
    }

    pub fn exists(&self, id: &str) -> bool {
        self.file(id).is_some()
    }

    /// Bytes and MIME type for the `scribular-bg` scheme.
    pub fn read(&self, id: &str) -> Option<(Vec<u8>, &'static str)> {
        let path = self.file(id)?;
        let mime = if path.extension().is_some_and(|e| e == "png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        fs::read(path).ok().map(|bytes| (bytes, mime))
    }

    /// Checks, decodes and downsizes `source` into a new owned copy. On error
    /// no new file remains and the source is untouched.
    fn import(&self, source: &Path, display: (u32, u32)) -> Result<String, String> {
        let unsupported = || UNSUPPORTED.to_string();
        if !source.is_absolute() {
            return Err(unsupported());
        }
        let meta = fs::metadata(source).map_err(|_| unsupported())?;
        if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_BYTES {
            return Err(unsupported());
        }
        let mut head = [0u8; 16];
        let mut file = fs::File::open(source).map_err(|_| unsupported())?;
        let mut read = 0;
        while read < head.len() {
            match file.read(&mut head[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(_) => return Err(unsupported()),
            }
        }
        drop(file);
        let format = sniff(&head[..read]).ok_or_else(unsupported)?;
        let (width, height) = dimensions(source).ok_or_else(unsupported)?;
        if width > MAX_SIDE || height > MAX_SIDE {
            return Err(unsupported());
        }
        fs::create_dir_all(&self.dir).map_err(|_| SAVE_FAILED.to_string())?;
        // The owned folder must be private before any copy is written.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))
                .map_err(|_| SAVE_FAILED.to_string())?;
        }
        let (ext, sips_format) = match format {
            Format::Png | Format::Webp => ("png", "png"),
            Format::Jpeg | Format::Heic => ("jpg", "jpeg"),
        };
        let id = new_id();
        let part = self.dir.join(format!("{id}.part.{ext}"));
        let done = self.dir.join(format!("{id}.{ext}"));
        let mut command = sips();
        if let Some((w, h)) = cover(width, height, display) {
            command.args(["-z", &h.to_string(), &w.to_string()]);
        }
        command.args(["-s", "format", sips_format]);
        let converted = command
            .arg(source)
            .arg("--out")
            .arg(&part)
            .output()
            .is_ok_and(|o| o.status.success())
            && dimensions(&part).is_some()
            && fs::rename(&part, &done).is_ok();
        if !converted {
            let _ = fs::remove_file(&part);
            let _ = fs::remove_file(&done);
            return Err(unsupported());
        }
        Ok(id)
    }

    fn remove(&self, id: &str) {
        if let Some(path) = self.file(id) {
            let _ = fs::remove_file(path);
        }
    }

    /// Deletes every owned copy except `keep`, including leftover partial files.
    fn sweep(&self, keep: Option<&str>) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let id = name.split('.').next().unwrap_or_default();
            if Some(id) != keep && entry.path().is_file() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Imports `source` and selects it in My Theme as one operation.
    pub fn import_and_select(
        &self,
        store: &Store,
        source: &Path,
        display: (u32, u32),
    ) -> Result<Appearance, String> {
        let _guard = self.guard();
        let current = store.appearance().map_err(|_| SAVE_FAILED.to_string())?;
        let Some(mut theme) = current.my_theme.clone() else {
            return Err(NEEDS_MY_THEME.into());
        };
        let id = self.import(source, display)?;
        theme.values.background = Background::Owned(id.clone());
        let next = Appearance {
            selected: Selection::MyTheme,
            my_theme: Some(theme),
            ..current
        };
        match store.set_appearance(&next) {
            Ok(saved) => {
                self.sweep(owned_id(&saved));
                Ok(saved)
            }
            Err(_) => {
                self.remove(&id);
                Err(SAVE_FAILED.into())
            }
        }
    }

    /// Saves `appearance`, then deletes copies no saved theme uses. Selecting
    /// My Theme needs its owned picture to exist; a built-in stays selectable
    /// while My Theme keeps a missing reference.
    pub fn save(&self, store: &Store, appearance: &Appearance) -> Result<Appearance, String> {
        let _guard = self.guard();
        if appearance.selected == Selection::MyTheme
            && owned_id(appearance).is_some_and(|id| !self.exists(id))
        {
            return Err(MISSING.into());
        }
        // Only deliberate validation text reaches the UI, never SQLite or
        // JSON details.
        let saved = store.set_appearance(appearance).map_err(|e| match e {
            StoreError::InvalidAppearance => e.to_string(),
            _ => APPEARANCE_SAVE_FAILED.to_string(),
        })?;
        self.sweep(owned_id(&saved));
        Ok(saved)
    }
}

/// Serves `scribular-bg://localhost/<id>` from the owned directory only.
pub fn respond(backgrounds: Option<&Backgrounds>, path: &str) -> tauri::http::Response<Vec<u8>> {
    let id = path.trim_start_matches('/');
    match backgrounds.and_then(|b| b.read(id)) {
        Some((bytes, mime)) => tauri::http::Response::builder()
            .header("Content-Type", mime)
            .header("Cache-Control", "no-store")
            .body(bytes),
        None => tauri::http::Response::builder()
            .status(404)
            .body(Vec::new()),
    }
    .unwrap_or_default()
}

/// Theme R6: the Rust-owned Open panel. `None` when the user cancels.
#[cfg(target_os = "macos")]
pub async fn choose(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{NSArray, NSString};
    use tauri::async_runtime::channel;

    let (send, mut receive) = channel(1);
    app.run_on_main_thread(move || {
        let chosen = MainThreadMarker::new().and_then(|mtm| {
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setCanChooseDirectories(false);
            panel.setAllowsMultipleSelection(false);
            let types: Vec<_> = ["jpg", "jpeg", "png", "heic", "heif", "webp"]
                .iter()
                .map(|t| NSString::from_str(t))
                .collect();
            #[allow(deprecated)]
            panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&types)));
            if panel.runModal() != NSModalResponseOK {
                return None;
            }
            panel
                .URL()
                .and_then(|url| url.path())
                .map(|p| PathBuf::from(p.to_string()))
        });
        let _ = send.try_send(chosen);
    })
    .map_err(|e| e.to_string())?;
    Ok(receive.recv().await.flatten())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{key::MemoryKeyStore, Accent, Mode, MyTheme, Preset, ThemeValues, Tint};
    use std::sync::atomic::AtomicUsize;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "scribular-backgrounds-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("sources")).unwrap();
            Self(dir)
        }
        fn store(&self) -> Store {
            Store::open(&self.0.join("scribular.db"), &MemoryKeyStore::default()).unwrap()
        }
        fn backgrounds(&self) -> Backgrounds {
            Backgrounds::new(self.0.join("Backgrounds"))
        }
        fn source(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join("sources").join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
        fn owned(&self) -> Vec<String> {
            let mut names: Vec<_> = fs::read_dir(self.0.join("Backgrounds"))
                .map(|d| {
                    d.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/backgrounds")
                .join(name),
        )
        .unwrap()
    }

    fn my_theme(background: Background) -> Appearance {
        Appearance {
            selected: Selection::MyTheme,
            my_theme: Some(MyTheme {
                origin: Preset::Default,
                copied_mode: Some(Mode::Automatic),
                values: ThemeValues {
                    mode: Mode::Automatic,
                    accent: Accent::System,
                    tint: Tint::None,
                    background,
                    blur: 0,
                    brightness: 100,
                    stars_motion: false,
                },
            }),
            ..Appearance::default()
        }
    }

    #[test]
    fn sniffs_allowed_content_and_covers_the_display() {
        assert_eq!(sniff(&fixture("picture.jpg")), Some(Format::Jpeg));
        assert_eq!(sniff(&fixture("picture.png")), Some(Format::Png));
        assert_eq!(sniff(&fixture("picture.heic")), Some(Format::Heic));
        assert_eq!(sniff(&fixture("picture.webp")), Some(Format::Webp));
        assert_eq!(sniff(b"GIF89a......"), None);
        assert_eq!(sniff(b"\0\0\0\x1cftypavif"), None);
        assert_eq!(cover(4000, 3000, (1000, 1000)), Some((1333, 1000)));
        assert_eq!(cover(3000, 4000, (2000, 1000)), Some((2000, 2667)));
        assert_eq!(cover(64, 48, (1000, 1000)), None);
    }

    // AS6
    #[test]
    fn imports_only_decodable_allowed_content_into_downsized_owned_copies() {
        let temp = Temp::new();
        let store = temp.store();
        let backgrounds = temp.backgrounds();
        assert_eq!(
            backgrounds
                .import_and_select(
                    &store,
                    &temp.source("a.png", &fixture("picture.png")),
                    (10, 10)
                )
                .unwrap_err(),
            NEEDS_MY_THEME
        );
        store.set_appearance(&my_theme(Background::None)).unwrap();

        for (name, bytes) in [
            ("picture.jpg", fixture("picture.jpg")),
            ("picture.png", fixture("picture.png")),
            ("picture.heic", fixture("picture.heic")),
            ("picture.webp", fixture("picture.webp")),
            // Content decides, not the extension.
            ("png-named.jpg", fixture("picture.png")),
        ] {
            let source = temp.source(name, &bytes);
            let saved = backgrounds
                .import_and_select(&store, &source, (32, 32))
                .unwrap();
            let id = owned_id(&saved).unwrap().to_string();
            assert_eq!(store.appearance().unwrap(), saved);
            let path = backgrounds.file(&id).unwrap();
            // 64x48 covering 32x32 keeps aspect ratio: 43x32.
            assert_eq!(dimensions(&path), Some((43, 32)), "{name}");
            assert_eq!(fs::read(&source).unwrap(), bytes, "{name} source untouched");
            assert_eq!(temp.owned().len(), 1, "{name}: replaced copy removed");
            assert!(!serde_json::to_string(&saved).unwrap().contains("sources"));
        }
        // A smaller picture is copied without upscaling.
        let saved = backgrounds
            .import_and_select(
                &store,
                &temp.source("small.png", &fixture("picture.png")),
                (5120, 2880),
            )
            .unwrap();
        assert_eq!(
            dimensions(&backgrounds.file(owned_id(&saved).unwrap()).unwrap()),
            Some((64, 48))
        );

        let prior = store.appearance().unwrap();
        let prior_files = temp.owned();
        let mut truncated = fixture("picture.png");
        truncated.truncate(24);
        let malformed = temp.source("malformed.png", &truncated);
        let text = temp.source("text.jpg", b"not a picture at all");
        let gif = temp.source("picture.gif", b"GIF89a\x01\0\x01\0\0\0\0;");
        let empty = temp.source("empty.png", b"");
        for source in [
            malformed.clone(),
            text,
            gif,
            empty,
            temp.0.join("sources/missing.png"),
            temp.0.join("sources"),
            PathBuf::from("relative.png"),
        ] {
            assert_eq!(
                backgrounds
                    .import_and_select(&store, &source, (32, 32))
                    .unwrap_err(),
                UNSUPPORTED,
                "{}",
                source.display()
            );
            assert_eq!(store.appearance().unwrap(), prior);
            assert_eq!(temp.owned(), prior_files, "no partial file");
        }
        assert_eq!(fs::read(&malformed).unwrap(), truncated);
    }

    #[test]
    fn failed_commit_removes_the_new_copy_and_keeps_the_prior_theme() {
        let temp = Temp::new();
        let store = temp.store();
        let backgrounds = temp.backgrounds();
        store.set_appearance(&my_theme(Background::Stars)).unwrap();
        store
            .execute_batch_for_test("CREATE TEMP TRIGGER reject_appearance BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT, 'test write failure'); END;")
            .unwrap();
        let source = temp.source("a.png", &fixture("picture.png"));
        assert_eq!(
            backgrounds
                .import_and_select(&store, &source, (32, 32))
                .unwrap_err(),
            SAVE_FAILED
        );
        assert_eq!(store.appearance().unwrap(), my_theme(Background::Stars));
        assert!(temp.owned().is_empty());
    }

    // AS7
    #[test]
    fn cleanup_removes_only_unreferenced_copies() {
        let temp = Temp::new();
        let store = temp.store();
        let backgrounds = temp.backgrounds();
        store.set_appearance(&my_theme(Background::None)).unwrap();
        let first = backgrounds
            .import_and_select(
                &store,
                &temp.source("a.png", &fixture("picture.png")),
                (32, 32),
            )
            .unwrap();
        let first_id = owned_id(&first).unwrap().to_string();

        // Selecting a built-in keeps My Theme and its picture.
        let built_in = Appearance {
            selected: Selection::Sage,
            ..first.clone()
        };
        backgrounds.save(&store, &built_in).unwrap();
        assert!(backgrounds.exists(&first_id));

        // Replacing the picture deletes the copy no saved theme uses.
        let second = backgrounds
            .import_and_select(
                &store,
                &temp.source("b.jpg", &fixture("picture.jpg")),
                (32, 32),
            )
            .unwrap();
        let second_id = owned_id(&second).unwrap().to_string();
        assert!(!backgrounds.exists(&first_id));
        assert!(backgrounds.exists(&second_id));

        // A saved reference must point at an owned copy.
        assert_eq!(
            backgrounds
                .save(&store, &my_theme(Background::Owned(first_id.clone())))
                .unwrap_err(),
            MISSING
        );
        assert_eq!(store.appearance().unwrap(), second);

        // Delete All Sessions leaves theme assets and settings intact.
        let session = store
            .create_session(&crate::store::NewSession {
                title: "session".into(),
                input_mode: crate::store::InputMode::Mic,
                prompt_id: None,
                model_id: None,
            })
            .unwrap();
        for s in store.list_sessions().unwrap() {
            store.delete_session(&s.id).unwrap();
        }
        assert!(store.get_session(&session.id).unwrap().is_none());
        assert_eq!(store.appearance().unwrap(), second);
        assert!(backgrounds.exists(&second_id));

        // Reset to the copied preset (no picture) reclaims the copy.
        backgrounds
            .save(&store, &my_theme(Background::None))
            .unwrap();
        assert!(temp.owned().is_empty());
    }

    #[test]
    fn missing_my_theme_picture_only_blocks_selecting_my_theme() {
        let temp = Temp::new();
        let store = temp.store();
        let backgrounds = temp.backgrounds();
        store.set_appearance(&my_theme(Background::None)).unwrap();
        let saved = backgrounds
            .import_and_select(
                &store,
                &temp.source("a.png", &fixture("picture.png")),
                (32, 32),
            )
            .unwrap();
        let id = owned_id(&saved).unwrap().to_string();
        fs::remove_file(backgrounds.file(&id).unwrap()).unwrap();

        // Built-ins stay selectable and My Theme keeps its reference.
        let sage = Appearance {
            selected: Selection::Sage,
            ..saved.clone()
        };
        assert_eq!(backgrounds.save(&store, &sage).unwrap(), sage);
        assert_eq!(backgrounds.save(&store, &saved).unwrap_err(), MISSING);
        assert_eq!(store.appearance().unwrap(), sage);
    }

    #[test]
    fn save_errors_never_expose_store_details() {
        let temp = Temp::new();
        let store = temp.store();
        let backgrounds = temp.backgrounds();
        store.set_appearance(&my_theme(Background::Stars)).unwrap();
        let invalid = Appearance {
            version: 2,
            ..my_theme(Background::Stars)
        };
        assert_eq!(
            backgrounds.save(&store, &invalid).unwrap_err(),
            StoreError::InvalidAppearance.to_string()
        );
        store
            .execute_batch_for_test("CREATE TEMP TRIGGER reject_appearance BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT, 'test write failure'); END;")
            .unwrap();
        assert_eq!(
            backgrounds
                .save(&store, &my_theme(Background::None))
                .unwrap_err(),
            APPEARANCE_SAVE_FAILED
        );
    }

    #[test]
    fn scheme_serves_only_owned_ids() {
        let temp = Temp::new();
        let store = temp.store();
        let backgrounds = temp.backgrounds();
        store.set_appearance(&my_theme(Background::None)).unwrap();
        let saved = backgrounds
            .import_and_select(
                &store,
                &temp.source("a.png", &fixture("picture.png")),
                (32, 32),
            )
            .unwrap();
        let id = owned_id(&saved).unwrap();
        let ok = respond(Some(&backgrounds), &format!("/{id}"));
        assert_eq!(ok.status(), 200);
        assert_eq!(ok.headers()["Content-Type"], "image/png");
        for path in [
            "/../scribular.db",
            "/sources%2Fa.png",
            "/",
            "/a.png",
            "/%2E%2E",
        ] {
            assert_eq!(respond(Some(&backgrounds), path).status(), 404, "{path}");
        }
        assert_eq!(respond(None, &format!("/{id}")).status(), 404);
    }
}
