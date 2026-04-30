use crate::game_state::{GamePhase, GameState, MatchMode};
use log::{debug, info, warn};
use regex::Regex;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const HIDEOUT_MAPS: &[&str] = &["dl_hideout"];
const RESYNC_BYTES: u64 = 10 * 1024 * 1024;

struct Patterns {
    // Map / phase signals
    map_info: Regex,
    map_created_physics: Regex,
    host_activate: Regex,
    loop_mode_menu: Regex,
    change_game_state: Regex,
    precaching_heroes: Regex,
    // Match lifecycle
    lobby_created: Regex,
    lobby_destroyed: Regex,
    spectate_broadcast: Regex,
    // Server connection
    server_connect: Regex,
    server_disconnect: Regex,
    server_shutdown: Regex,
    // Hero signals
    loaded_hero: Regex,
    client_hero_vmdl: Regex,
    // Match mode inference
    player_info: Regex,
    bot_init: Regex,
    // Party tracking
    hideout_lobby_state: Regex,
    party_event: Regex,
    local_account_id: Regex,
}

impl Patterns {
    fn new() -> Self {
        Self {
            map_info: Regex::new(r#"\[Client\] Map:\s+"([^"]+)""#).unwrap(),
            map_created_physics: Regex::new(r"\[Client\] Created physics for\s+([^\s]+)").unwrap(),
            host_activate: Regex::new(r"\[HostStateManager\] Host activate:.*\(([^)]+)\)").unwrap(),
            loop_mode_menu: Regex::new(r"LoopMode:\s*menu").unwrap(),
            change_game_state: Regex::new(r"ChangeGameState:\s+(\w+)\s+\((\d+)\)").unwrap(),
            precaching_heroes: Regex::new(r"Precaching (\d+) heroes in CCitadelGameRules").unwrap(),
            lobby_created: Regex::new(r"Lobby\s+\d+\s+for\s+Match\s+\d+\s+created").unwrap(),
            lobby_destroyed: Regex::new(r"Lobby\s+\d+\s+for\s+Match\s+\d+\s+destroyed").unwrap(),
            spectate_broadcast: Regex::new(r"Playing Broadcast").unwrap(),
            server_connect: Regex::new(r"\[Client\] CL:\s+Connected to '([^']+)'").unwrap(),
            server_disconnect: Regex::new(r"\[Client\] Disconnecting from server:\s+(\S+)").unwrap(),
            server_shutdown: Regex::new(r"\[Server\] SV:\s+Server shutting down:\s+(\S+)").unwrap(),
            loaded_hero: Regex::new(r"\[Server\] Loaded hero \d+/(hero_\w+)").unwrap(),
            client_hero_vmdl: Regex::new(r"VMDL Camera Pose Success!.*models/heroes(?:_wip|_staging)?/(\w+)/").unwrap(),
            player_info: Regex::new(r"\[Client\] Players:\s+(\d+)\s+\(\d+ bots\)\s+/\s+\d+ humans").unwrap(),
            bot_init: Regex::new(r"Initializing bot for player slot \d+:\s+k_ECitadelBotDifficulty_\w+").unwrap(),
            hideout_lobby_state: Regex::new(r"\[Hideout\] Hideout Lobby Connection State:\s+(\w+)\s+\((-?\d+)\)").unwrap(),
            party_event: Regex::new(r"CMsgGCToClientPartyEvent:\s+\{\s*party_id:\s+(\d+)\s+event:\s+(k_e\w+)\s+initiator_account_id:\s+(\d+)\s*\}").unwrap(),
            local_account_id: Regex::new(r"\[U:1:(\d+)\]").unwrap(),
        }
    }
}

/// Seconds of log silence during an active match before assuming a crash.
/// Only applied during InMatch/MatchIntro — passive phases (menu, hideout, queue)
/// produce very little log output and would cause false positives for AFK users.
const MATCH_STALE_SECS: u64 = 300;

pub struct LogWatcher {
    log_path: PathBuf,
    /// Milliseconds between log file polls (from config).
    poll_interval_ms: u64,
}

impl LogWatcher {
    pub fn new(log_path: PathBuf, poll_interval_ms: u64) -> Self {
        Self {
            log_path,
            poll_interval_ms,
        }
    }

