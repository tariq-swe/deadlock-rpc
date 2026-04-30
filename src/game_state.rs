use std::collections::HashSet;

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

    // Whether the hero image and "Playing as" label should be shown for this phase.
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
    pub party_size: u8,
    
    // internal tracking
    pub(crate) hero_window_open: bool,
    pub(crate) hideout_loaded: bool,
    pub(crate) local_account_id: Option<u64>,
    pub(crate) party_id: Option<u64>,
    pub(crate) party_members: HashSet<u64>,
    pub(crate) pending_player_count: u32,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            phase: GamePhase::NotRunning,
            match_mode: MatchMode::Unknown,
            hero_key: None,
            map_name: None,
            party_size: 1,
            hero_window_open: true,
            hideout_loaded: false,
            local_account_id: None,
            party_id: None,
            party_members: HashSet::new(),
            pending_player_count: 0,
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

    pub(crate) fn clear_party(&mut self) {
        self.party_id = None;
        self.party_members.clear();
        self.party_size = 1;
    }

    pub(crate) fn apply_party_event(&mut self, party_id: u64, event_name: &str, account_id: u64) {
        let ev = event_name.to_lowercase();
        if ev.contains("joinedparty") {
            let local = self.local_account_id.unwrap_or(u64::MAX);
            if account_id == local {
                self.party_id = Some(party_id);
                self.party_members = std::iter::once(account_id).collect();
            } else if self.party_id != Some(party_id) {
                self.party_id = Some(party_id);
                self.party_members.clear();
            }
            self.party_members.insert(account_id);
            self.party_size = (self.party_members.len() as u8).max(2);
        } else if ev.contains("leftparty")
            || ev.contains("removedfromparty")
            || ev.contains("kickedfromparty")
        {
            if account_id == self.local_account_id.unwrap_or(u64::MAX) {
                self.clear_party();
            } else {
                self.party_members.remove(&account_id);
                self.party_size = (self.party_members.len() as u8).max(1);
            }
        } else if ev.contains("disband") {
            self.clear_party();
        }
    }

    pub fn enter_main_menu(&mut self) {
        self.phase = GamePhase::MainMenu;
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
        self.pending_player_count = 0;
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

    // Returns the Discord presence status line for the current phase.
    //
    // - `hideout_text`: hero-specific text from the API (takes priority in Hideout phase).
    // - `hero_name`: display name of the current hero (used as `{hero}` variable).
    // - `cfg`: per-phase string templates from the loaded config.
    pub fn presence_status(
        &self,
        hideout_text: Option<&str>,
        hero_name: Option<&str>,
        cfg: &crate::config::StatusStrings,
    ) -> String {
        use crate::config::apply_vars;
        match self.phase {
            GamePhase::NotRunning => cfg.not_running.clone(),
            GamePhase::MainMenu => cfg.main_menu.clone(),
            GamePhase::Hideout => {
                if let Some(text) = hideout_text.filter(|t| !t.is_empty()) {
                    text.to_string()
                } else {
                    apply_vars(&cfg.in_hideout, &[("hero", hero_name.unwrap_or(""))])
                }
            }
            GamePhase::InQueue => cfg.in_queue.clone(),
            GamePhase::MatchIntro => {
                apply_vars(&cfg.match_intro, &[("mode", self.match_mode.display())])
            }
            GamePhase::InMatch => {
                let mode = self.match_mode.display();
                if self.match_mode.show_map_location() {
                    apply_vars(
                        &cfg.in_match,
                        &[("mode", mode), ("location", &cfg.in_match_location)],
                    )
                } else {
                    mode.to_string()
                }
            }
            GamePhase::PostMatch => cfg.post_match.clone(),
            GamePhase::Spectating => cfg.spectating.clone(),
        }
    }

    pub fn apply_hero_signal(&mut self, hero_key: &str) {
        if self.phase == GamePhase::Spectating {
            return;
        }

        match self.phase {
            GamePhase::MatchIntro | GamePhase::InMatch => {
                let free_swap = matches!(self.match_mode, MatchMode::TrainingRange | MatchMode::HeroLabs);
                if !free_swap {
                    if let Some(ref current) = self.hero_key {
                        if hero_key != current.as_str() {
                            return; // hero locked in, ignore a different hero
                        }
                    } else if !self.hero_window_open {
                        return;
                    }
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

