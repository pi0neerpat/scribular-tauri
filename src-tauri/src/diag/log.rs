use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 1024 * 1024;
static LOGGER: OnceLock<Mutex<Option<Logger>>> = OnceLock::new();
static DISK_FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);

struct Logger {
    file: File,
    path: PathBuf,
}

impl Logger {
    fn open(dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        let path = dir.join("scribular.log");
        rotate(&path)?;
        let file = open_file(&path)?;
        Ok(Self { file, path })
    }

    fn write(&mut self, level: &str, module: &str, message: &str) -> std::io::Result<String> {
        if self.file.metadata()?.len() > MAX_BYTES {
            rotate(&self.path)?;
            self.file = open_file(&self.path)?;
        }
        let line = format!("{} {level} [{module}] {message}", timestamp());
        writeln!(self.file, "{line}")?;
        if level == "PANIC" {
            let marker = self.path.with_file_name("panic.pending");
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&marker)
            {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&marker, fs::Permissions::from_mode(0o600));
                }
                let _ = file.write_all(b"1");
                let _ = file.sync_all();
            }
        }
        self.file.flush()?;
        Ok(line)
    }
}

fn open_file(path: &Path) -> std::io::Result<File> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn rotate(path: &Path) -> std::io::Result<()> {
    if fs::metadata(path)
        .map(|m| m.len() <= MAX_BYTES)
        .unwrap_or(true)
    {
        return Ok(());
    }
    let backup = |n: u8| path.with_file_name(format!("scribular.log.{n}"));
    let _ = fs::remove_file(backup(3));
    for n in (1..=2).rev() {
        if backup(n).exists() {
            fs::rename(backup(n), backup(n + 1))?;
        }
    }
    fs::rename(path, backup(1))
}

pub(crate) fn timestamp() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (d.as_secs() / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let secs = d.as_secs() % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        d.subsec_millis()
    )
}

pub fn init(dir: &Path) -> std::io::Result<()> {
    let logger = match Logger::open(dir) {
        Ok(logger) => logger,
        Err(error) => {
            if !DISK_FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
                eprintln!("Scribular diagnostics log unavailable");
            }
            return Err(error);
        }
    };
    let _ = LOGGER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .replace(logger);
    Ok(())
}

pub fn log(level: &str, module: &str, message: &str) {
    let Some(lock) = LOGGER.get() else {
        if level == "PANIC" && !DISK_FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
            eprintln!("Scribular application panic before diagnostics startup");
        }
        return;
    };
    let result = lock
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_mut()
        .map(|l| l.write(level, module, message));
    match result {
        Some(Ok(line)) => {
            #[cfg(debug_assertions)]
            eprintln!("{line}");
            #[cfg(not(debug_assertions))]
            let _ = line;
        }
        Some(Err(_)) if !DISK_FAILURE_REPORTED.swap(true, Ordering::Relaxed) => {
            eprintln!("Scribular diagnostics log unavailable")
        }
        _ => {}
    }
}

/// Returns a closed, content-free error category.
pub fn kind(error: &(dyn Error + 'static)) -> &'static str {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(e) = error.downcast_ref::<std::io::Error>() {
            return match e.kind() {
                std::io::ErrorKind::NotFound => "io_not_found",
                std::io::ErrorKind::PermissionDenied => "io_permission_denied",
                std::io::ErrorKind::TimedOut => "io_timeout",
                _ => "io_error",
            };
        }
        if error.is::<rusqlite::Error>() {
            return "database_error";
        }
        if error.is::<reqwest::Error>() {
            return "network_error";
        }
        current = error.source();
    }
    "error"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_after_one_mib_and_keeps_three_backups() {
        let dir = std::env::temp_dir().join(format!("scribular-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut logger = Logger::open(&dir).unwrap();
        for i in 0..12_000 {
            logger
                .write("INFO", "test", &format!("{i:05} {}", "x".repeat(390)))
                .unwrap();
        }
        assert!(fs::metadata(dir.join("scribular.log")).unwrap().len() <= MAX_BYTES);
        for suffix in [".1", ".2", ".3"] {
            assert!(dir.join(format!("scribular.log{suffix}")).exists());
        }
        assert!(!dir.join("scribular.log.4").exists());
        assert!(fs::read_to_string(dir.join("scribular.log"))
            .unwrap()
            .contains("11999"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn error_kind_never_returns_display_text() {
        let error = std::io::Error::other("CANARY-CLIENT-JANE");
        assert_eq!(kind(&error), "io_error");
    }

    #[test]
    fn error_kind_follows_store_error_sources() {
        let error =
            crate::store::StoreError::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(kind(&error), "io_not_found");
        let error = crate::store::StoreError::Sqlite(rusqlite::Error::InvalidQuery);
        assert_eq!(kind(&error), "database_error");
    }
}
