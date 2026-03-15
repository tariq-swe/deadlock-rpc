#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Ranked and HeroLabs reserved for future detection
pub enum MatchMode {
    Unknown,
    Standard,
    Ranked,
    StreetBrawl,
    BotMatch,
    TrainingRange,
    HeroLabs,
}

impl MatchMode {
    pub fn display(self) -> &'static str {
        match self {
            MatchMode::Unknown => "In Match",
            MatchMode::Standard => "Standard Match",
            MatchMode::Ranked => "Ranked Match",
            MatchMode::StreetBrawl => "Street Brawl",
            MatchMode::BotMatch => "Bot Match",
            MatchMode::TrainingRange => "Training Range",
            MatchMode::HeroLabs => "Hero Labs",
        }
    }

    pub fn show_map_location(self) -> bool {
        matches!(self, MatchMode::Standard | MatchMode::Ranked | MatchMode::StreetBrawl | MatchMode::Unknown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    NotRunning,
    MainMenu,
    Hideout,
    InQueue,
    MatchIntro,
    InMatch,
    PostMatch,
    Spectating,
}

impl GamePhase {
    pub fn description(self) -> &'static str {
        match self {
            GamePhase::NotRunning => "Not Running",
            GamePhase::MainMenu => "Main Menu",
            GamePhase::Hideout => "Hideout",
            GamePhase::InQueue => "Searching for Match",
            GamePhase::MatchIntro => "Match Starting",
            GamePhase::InMatch => "In Match",
            GamePhase::PostMatch => "Post Match",
            GamePhase::Spectating => "Spectating",
        }
    }

    /// Whether the hero image and "Playing as" label should be shown for this phase.
    pub fn shows_hero(self) -> bool {
        !matches!(
            self,
            GamePhase::NotRunning | GamePhase::MainMenu | GamePhase::PostMatch | GamePhase::Spectating
        )
    }
}

pub struct GameState {
    pub phase: GamePhase,
    pub match_mode: MatchMode,
    pub hero_key: Option<String>,
    pub map_name: Option<String>,
    pub game_start_time: Option<i64>,
    /// Counts log lines received after the initial resync.
    /// Zero means the log is stale from a prior session — game is not actually running.
    pub live_lines_seen: u64,
    // internal tracking
    pub(crate) hero_window_open: bool,
    pub(crate) hideout_loaded: bool,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            phase: GamePhase::NotRunning,
            match_mode: MatchMode::Unknown,
            hero_key: None,
            map_name: None,
            game_start_time: None,
            live_lines_seen: 0,
            hero_window_open: true,
            hideout_loaded: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn enter_hideout(&mut self) {
        self.phase = GamePhase::Hideout;
        self.hero_key = None;
        self.hero_window_open = true;
        self.hideout_loaded = false;
    }

    pub fn enter_main_menu(&mut self) {
        self.phase = GamePhase::MainMenu;
        if self.game_start_time.is_none() {
            use std::time::{SystemTime, UNIX_EPOCH};
            self.game_start_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs() as i64);
        }
    }

    pub fn enter_queue(&mut self) {
        self.phase = GamePhase::InQueue;
    }

    pub fn leave_queue(&mut self) {
        self.phase = GamePhase::Hideout;
    }

    pub fn enter_match_intro(&mut self) {
        self.phase = GamePhase::MatchIntro;
        self.match_mode = MatchMode::Unknown;
        self.hero_key = None;
        self.hero_window_open = true;
    }

    pub fn start_match(&mut self) {
        self.phase = GamePhase::InMatch;
    }

    pub fn end_match(&mut self) {
        self.phase = GamePhase::PostMatch;
    }

    pub fn enter_spectating(&mut self) {
        self.phase = GamePhase::Spectating;
    }

    pub fn prepare_match_hero_tracking(&mut self) {
        self.hero_key = None;
        self.hero_window_open = true;
    }

    /// Returns the Discord presence status line for the current phase.
    /// `hideout_text` is the hero-specific hideout message from the API (used in Hideout phase).
    pub fn presence_status(&self, hideout_text: Option<&str>) -> String {
        match self.phase {
            GamePhase::NotRunning => "Not Running".to_string(),
            GamePhase::MainMenu => "Browsing the Main Menu".to_string(),
            GamePhase::Hideout => hideout_text
                .filter(|t| !t.is_empty())
                .unwrap_or("In the Hideout")
                .to_string(),
            GamePhase::InQueue => "Searching for a Match...".to_string(),
            GamePhase::MatchIntro => format!("{} • Loading into Match", self.match_mode.display()),
            GamePhase::InMatch => {
                let mode = self.match_mode.display();
                if self.match_mode.show_map_location() {
                    format!("{mode} • Battling in the Cursed Apple")
                } else {
                    mode.to_string()
                }
            }
            GamePhase::PostMatch => "Reviewing Match Results".to_string(),
            GamePhase::Spectating => "Watching a Match".to_string(),
        }
    }

    pub fn apply_hero_signal(&mut self, hero_key: &str) {
        if self.phase == GamePhase::Spectating {
            return;
        }

        match self.phase {
            GamePhase::MatchIntro | GamePhase::InMatch => {
                if let Some(ref current) = self.hero_key {
                    if hero_key != current.as_str() {
                        return; // hero locked in, ignore a different hero
                    }
                } else if !self.hero_window_open {
                    return;
                }
                self.hero_key = Some(hero_key.to_string());
                self.hero_window_open = false;
            }
            GamePhase::Hideout => {
                self.hero_key = Some(hero_key.to_string());
            }
            GamePhase::MainMenu | GamePhase::PostMatch => {}
            _ => {
                self.hero_key = Some(hero_key.to_string());
            }
        }
    }
}

