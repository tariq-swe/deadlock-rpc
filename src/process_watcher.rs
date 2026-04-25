use crate::game_state::GameState;
use crate::log;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const POLL_SECS: u64 = 2;

pub struct ProcessWatcher;

impl ProcessWatcher {
    pub fn run(state: Arc<Mutex<GameState>>) {
        let mut was_alive = false;
        loop {
            let alive = is_deadlock_running();
            if alive && !was_alive {
                log!("[process] Deadlock process detected");
            } else if !alive && was_alive {
                log!("[process] Deadlock process gone — resetting state");
                state.lock().unwrap().reset();
            }
            was_alive = alive;
            thread::sleep(Duration::from_secs(POLL_SECS));
        }
    }
}

#[cfg(target_os = "linux")]
fn is_deadlock_running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
            if comm.trim().eq_ignore_ascii_case("deadlock") {
                return true;
            }
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn is_deadlock_running() -> bool {
    use std::ffi::CStr;
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
        let mut found = false;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                let exe = CStr::from_ptr(entry.szExeFile.as_ptr()).to_string_lossy();
                if exe.eq_ignore_ascii_case("deadlock.exe") {
                    found = true;
                    break;
                }
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
        found
    }
}
