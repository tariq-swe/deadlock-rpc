use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub presence: PresenceConfig,
    pub images: ImagesConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub launch_game_on_start: bool,
    pub exit_when_game_closes: bool,
    pub game_log_poll_interval_ms: u64,
    pub discord_update_interval_s: u64,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PresenceConfig {
    pub show_elapsed_timer: bool,
    pub show_hero_image: bool,
    pub show_statlocker_button: bool,
    pub details_with_hero: String,
    pub details_without_hero: String,
    pub status: StatusStrings,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct StatusStrings {
    pub game_not_running: String,
    pub in_main_menu: String,
    pub in_hideout: String,
    pub in_matchmaking: String,
    pub loading_into_match: String,
    pub in_match: String,
    pub match_location_label: String,
    pub post_match: String,
    pub spectating: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ImagesConfig {
    pub fallback_large_image: String,
    pub fallback_large_image_tooltip: String,
    pub corner_image: String,
    pub corner_image_tooltip: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            launch_game_on_start: true,
            exit_when_game_closes: true,
            game_log_poll_interval_ms: 500,
            discord_update_interval_s: 5,
        }
    }
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self {
            show_elapsed_timer: true,
            show_hero_image: true,
            show_statlocker_button: false,
            details_with_hero: "Playing as {hero}".to_string(),
            details_without_hero: "{phase}".to_string(),
            status: StatusStrings::default(),
        }
    }
}

impl Default for StatusStrings {
    fn default() -> Self {
        Self {
            game_not_running: "Not Running".to_string(),
            in_main_menu: "Browsing the Main Menu".to_string(),
            in_hideout: "In the Hideout".to_string(),
            in_matchmaking: "Searching for a Match...".to_string(),
            loading_into_match: "{mode} \u{2022} Loading into Match".to_string(),
            in_match: "In Match: {mode}".to_string(),
            match_location_label: "the Cursed Apple".to_string(),
            post_match: "Reviewing Match Results".to_string(),
            spectating: "Spectating a Match".to_string(),
        }
    }
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            fallback_large_image: "deadlock_logo".to_string(),
            fallback_large_image_tooltip: "Deadlock".to_string(),
            corner_image: "deadlock_logo".to_string(),
            corner_image_tooltip: "Deadlock RPC".to_string(),
        }
    }
}

pub fn apply_vars(template: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.toml")))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

// Recursively fills missing keys in `user` from `defaults`.
// Returns true if any key was added.
fn merge_defaults(user: &mut toml::Value, defaults: &toml::Value) -> bool {
    let (toml::Value::Table(user_table), toml::Value::Table(default_table)) = (user, defaults)
    else {
        return false;
    };
    let mut changed = false;
    for (key, default_val) in default_table {
        if let Some(user_val) = user_table.get_mut(key) {
            if merge_defaults(user_val, default_val) {
                changed = true;
            }
        } else {
            user_table.insert(key.clone(), default_val.clone());
            log::info!("[config] Added missing key '{key}' with default value");
            changed = true;
        }
    }
    changed
}

// Load config from `config.toml` next to the executable.
//
// - If the file does not exist: write a fully-documented default and return defaults.
// - If the file is malformed: log a warning and return defaults without overwriting.
// - If the file is a valid partial config: unset fields fall back to their defaults,
//   and any missing keys are written back to disk with their default values.
pub fn load() -> Config {
    let path = config_path();

    if !path.exists() {
        match std::fs::write(&path, DEFAULT_TOML) {
            Ok(_) => log::info!("[config] Created default config.toml at {}", path.display()),
            Err(e) => log::warn!("[config] Could not write default config.toml: {e}"),
        }
        return Config::default();
    }

    match std::fs::read_to_string(&path) {
        Err(e) => {
            log::warn!("[config] Could not read config.toml: {e} — using defaults");
            Config::default()
        }
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(cfg) => {
                patch_missing_keys(&path, &text);
                cfg
            }
            Err(e) => {
                log::warn!("[config] config.toml parse error: {e} — using defaults");
                Config::default()
            }
        },
    }
}

fn patch_missing_keys(path: &std::path::Path, text: &str) {
    let Ok(mut user_val) = toml::from_str::<toml::Value>(text) else {
        return;
    };
    let Ok(default_val) = toml::from_str::<toml::Value>(DEFAULT_TOML) else {
        return;
    };
    if !merge_defaults(&mut user_val, &default_val) {
        return;
    }
    match toml::to_string_pretty(&user_val) {
        Ok(new_text) => match std::fs::write(path, new_text) {
            Ok(_) => log::info!("[config] config.toml updated with new default keys"),
            Err(e) => log::warn!("[config] Could not update config.toml: {e}"),
        },
        Err(e) => log::warn!("[config] Could not serialize patched config: {e}"),
    }
}

const DEFAULT_TOML: &str = r#"[general]
launch_game_on_start = true
exit_when_game_closes = true
game_log_poll_interval_ms = 500
discord_update_interval_s = 5

[presence]
show_elapsed_timer = true
show_hero_image = true
show_statlocker_button = false
details_with_hero = "Playing as {hero}"
details_without_hero = "{phase}"

[presence.status]
game_not_running = "Not Running"
in_main_menu = "Browsing the Main Menu"
in_hideout = "In the Hideout"
in_matchmaking = "Searching for a Match..."
loading_into_match = "{mode} \u{2022} Loading into Match"
in_match = "In Match: {mode}"
match_location_label = "the Cursed Apple"
post_match = "Reviewing Match Results"
spectating = "Spectating a Match"

[images]
fallback_large_image = "deadlock_logo"
fallback_large_image_tooltip = "Deadlock"
corner_image = "deadlock_logo"
corner_image_tooltip = "Deadlock RPC"
"#;
