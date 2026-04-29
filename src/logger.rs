use simplelog::*;
use std::path::PathBuf;

pub fn init() {
    let log_dir = log_dir();
    std::fs::create_dir_all(&log_dir).ok();
    let path = log_dir.join("deadlock-rpc.log");

    let level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_location_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Off)
        .set_thread_level(LevelFilter::Off)
        .build();

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        level,
        config.clone(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )];

    if let Ok(file) = std::fs::File::create(&path) {
        loggers.push(WriteLogger::new(level, config, file));
    }

    CombinedLogger::init(loggers).ok();

    log::info!("deadlock-rpc started — log: {}", path.display());
}

fn log_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("logs")))
        .unwrap_or_else(|| PathBuf::from("logs"))
}