    pub fn run(&self, state: Arc<Mutex<GameState>>) {
        let patterns = Patterns::new();
        let mut last_pos: u64 = 0;
        let mut initialized = false;
        let mut last_activity = std::time::Instant::now();

        loop {
            if !self.log_path.exists() {
                if initialized {
                    info!("[watcher] Log file gone — game closed, resetting state");
                    let mut gs = state.lock().unwrap();
                    gs.reset();
                    initialized = false;
                    last_pos = 0;
                }
                thread::sleep(Duration::from_secs(2));
                continue;
            }

            let file_size = std::fs::metadata(&self.log_path)
                .map(|m| m.len())
                .unwrap_or(0);

            if !initialized {
                let start = file_size.saturating_sub(RESYNC_BYTES);
                let lines = read_lines_from(&self.log_path, start, start > 0);
                log_last_instances(&lines, &patterns);
                let mut gs = state.lock().unwrap();
                gs.enter_main_menu();
                for line in &lines {
                    process_line(line.trim(), &mut gs, &patterns);
                }
                info!(
                    "[resync] phase={:?} map={:?} mode={:?} hero={:?} party={} ({} lines)",
                    gs.phase, gs.map_name, gs.match_mode, gs.hero_key, gs.party_size, lines.len()
                );

                last_pos = file_size;
                last_activity = std::time::Instant::now();
                initialized = true;
                continue;
            }

            if file_size < last_pos {
                warn!("[watcher] Log truncated, resyncing...");
                initialized = false;
                last_pos = 0;
                continue;
            }

            if file_size > last_pos {
                last_activity = std::time::Instant::now();
                let lines = read_lines_from(&self.log_path, last_pos, false);

                if !lines.is_empty() {
                    let mut gs = state.lock().unwrap();
                    let prev_hero = gs.hero_key.clone();
                    let prev_phase = gs.phase;
                    let prev_party = gs.party_size;
                    for line in &lines {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            process_line(trimmed, &mut gs, &patterns);
                        }
                    }
                    if gs.phase != prev_phase {
                        info!("[state] {:?} → {:?}", prev_phase, gs.phase);
                    }
                    if gs.hero_key != prev_hero {
                        info!("[hero]  {:?} → {:?}", prev_hero, gs.hero_key);
                    }
                    if gs.party_size != prev_party {
                        info!("[party] {} → {}", prev_party, gs.party_size);
                    }
                }

                last_pos = file_size;
            } else {
                // No new bytes — only flag a crash if we're in an active match phase.
                // Passive phases (menu, hideout, queue) produce little log output
                // and would cause false positives for AFK users.
                let gs_phase = state.lock().unwrap().phase;
                let in_active_match = matches!(
                    gs_phase,
                    GamePhase::InMatch | GamePhase::MatchIntro
                );
                if in_active_match
                    && last_activity.elapsed() > Duration::from_secs(MATCH_STALE_SECS)
                {
                    warn!(
                        "[watcher] No log activity for {}s during active match — assuming crash",
                        MATCH_STALE_SECS
                    );
                    let mut gs = state.lock().unwrap();
                    gs.reset();
                    initialized = false;
                    last_pos = 0;
                    last_activity = std::time::Instant::now();
                }
            }

            thread::sleep(Duration::from_millis(self.poll_interval_ms));
        }
    }
}

/// Opens the log file, seeks to `offset`, and returns all complete lines from that point.
/// If `skip_partial` is true, discards the first (potentially incomplete) line after seeking.
///
/// Uses `from_utf8_lossy` to replace invalid bytes (e.g. non-ASCII player names) with
/// U+FFFD rather than stopping iteration — equivalent to Python's `errors="replace"`.
fn read_lines_from(path: &std::path::Path, offset: u64, skip_partial: bool) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok();

    let content = String::from_utf8_lossy(&bytes);
    let mut lines = content.lines();
    if skip_partial {
        lines.next(); // discard the incomplete line at the seek boundary
    }
    lines.map(str::to_owned).collect()
}

