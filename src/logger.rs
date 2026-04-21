use std::io::{LineWriter, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOGGER: OnceLock<Mutex<LineWriter<std::fs::File>>> = OnceLock::new();

pub fn init() {
    let log_dir = log_dir();
    std::fs::create_dir_all(&log_dir).ok();
    let path = log_dir.join("deadlock-rpc.log");

    match std::fs::File::create(&path) {
        Ok(file) => {
            LOGGER.set(Mutex::new(LineWriter::new(file))).ok();
            write_log(&format!("deadlock-rpc started — log: {}", path.display()));
        }
        Err(e) => eprintln!("[logger] Failed to open log file: {e}"),
    }
}

pub fn write_log(msg: &str) {
    if let Some(m) = LOGGER.get() {
        let ts = timestamp();
        let line = format!("[{ts}] {msg}\n");
        if let Ok(mut w) = m.lock() {
            let _ = w.write_all(line.as_bytes());
        }
    }
}

fn log_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("logs")))
        .unwrap_or_else(|| PathBuf::from("logs"))
}

fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::logger::write_log(&format!($($arg)*))
    };
}
