#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod config;
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
    details: &'a str,
    hero_data: Option<&'a HeroData>,
    state: Option<&'a str>,
    start_time: Option<i64>,
    show_party: bool,
    party_size: u8,
    img_cfg: &'a config::ImagesConfig,
    show_elapsed_timer: bool,
) -> activity::Activity<'a> {
    let large_image = hero_data
        .filter(|d| !d.icon_url.is_empty())
        .map(|d| d.icon_url.as_str())
        .unwrap_or(img_cfg.default_large_image.as_str());

    let large_text = hero_data
        .map(|d| d.name.as_str())
        .unwrap_or(img_cfg.default_large_text.as_str());

    let assets = activity::Assets::new()
        .large_image(large_image)
        .large_text(large_text)
        .small_image(img_cfg.small_image.as_str())
        .small_text(img_cfg.small_text.as_str());

    let mut act = activity::Activity::new()
        .details(details)
        .assets(assets);

    if let Some(s) = state {
        act = act.state(s);
    }

    if show_elapsed_timer {
        if let Some(ts) = start_time {
            act = act.timestamps(activity::Timestamps::new().start(ts));
        }
    }

    if show_party {
        act = act.party(activity::Party::new().size([party_size as i32, 6]));
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

fn run_rpc_loop(state: Arc<Mutex<GameState>>, no_launch: bool, cfg: config::Config) {
    log!("[discord] Connecting...");
    let mut client = connect_discord(DISCORD_APP_ID);
    let mut hero_cache = HeroCache::new();
    let mut last_state: Option<(GamePhase, MatchMode, Option<String>, u8)> = None;
    let mut game_was_running = false;

    // If we launched the game, give it up to `launch_timeout_s` to appear before giving up.
    let launch_deadline = if !no_launch {
        Some(std::time::Instant::now() + Duration::from_secs(cfg.general.launch_timeout_s))
    } else {
        None
    };

    let update_interval = Duration::from_secs(cfg.general.presence_update_interval_s);

    loop {
        let (phase, match_mode, hero_key, start_time, party_size) = {
            let gs = state.lock().unwrap();
            (gs.phase, gs.match_mode, gs.hero_key.clone(), gs.game_start_time, gs.party_size)
        };

        if phase != GamePhase::NotRunning {
            game_was_running = true;
        } else if game_was_running {
            log!("[deadlock-rpc] Game closed.");
            if cfg.general.auto_exit {
                exit_discord(&mut client);
std::process::exit(0);
            }
        } else if let Some(deadline) = launch_deadline {
            if std::time::Instant::now() > deadline {
                log!(
                    "[deadlock-rpc] Game did not launch within {}s, exiting.",
                    cfg.general.launch_timeout_s
                );
                exit_discord(&mut client);
                std::process::exit(0);
            }
        }

        let current = (phase, match_mode, hero_key.clone(), party_size);
        if last_state.as_ref() == Some(&current) {
            thread::sleep(update_interval);
            continue;
        }

        // Respect show_hero: if disabled, never pass hero data for display.
        let effective_hero_data: Option<&HeroData> =
            if cfg.presence.show_hero && phase.shows_hero() {
                hero_key.as_deref().and_then(|k| hero_cache.get_or_fetch(k))
            } else {
                None
            };

        // Clone the name so we can drop the hero_cache borrow before locking state.
        let hero_name_owned: Option<String> = effective_hero_data.map(|d| d.name.clone());
        let hero_name: Option<&str> = hero_name_owned.as_deref();

        let hideout_text: Option<&str> = effective_hero_data.map(|d| d.hideout_text.as_str());

        let game_status: String = {
            let gs = state.lock().unwrap();
            gs.presence_status(hideout_text, hero_name, &cfg.presence.status)
        };

        let hero_label: String = match hero_name {
            Some(name) => config::apply_vars(&cfg.presence.details_with_hero, &[("hero", name)]),
            None => config::apply_vars(
                &cfg.presence.details_no_hero,
                &[("phase", phase.description())],
            ),
        };

        // Party is only shown in the Hideout.
        let show_party = phase == GamePhase::Hideout && party_size > 1;

        // Hideout: hideout text on top, party line on bottom (or nothing if solo).
        // All other phases: hero/phase label on top, game status on bottom, no party.
        let (details, state_opt): (&str, Option<&str>) = if phase == GamePhase::Hideout {
            let s = if show_party { Some("In a Party") } else { None };
            (game_status.as_str(), s)
        } else {
            (hero_label.as_str(), Some(game_status.as_str()))
        };

        log!(
            "[rpc] phase={:?} hero={} details=\"{}\"",
            phase,
            hero_key.as_deref().unwrap_or("none"),
            details
        );

        let act = build_activity(
            details,
            effective_hero_data,
            state_opt,
            start_time,
            show_party,
            party_size,
            &cfg.images,
            cfg.presence.show_elapsed_timer,
        );

        match client.set_activity(act) {
            Ok(_) => {
                last_state = Some(current);
            }
            Err(e) => {
                log!("[rpc] set_activity error: {e}. Reconnecting...");
                let _ = client.reconnect();
            }
        }

        thread::sleep(update_interval);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for updates before acquiring the instance lock.
    // If an update is applied: on Linux exec() replaces this process in-place;
    // on Windows we exit before the port is ever bound — so no lock conflicts.
    updater::check_on_startup();

    // Ensure only one instance runs at a time.
    let _instance_lock = acquire_single_instance_lock();

    logger::init();

    let cfg = config::load();
    log!("[config] Loaded from config.toml");

    // --no-launch CLI flag always overrides auto_launch, even if config enables it.
    let no_launch = args.iter().any(|a| a == "--no-launch") || !cfg.general.auto_launch;

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
        let poll_ms = cfg.general.log_poll_interval_ms;
        thread::spawn(move || LogWatcher::new(log_path, poll_ms).run(state));
    }

    {
        let state = Arc::clone(&state);
        thread::spawn(move || run_rpc_loop(state, no_launch, cfg));
    }

    // Block the main thread on the tray icon for the lifetime of the process.
    // The only exit is the user clicking Quit, which calls process::exit.
    tray::run();
}