/// Normalise a raw hero key from the log: lowercase, strip version suffix (_v2, _v3, …),
/// and ensure a hero_ prefix so the result is always a valid API class_name.
fn normalize_hero_key(raw: &str) -> String {
    let s = raw.to_lowercase();
    // Strip trailing version suffix
    let s = if let Some(pos) = s.rfind('_') {
        let suffix = &s[pos + 1..];
        let is_version = suffix.starts_with('v')
            && suffix.len() > 1
            && suffix[1..].bytes().all(|b| b.is_ascii_digit());
        if is_version { s[..pos].to_string() } else { s }
    } else {
        s
    };
    // Ensure hero_ prefix
    if s.starts_with("hero_") { s } else { format!("hero_{s}") }
}

/// Finalise match mode once we have both a map name and a stable player count.
/// Only acts when mode is still Unknown — safe to call multiple times.
fn try_infer_mode(state: &mut GameState) {
    if state.match_mode != MatchMode::Unknown || state.pending_player_count == 0 {
        return;
    }
    if state.map_name.as_deref() == Some("dl_midtown") {
        state.match_mode = if state.pending_player_count >= 9 {
            MatchMode::Standard
        } else if state.pending_player_count >= 4 {
            MatchMode::StreetBrawl
        } else {
            return; // too few players to determine mode yet
        };
        info!("[mode] inferred {:?} from dl_midtown + {} players", state.match_mode, state.pending_player_count);
    }
}

fn apply_map(state: &mut GameState, map_name: &str) {
    if state.phase == GamePhase::Spectating {
        return;
    }

    let map_lower = map_name.to_lowercase();
    if map_lower.is_empty() || map_lower == "<empty>" {
        return;
    }

    if HIDEOUT_MAPS.contains(&map_lower.as_str()) {
        state.enter_hideout();
        state.map_name = Some(map_lower);
        return;
    }

    state.map_name = Some(map_lower.clone());

    // Reset mode on every map change so it gets re-derived for the new map.
    state.match_mode = MatchMode::Unknown;

    // Set mode from map name where it's unambiguous
    match map_lower.as_str() {
        "new_player_basics" => state.match_mode = MatchMode::TrainingRange,
        "street_test" | "street_test_bridge" => state.match_mode = MatchMode::Standard,
        _ => {}
    }

    // Any non-hideout named map means a match is starting/restarting.
    if matches!(
        state.phase,
        GamePhase::MatchIntro
            | GamePhase::InQueue
            | GamePhase::MainMenu
            | GamePhase::Hideout
            | GamePhase::InMatch
            | GamePhase::PostMatch
    ) {
        state.phase = GamePhase::InMatch;
        state.prepare_match_hero_tracking();
        state.hideout_loaded = false;
        // Handle the case where player_info arrived before map_created_physics.
        try_infer_mode(state);
    }
}

