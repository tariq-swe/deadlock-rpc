#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod config;
mod game_state;
mod hero_api;
mod launcher;
mod logger;
mod log_watcher;
mod process_watcher;
mod steam;
mod tray;
mod updater;

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use game_state::{GamePhase, GameState, MatchMode};
use hero_api::{HeroCache, HeroData};
use log::{info, warn};
use log_watcher::LogWatcher;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DISCORD_APP_ID: &str = "1474302474474094634";

type LastRpcState = (GamePhase, MatchMode, Option<String>, u8, Option<String>, Option<u64>);

fn connect_discord(app_id: &str) -> DiscordIpcClient {
    let mut client = DiscordIpcClient::new(app_id);
    loop {
        match client.connect() {
            Ok(_) => {
                info!("[discord] Connected!");
                return client;
            }
            Err(e) => {
                warn!("[discord] Connect failed: {e}. Retrying in 10s...");
                thread::sleep(Duration::from_secs(10));
            }
        }
    }
}

struct StatlockerOpts {
    account_id: Option<u64>,
    show_button: bool,
}

fn build_activity<'a>(
    details: &'a str,
    hero_data: Option<&'a HeroData>,
    state: Option<&'a str>,
    start_time: Option<i64>,
    party_size: Option<u8>,
    img_cfg: &'a config::ImagesConfig,
    statlocker: StatlockerOpts,
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

    if let Some(ts) = start_time {
        act = act.timestamps(activity::Timestamps::new().start(ts));
    }

    if let Some(size) = party_size {
        act = act.party(activity::Party::new().size([size as i32, 6]));
    }

    if statlocker.show_button {
        if let Some(id) = statlocker.account_id {
            let url = format!("https://statlocker.gg/profile/{}/matches", id);
            act = act.buttons(vec![activity::Button::new("View on Statlocker", url)]);
        }
    }

    act
}

/// Binds a local port to prevent multiple instances running simultaneously.
/// The OS releases the port automatically when the process exits.
/// Returns None if another instance already holds the port.
fn try_acquire_single_instance_lock() -> Option<std::net::TcpListener> {
    std::net::TcpListener::bind("127.0.0.1:47782").ok()
}

fn exit_discord(client: &mut DiscordIpcClient) {
    let _ = client.clear_activity();
    let _ = client.close();
}

fn run_rpc_loop(state: Arc<Mutex<GameState>>, cfg: config::Config) {
    info!("[discord] Connecting...");
    let mut client = connect_discord(DISCORD_APP_ID);
    let mut hero_cache = HeroCache::new();
    let mut last_state: Option<LastRpcState> = None;
    let mut game_was_running = false;

    let rpc_start_time: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let update_interval = Duration::from_secs(cfg.general.presence_update_interval_s);

    loop {
        let (phase, match_mode, hero_key, party_size, map_name, account_id) = {
            let gs = state.lock().unwrap();
            (gs.phase, gs.match_mode, gs.hero_key.clone(), gs.party_size, gs.map_name.clone(), gs.local_account_id)
        };

        if phase != GamePhase::NotRunning {
            game_was_running = true;
        } else if game_was_running {
            info!("[deadlock-rpc] Game closed.");
            if cfg.general.auto_exit {
                exit_discord(&mut client);
std::process::exit(0);
            }
        }

        let current = (phase, match_mode, hero_key.clone(), party_size, map_name, account_id);
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
        // NotRunning: status text only, no second line.
        // All other phases: hero/phase label on top, game status on bottom, no party.
        let (details, state_opt): (&str, Option<&str>) = if phase == GamePhase::Hideout {
            let s = if show_party { Some("In a Party") } else { None };
            (game_status.as_str(), s)
        } else if phase == GamePhase::NotRunning || phase == GamePhase::Spectating {
            (game_status.as_str(), None)
        } else {
            (hero_label.as_str(), Some(game_status.as_str()))
        };

        info!(
            "[rpc] {{\n  \"phase\": \"{:?}\",\n  \"hero\": \"{}\",\n  \"details\": \"{}\",\n  \"state\": \"{}\",\n  \"party_size\": {},\n  \"account_id\": {},\n  \"statlocker_button\": \"{}\"\n}}",
            phase,
            hero_key.as_deref().unwrap_or("none"),
            details,
            state_opt.unwrap_or("none"),
            party_size,
            account_id.map_or("null".to_string(), |id| id.to_string()),
            if cfg.presence.show_statlocker_button {
                if account_id.is_some() { "enabled" } else { "enabled (awaiting Steam ID)" }
            } else {
                "disabled"
            }
        );

        let elapsed_start = if cfg.presence.show_elapsed_timer { Some(rpc_start_time) } else { None };
        let party = if show_party { Some(party_size) } else { None };
        let act = build_activity(
            details,
            effective_hero_data,
            state_opt,
            elapsed_start,
            party,
            &cfg.images,
            StatlockerOpts { account_id, show_button: cfg.presence.show_statlocker_button },
        );

        match client.set_activity(act) {
            Ok(_) => {
                last_state = Some(current);
            }
            Err(e) => {
                warn!("[rpc] set_activity error: {e}. Reconnecting...");
                let _ = client.reconnect();
            }
        }

        thread::sleep(update_interval);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    logger::init();

    // Debug-only: --simulate-update fakes the full update flow then re-execs.
    #[cfg(debug_assertions)]
    if args.iter().any(|a| a == "--simulate-update") {
        updater::simulate_update();
    }

    // Check for updates before acquiring the instance lock.
    // If an update is applied: on Linux exec() replaces this process in-place;
    // on Windows we exit before the port is ever bound — so no lock conflicts.
    updater::check_on_startup();

    let instance_lock = try_acquire_single_instance_lock();

    let cfg = config::load();
    info!("[config] Loaded from config.toml");

    let no_launch_flag = args.iter().any(|a| a == "--no-launch");
    // --no-launch CLI flag always overrides auto_launch, even if config enables it.
    let no_launch = no_launch_flag || !cfg.general.auto_launch;

    if instance_lock.is_none() {
        if !no_launch_flag {
            info!("[deadlock-rpc] Another instance is running — re-triggering launch (Steam may be updating).");
            launcher::launch_deadlock();
        } else {
            info!("[deadlock-rpc] Another instance is already running. Exiting.");
        }
        std::process::exit(0);
    }
    let _instance_lock = instance_lock;

    // Only install the shortcut in release builds so dev runs don't overwrite it with a debug path.
    #[cfg(not(debug_assertions))]
    launcher::install_shortcut();

    if !no_launch {
        launcher::launch_deadlock();
    }

    let log_path = steam::find_console_log();
    info!("[deadlock-rpc] Monitoring: {}", log_path.display());

    let state = Arc::new(Mutex::new(GameState::new()));

    {
        let state = Arc::clone(&state);
        let poll_ms = cfg.general.log_poll_interval_ms;
        thread::spawn(move || LogWatcher::new(log_path, poll_ms).run(state));
    }

    {
        let state = Arc::clone(&state);
        thread::spawn(move || process_watcher::ProcessWatcher::run(state));
    }

    {
        let state = Arc::clone(&state);
        thread::spawn(move || run_rpc_loop(state, cfg));
    }

    // Block the main thread on the tray icon for the lifetime of the process.
    // The only exit is the user clicking Quit, which calls process::exit.
    tray::run();
}
