#![allow(dead_code)]

use crate::log;
use std::time::{Duration, Instant};

#[cfg(windows)]
const GAME_PROCESS: &str = "deadlock.exe";
#[cfg(not(windows))]
const GAME_PROCESS: &str = "deadlock";

/// How long to wait for a clean exit before force-killing.
const EXIT_WAIT_SECS: u64 = 30;

/// Waits up to `EXIT_WAIT_SECS` for the Deadlock process to exit on its own.
/// If it's still alive after that, kills it to avoid a stuck black screen.
pub fn wait_and_kill_if_needed() {
    if !is_game_running() {
        return;
    }

    log!(
        "[guard] Game process still alive — waiting up to {}s for clean exit...",
        EXIT_WAIT_SECS
    );

    let deadline = Instant::now() + Duration::from_secs(EXIT_WAIT_SECS);
    loop {
        std::thread::sleep(Duration::from_secs(2));

        if !is_game_running() {
            log!("[guard] Game exited cleanly.");
            return;
        }

        if Instant::now() >= deadline {
            log!(
                "[guard] Game still running after {}s — force-killing to clear black screen.",
                EXIT_WAIT_SECS
            );
            force_kill();
            return;
        }
    }
}

#[cfg(windows)]
fn is_game_running() -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {}", GAME_PROCESS), "/NH"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains("deadlock.exe")
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_game_running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        std::fs::read_to_string(entry.path().join("comm"))
            .ok()
            .map(|s| s.trim() == GAME_PROCESS)
            .unwrap_or(false)
    })
}

#[cfg(windows)]
fn force_kill() {
    match std::process::Command::new("taskkill")
        .args(["/F", "/IM", GAME_PROCESS])
        .status()
    {
        Ok(s) if s.success() => log!("[guard] Force-killed {}.", GAME_PROCESS),
        Ok(s) => log!("[guard] taskkill exited with status {}.", s),
        Err(e) => log!("[guard] taskkill failed: {}.", e),
    }
}

#[cfg(not(windows))]
fn force_kill() {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(name) = std::fs::read_to_string(path.join("comm")) else {
            continue;
        };
        if name.trim() != GAME_PROCESS {
            continue;
        }
        let Some(pid_str) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        // SIGTERM — polite first, lets the game flush if it can.
        let result = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        match result {
            Ok(_) => log!("[guard] Sent SIGTERM to {} (pid {}).", GAME_PROCESS, pid),
            Err(e) => log!("[guard] kill failed for pid {}: {}.", pid, e),
        }
    }
}