/// Scans the buffered log lines in reverse to find and log the last raw signal
/// for each key RPC field (map, phase, match mode, hero). Run before replay so
/// the derived state can be cross-checked against what was actually seen in the log.
fn log_last_instances(lines: &[String], p: &Patterns) {
    let mut found_map = false;
    let mut found_phase = false;
    let mut found_mode = false;
    let mut found_hero = false;

    for line in lines.iter().rev() {
        let line = line.trim();

        if !found_map {
            if let Some(m) = p.map_created_physics.captures(line) {
                info!("[startup] last map:   {:?} (map_created_physics)", m.get(1).unwrap().as_str());
                found_map = true;
            } else if let Some(m) = p.map_info.captures(line) {
                info!("[startup] last map:   {:?} (map_info)", m.get(1).unwrap().as_str());
                found_map = true;
            }
        }

        if !found_phase {
            if let Some(m) = p.change_game_state.captures(line) {
                info!(
                    "[startup] last phase: ChangeGameState {} ({})",
                    m.get(1).unwrap().as_str(),
                    m.get(2).unwrap().as_str()
                );
                found_phase = true;
            } else if p.lobby_created.is_match(line) {
                info!("[startup] last phase: lobby created");
                found_phase = true;
            } else if p.lobby_destroyed.is_match(line) {
                info!("[startup] last phase: lobby destroyed");
                found_phase = true;
            } else if p.spectate_broadcast.is_match(line) {
                info!("[startup] last phase: spectate broadcast");
                found_phase = true;
            } else if line.contains("k_EMsgClientToGCStartMatchmaking") {
                info!("[startup] last phase: StartMatchmaking");
                found_phase = true;
            } else if line.contains("k_EMsgClientToGCStopMatchmaking") {
                info!("[startup] last phase: StopMatchmaking");
                found_phase = true;
            }
        }

        if !found_mode {
            if let Some(m) = p.player_info.captures(line) {
                info!("[startup] last mode:  {} players (player_info)", m.get(1).unwrap().as_str());
                found_mode = true;
            } else if p.bot_init.is_match(line) {
                info!("[startup] last mode:  bot match init");
                found_mode = true;
            }
        }

        if !found_hero {
            if let Some(m) = p.loaded_hero.captures(line) {
                info!(
                    "[startup] last hero:  {:?} (server loaded_hero)",
                    normalize_hero_key(m.get(1).unwrap().as_str())
                );
                found_hero = true;
            } else if let Some(m) = p.client_hero_vmdl.captures(line) {
                info!(
                    "[startup] last hero:  {:?} (client vmdl)",
                    normalize_hero_key(m.get(1).unwrap().as_str())
                );
                found_hero = true;
            }
        }

        if found_map && found_phase && found_mode && found_hero {
            break;
        }
    }

    if !found_map   { info!("[startup] last map:   none"); }
    if !found_phase { info!("[startup] last phase: none"); }
    if !found_mode  { info!("[startup] last mode:  none"); }
    if !found_hero  { info!("[startup] last hero:  none"); }
}

// ── Event dispatch ────────────────────────────────────────────────────────────

/// Parsed representation of a single exclusive log event.
/// `parse_event` produces this; `apply_event` consumes it.
enum Event {
    MapCreated(String),
    MapInfo(String),
    StartMatchmaking,
    StopMatchmaking,
    LobbyCreated,
    LobbyDestroyed,
    Spectating,
    ServerConnected(String),    // lowercase addr
    HeroLoaded(String),         // normalized key
    HeroVmdl(String),           // normalized key
    ServerDisconnected(String), // uppercase reason
    LoopModeMenu,
    GameStateChanged { name: String, id: u32 },
    HideoutLobbyState { lobby_id: i64 },
    HostActivate(String),       // lowercase map name
    ServerShutdown(String),     // uppercase reason
    AppShutdown,
    PrecachingHeroes(u32),
    PlayerInfo(u32),
    BotInit,
}

