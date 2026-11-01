use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use dirs::config_dir;

static LOGGER: OnceLock<Mutex<PathBuf>> = OnceLock::new();
const APP_DIR_NAME: &str = "reins";

pub fn init_logger() {
    let log_path = log_file_path();

    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let _ = OpenOptions::new().create(true).append(true).open(&log_path);

    let _ = LOGGER.set(Mutex::new(log_path));
}

pub fn log_info(message: impl AsRef<str>) {
    write_log("INFO", message.as_ref());
}

pub fn log_error(message: impl AsRef<str>) {
    write_log("ERROR", message.as_ref());
}

fn log_file_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
        .join("logs")
        .join("app.log")
}

fn write_log(level: &str, message: &str) {
    let Some(path_lock) = LOGGER.get() else {
        return;
    };

    let Ok(path) = path_lock.lock() else {
        return;
    };

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&*path) else {
        return;
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();

    let _ = writeln!(file, "[{timestamp}] {level} {message}");
}
