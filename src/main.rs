#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod game_state;
mod hero_api;
mod launcher;
mod logger;
mod log_watcher;
mod steam;
mod tray;
mod updater;

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use game_state::{GamePhase, GameState, MatchMode};
use hero_api::{HeroCache, HeroData};
use log_watcher::LogWatcher;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DISCORD_APP_ID: &str = "1474302474474094634";

fn connect_discord(app_id: &str) -> DiscordIpcClient {
    let mut client = DiscordIpcClient::new(app_id);
    loop {
        match client.connect() {
            Ok(_) => {
                log!("[discord] Connected!");
                return client;
            }
            Err(e) => {
                log!("[discord] Connect failed: {e}. Retrying in 10s...");
                thread::sleep(Duration::from_secs(10));
            }
        }
    }
}

fn build_activity<'a>(
    phase: GamePhase,
    hero_data: Option<&'a HeroData>,
    status: &'a str,
    start_time: Option<i64>,
) -> activity::Activity<'a> {
    let details: String = match hero_data {
        Some(d) => format!("Playing as {}", d.name),
        None => phase.description().to_string(),
    };

    let large_image = hero_data
        .filter(|d| !d.icon_url.is_empty())
        .map(|d| d.icon_url.as_str())
        .unwrap_or("deadlock_logo");
    let large_text = hero_data.map(|d| d.name.as_str()).unwrap_or("Deadlock");

    let assets = activity::Assets::new()
        .large_image(large_image)
        .large_text(large_text)
        .small_image("deadlock_logo")
        .small_text("Deadlock");

    let mut act = activity::Activity::new()
        .details(details)
        .state(status)
        .assets(assets);

    if let Some(ts) = start_time {
        act = act.timestamps(activity::Timestamps::new().start(ts));
    }

    act
}

/// Binds a local port to prevent multiple instances running simultaneously.
/// The OS releases the port automatically when the process exits.
fn acquire_single_instance_lock() -> Option<std::net::TcpListener> {
    match std::net::TcpListener::bind("127.0.0.1:47782") {
        Ok(l) => Some(l),
        Err(_) => {
            log!("[deadlock-rpc] Another instance is already running. Exiting.");
            std::process::exit(0);
        }
    }
}

fn exit_discord(client: &mut DiscordIpcClient) {
    let _ = client.clear_activity();
    let _ = client.close();
}

fn run_rpc_loop(state: Arc<Mutex<GameState>>, no_launch: bool) {
    log!("[discord] Connecting...");
    let mut client = connect_discord(DISCORD_APP_ID);
    let mut hero_cache = HeroCache::new();
    let mut last_state: Option<(GamePhase, MatchMode, Option<String>)> = None;
    let mut game_was_running = false;

    // If we launched the game, give it up to 2 minutes to appear before giving up.
    let launch_deadline = if !no_launch {
        Some(std::time::Instant::now() + Duration::from_secs(120))
    } else {
        None
    };

    loop {
        let (phase, match_mode, hero_key, start_time) = {
            let gs = state.lock().unwrap();
            (gs.phase, gs.match_mode, gs.hero_key.clone(), gs.game_start_time)
        };

        if phase != GamePhase::NotRunning {
            game_was_running = true;
        } else if game_was_running {
            log!("[deadlock-rpc] Game closed, exiting.");
            exit_discord(&mut client);
            std::process::exit(0);
        } else if let Some(deadline) = launch_deadline {
            if std::time::Instant::now() > deadline {
                log!("[deadlock-rpc] Game did not launch within 2 minutes, exiting.");
                exit_discord(&mut client);
                std::process::exit(0);
            }
        }

        let current = (phase, match_mode, hero_key.clone());
        if last_state.as_ref() == Some(&current) {
            thread::sleep(Duration::from_secs(5));
            continue;
        }

        let hero_data: Option<&HeroData> = if phase.shows_hero() {
            hero_key.as_deref().and_then(|k| hero_cache.get_or_fetch(k))
        } else {
            None::<&HeroData>
        };

        let hideout_text = hero_data.map(|d| d.hideout_text.as_str());
        let status = {
            let gs = state.lock().unwrap();
            gs.presence_status(hideout_text)
        };

        log!(
            "[rpc] phase={:?} hero={} status=\"{}\"",
            phase,
            hero_key.as_deref().unwrap_or("none"),
            status
        );

        let act = build_activity(phase, hero_data, &status, start_time);

        match client.set_activity(act) {
            Ok(_) => {
                last_state = Some(current);
            }
            Err(e) => {
                log!("[rpc] set_activity error: {e}. Reconnecting...");
                let _ = client.reconnect();
            }
        }

        thread::sleep(Duration::from_secs(5));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let no_launch = args.iter().any(|a| a == "--no-launch");

    logger::init();

    // Check for updates before acquiring the instance lock.
    // If an update is applied: on Linux exec() replaces this process in-place;
    // on Windows we exit before the port is ever bound — so no lock conflicts.
    updater::check_on_startup();

    // Ensure only one instance runs at a time.
    let _instance_lock = acquire_single_instance_lock();

    // Only install the shortcut in release builds so dev runs don't overwrite it with a debug path.
    #[cfg(not(debug_assertions))]
    launcher::install_shortcut();

    if !no_launch {
        launcher::launch_deadlock();
    }

    let log_path = steam::find_console_log();
    log!("[deadlock-rpc] Monitoring: {}", log_path.display());

    let state = Arc::new(Mutex::new(GameState::new()));

    {
        let state = Arc::clone(&state);
        thread::spawn(move || LogWatcher::new(log_path).run(state));
    }

    {
        let state = Arc::clone(&state);
        thread::spawn(move || run_rpc_loop(state, no_launch));
    }

    // Block the main thread on the tray icon for the lifetime of the process.
    // The only exit is the user clicking Quit, which calls process::exit.
    tray::run();
}