/// Pure pattern match: returns the first exclusive event found in `line`, or `None`.
/// No state is read or written here — only the line text and compiled patterns are used.
fn parse_event(line: &str, p: &Patterns) -> Option<Event> {
    if let Some(m) = p.map_created_physics.captures(line) {
        return Some(Event::MapCreated(m[1].to_owned()));
    }
    if let Some(m) = p.map_info.captures(line) {
        let map = m[1].to_owned();
        // "start" is a transient engine placeholder, not an actual map transition.
        if map != "start" {
            return Some(Event::MapInfo(map));
        }
    }
    if line.contains("k_EMsgClientToGCStartMatchmaking") {
        return Some(Event::StartMatchmaking);
    }
    if line.contains("k_EMsgClientToGCStopMatchmaking") {
        return Some(Event::StopMatchmaking);
    }
    if p.lobby_created.is_match(line) {
        return Some(Event::LobbyCreated);
    }
    if p.lobby_destroyed.is_match(line) {
        return Some(Event::LobbyDestroyed);
    }
    if p.spectate_broadcast.is_match(line) {
        return Some(Event::Spectating);
    }
    if let Some(m) = p.server_connect.captures(line) {
        return Some(Event::ServerConnected(m[1].to_lowercase()));
    }
    if let Some(m) = p.loaded_hero.captures(line) {
        return Some(Event::HeroLoaded(normalize_hero_key(&m[1])));
    }
    if let Some(m) = p.client_hero_vmdl.captures(line) {
        return Some(Event::HeroVmdl(normalize_hero_key(&m[1])));
    }
    if let Some(m) = p.server_disconnect.captures(line) {
        return Some(Event::ServerDisconnected(m[1].to_uppercase()));
    }
    if p.loop_mode_menu.is_match(line) {
        return Some(Event::LoopModeMenu);
    }
    if let Some(m) = p.change_game_state.captures(line) {
        return Some(Event::GameStateChanged {
            name: m[1].to_lowercase(),
            id:   m[2].parse().unwrap_or(0),
        });
    }
    if let Some(m) = p.hideout_lobby_state.captures(line) {
        return Some(Event::HideoutLobbyState { lobby_id: m[2].parse().unwrap_or(-1) });
    }
    if let Some(m) = p.host_activate.captures(line) {
        return Some(Event::HostActivate(m[1].to_lowercase()));
    }
    if let Some(m) = p.server_shutdown.captures(line) {
        return Some(Event::ServerShutdown(m[1].to_uppercase()));
    }
    if line.contains("Dispatching EventAppShutdown_t") || line.contains("Source2Shutdown") {
        return Some(Event::AppShutdown);
    }
    if let Some(m) = p.precaching_heroes.captures(line) {
        return Some(Event::PrecachingHeroes(m[1].parse().unwrap_or(0)));
    }
    if let Some(m) = p.player_info.captures(line) {
        return Some(Event::PlayerInfo(m[1].parse().unwrap_or(0)));
    }
    if p.bot_init.is_match(line) {
        return Some(Event::BotInit);
    }
    None
}

