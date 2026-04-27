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
    /// Launch Deadlock automatically when deadlock-rpc starts.
    /// The --no-launch CLI flag overrides this to false for a single run.
    pub auto_launch: bool,
    /// Exit deadlock-rpc when the game closes.
    pub auto_exit: bool,
    /// Milliseconds between game log polls.
    pub log_poll_interval_ms: u64,
    /// Seconds between Discord presence refreshes.
    pub presence_update_interval_s: u64,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PresenceConfig {
    /// Show the elapsed session timer in Discord.
    pub show_elapsed_timer: bool,
    /// Show the hero icon and "Playing as" text.
    /// When false, the Deadlock logo is always shown.
    pub show_hero: bool,
    /// Top "details" line when a hero is known.
    /// Variables: {hero}
    pub details_with_hero: String,
    /// Top "details" line when no hero is known.
    /// Variables: {phase}
    pub details_no_hero: String,
    pub status: StatusStrings,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct StatusStrings {
    pub not_running: String,
    pub main_menu: String,
    /// Hideout fallback — used when the API provides no hero-specific text.
    /// Variables: {hero}
    pub in_hideout: String,
    pub in_queue: String,
    /// Variables: {mode}
    pub match_intro: String,
    /// Variables: {mode}, {location}
    pub in_match: String,
    /// The value substituted for {location} in in_match.
    pub in_match_location: String,
    pub post_match: String,
    pub spectating: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ImagesConfig {
    /// Discord asset key for the large image when no hero image is available.
    pub default_large_image: String,
    /// Hover text for the large image when no hero is shown.
    pub default_large_text: String,
    /// Discord asset key for the small corner image.
    pub small_image: String,
    /// Hover text for the small corner image.
    pub small_text: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_launch: true,
            auto_exit: true,
            log_poll_interval_ms: 500,
            presence_update_interval_s: 5,
        }
    }
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self {
            show_elapsed_timer: true,
            show_hero: true,
            details_with_hero: "Playing as {hero}".to_string(),
            details_no_hero: "{phase}".to_string(),
            status: StatusStrings::default(),
        }
    }
}

impl Default for StatusStrings {
    fn default() -> Self {
        Self {
            not_running: "Not Running".to_string(),
            main_menu: "Browsing the Main Menu".to_string(),
            in_hideout: "In the Hideout".to_string(),
            in_queue: "Searching for a Match...".to_string(),
            match_intro: "{mode} \u{2022} Loading into Match".to_string(),
            in_match: "{mode} \u{2022} Battling in {location}".to_string(),
            in_match_location: "the Cursed Apple".to_string(),
            post_match: "Reviewing Match Results".to_string(),
            spectating: "Watching a Match".to_string(),
        }
    }
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            default_large_image: "deadlock_logo".to_string(),
            default_large_text: "Deadlock".to_string(),
            small_image: "deadlock_logo".to_string(),
            small_text: "Deadlock".to_string(),
        }
    }
}

/// Replaces all `{key}` placeholders in `template` with their corresponding values.
/// Unrecognised placeholders are left as-is.
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

/// Load config from `config.toml` next to the executable.
///
/// - If the file does not exist: write a fully-documented default and return defaults.
/// - If the file is malformed: log a warning and return defaults without overwriting.
/// - If the file is a valid partial config: unset fields fall back to their defaults.
pub fn load() -> Config {
    let path = config_path();

    if !path.exists() {
        match std::fs::write(&path, DEFAULT_TOML) {
            Ok(_) => crate::log!("[config] Created default config.toml at {}", path.display()),
            Err(e) => crate::log!("[config] Could not write default config.toml: {e}"),
        }
        return Config::default();
    }

    match std::fs::read_to_string(&path) {
        Err(e) => {
            crate::log!("[config] Could not read config.toml: {e} — using defaults");
            Config::default()
        }
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                crate::log!("[config] config.toml parse error: {e} — using defaults");
                Config::default()
            }
        },
    }
}

const DEFAULT_TOML: &str = r#"# deadlock-rpc configuration
# All values shown are the defaults. Edit freely — unrecognised keys are ignored.
# Booleans: true / false     Strings: "value"     Numbers: 123

[general]
# Launch Deadlock automatically when deadlock-rpc starts.
# Set to false to behave like passing --no-launch every time.
auto_launch = true

# Exit deadlock-rpc automatically when the game closes.
# Set to false to keep it running (useful if you restart the game often).
auto_exit = true

# Milliseconds between game log polls. Lower = faster state detection, slightly more I/O.
# Minimum recommended: 100. Default: 500.
log_poll_interval_ms = 500

# Seconds between Discord presence refreshes.
presence_update_interval_s = 5


[presence]
# Show the elapsed time counter on your Discord profile.
show_elapsed_timer = true

# Show the current hero's image and name.
# Set to false to always display the Deadlock logo instead.
show_hero = true

# The top "details" line in the presence card when a hero is known.
# Available variables:
#   {hero}  — hero display name, e.g. "Vindicta"
details_with_hero = "Playing as {hero}"

# The top "details" line when no hero is known (menus, post-match, etc.).
# Available variables:
#   {phase} — current phase label, e.g. "Post Match"
details_no_hero = "{phase}"


[presence.status]
# The bottom "state" line in the presence card, one entry per game phase.

not_running = "Not Running"
main_menu   = "Browsing the Main Menu"

# Shown in the Hideout when the API has no hero-specific flavour text.
# Available variables:
#   {hero} — hero display name (empty string if no hero is selected yet)
in_hideout  = "In the Hideout"

in_queue    = "Searching for a Match..."

# Available variables:
#   {mode} — match mode, e.g. "Standard Match" / "Bot Match"
match_intro = "{mode} • Loading into Match"

# Available variables:
#   {mode}     — match mode
#   {location} — value of in_match_location below
in_match    = "In Match: {mode}"

# The name substituted for {location} in in_match.
in_match_location = "the Cursed Apple"

post_match  = "Reviewing Match Results"
spectating  = "Spectating a Match"


[images]
# Discord application asset key (or HTTPS URL) for the large image
# shown when no hero image is available.
default_large_image = "deadlock_logo"

# Tooltip text for the large image when no hero is shown.
default_large_text  = "Deadlock"

# Discord application asset key for the small corner overlay image.
small_image = "deadlock_logo"

# Tooltip text for the small corner image.
small_text  = "Deadlock"
"#;