/// Applies a parsed event to the game state.
fn apply_event(event: Event, state: &mut GameState, is_hideout_map: bool) {
    match event {
        Event::MapCreated(map) => {
            debug!("[dbg] matched map_created_physics: {:?}", map);
            apply_map(state, &map);
        }
        Event::MapInfo(map) => {
            debug!("[dbg] matched map_info: {:?}", map);
            apply_map(state, &map);
        }
        Event::StartMatchmaking => {
            debug!("[dbg] matched StartMatchmaking, phase={:?}", state.phase);
            if matches!(state.phase, GamePhase::Hideout | GamePhase::MainMenu) {
                state.enter_queue();
            }
        }
        Event::StopMatchmaking => {
            debug!("[dbg] matched StopMatchmaking, phase={:?}", state.phase);
            if state.phase == GamePhase::InQueue {
                state.leave_queue();
            }
        }
        Event::LobbyCreated => {
            debug!("[dbg] matched lobby_created, phase={:?}", state.phase);
            state.prepare_match_hero_tracking();
            if matches!(
                state.phase,
                GamePhase::MainMenu | GamePhase::Hideout | GamePhase::InQueue | GamePhase::MatchIntro
            ) {
                state.enter_match_intro();
            }
        }
        Event::LobbyDestroyed => {
            debug!("[dbg] matched lobby_destroyed");
            state.end_match();
        }
        Event::Spectating => {
            debug!("[dbg] matched spectate_broadcast");
            state.enter_spectating();
            state.hideout_loaded = false;
        }
        Event::ServerConnected(addr) => {
            let is_real = !addr.contains("loopback");
            debug!("[dbg] matched server_connect: addr={addr:?} is_real={is_real}");
            if is_real {
                state.prepare_match_hero_tracking();
                if matches!(
                    state.phase,
                    GamePhase::MainMenu | GamePhase::Hideout | GamePhase::InQueue | GamePhase::MatchIntro
                ) {
                    state.enter_match_intro();
                }
            }
        }
        Event::HeroLoaded(hero) => {
            let is_hideout = matches!(state.phase, GamePhase::Hideout);
            debug!("[dbg] matched loaded_hero: hero={hero:?} is_hideout={is_hideout} hideout_loaded={}", state.hideout_loaded);
            if !is_hideout || state.hideout_loaded {
                state.apply_hero_signal(&hero);
            }
        }
        Event::HeroVmdl(hero) => {
            debug!("[dbg] matched client_hero_vmdl: hero={hero:?}");
            state.apply_hero_signal(&hero);
        }
        Event::ServerDisconnected(reason) => {
            debug!("[dbg] matched server_disconnect: reason={reason:?}");
            if reason.contains("EXITING") {
                state.reset();
            } else if !reason.contains("LOOPDEACTIVATE")
                && matches!(
                    state.phase,
                    GamePhase::InMatch | GamePhase::MatchIntro | GamePhase::Spectating
                )
            {
                state.end_match();
            }
        }
        Event::LoopModeMenu => {
            debug!("[dbg] matched loop_mode_menu, phase={:?}", state.phase);
            if matches!(
                state.phase,
                GamePhase::InMatch | GamePhase::MatchIntro | GamePhase::Spectating
            ) {
                state.end_match();
            }
        }
        Event::GameStateChanged { name, id } => {
            debug!("[dbg] matched change_game_state: name={name:?} id={id} phase={:?} is_hideout_map={is_hideout_map}", state.phase);
            if state.phase != GamePhase::Spectating && !is_hideout_map && !state.hideout_loaded {
                if name == "matchintro" || id == 4 {
                    state.enter_match_intro();
                } else if name == "gameinprogress" || name == "inprogress" || id == 7 {
                    state.start_match();
                } else if name == "postgame" || id == 6 {
                    state.end_match();
                }
            }
        }
        Event::HideoutLobbyState { lobby_id } => {
            // lobby_id == 0 means the player is solo (no active party lobby).
            if lobby_id == 0 && matches!(state.phase, GamePhase::Hideout) {
                state.clear_party();
            }
        }
        Event::HostActivate(map) => {
            debug!("[dbg] matched host_activate: map={map:?}");
            if HIDEOUT_MAPS.contains(&map.as_str()) {
                state.hideout_loaded = true;
            }
        }
        Event::ServerShutdown(reason) => {
            debug!("[dbg] matched server_shutdown: reason={reason:?}");
            if reason.contains("EXITING") {
                state.reset();
            }
        }
        Event::AppShutdown => {
            debug!("[dbg] matched app shutdown signal");
            state.reset();
        }
        Event::PrecachingHeroes(count) => {
            debug!("[dbg] matched precaching_heroes: count={count}");
            if count > 0 {
                state.hideout_loaded = false;
            }
        }
        Event::PlayerInfo(count) => {
            if matches!(state.phase, GamePhase::MatchIntro | GamePhase::InMatch) {
                debug!("[dbg] matched player_info: count={count} pending={} mode={:?}", state.pending_player_count, state.match_mode);
                if count > 0 {
                    state.pending_player_count = state.pending_player_count.max(count);
                }
                // Only finalise mode once the match is actually in progress.
                if state.phase == GamePhase::InMatch {
                    try_infer_mode(state);
                }
            }
        }
        Event::BotInit => {
            // Hideout spawns target-practice bots — don't let them poison match_mode.
            if matches!(state.phase, GamePhase::MatchIntro | GamePhase::InMatch)
                && state.match_mode == MatchMode::Unknown
            {
                debug!("[dbg] matched bot_init");
                state.match_mode = MatchMode::BotMatch;
            }
        }
    }
}

fn process_line(line: &str, state: &mut GameState, p: &Patterns) {
    // Always-run: these can co-appear on the same line as exclusive events.

    if state.local_account_id.is_none() {
        if let Some(m) = p.local_account_id.captures(line) {
            let id: u64 = m[1].parse().unwrap_or(0);
            if id > 0 {
                debug!("[dbg] captured local_account_id: {}", id);
                state.local_account_id = Some(id);
            }
        }
    }

    if let Some(m) = p.party_event.captures(line) {
        let party_id: u64 = m[1].parse().unwrap_or(0);
        let account_id: u64 = m[3].parse().unwrap_or(0);
        state.apply_party_event(party_id, &m[2], account_id);
    }

    // Exclusive: first matching pattern claims the line.
    let is_hideout_map = state.map_name.as_deref()
        .map(|m| HIDEOUT_MAPS.contains(&m))
        .unwrap_or(false);

    if let Some(event) = parse_event(line, p) {
        apply_event(event, state, is_hideout_map);
    }
}
