use log::{debug, info, warn};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HeroData {
    pub name: String,
    pub hideout_text: String,
    pub icon_url: String,
}

#[derive(Deserialize)]
struct ApiHero {
    name: Option<String>,
    hideout_rich_presence: Option<String>,
    images: Option<ApiImages>,
}

#[derive(Deserialize)]
struct ApiImages {
    icon_hero_card: Option<String>,
}

pub struct HeroCache {
    map: HashMap<String, HeroData>,
    client: reqwest::blocking::Client,
}

impl HeroCache {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build HTTP client");
        Self { map: HashMap::new(), client }
    }

    /// Returns cached data if available, otherwise fetches from the API using the hero class_name.
    pub fn get_or_fetch(&mut self, hero_key: &str) -> Option<&HeroData> {
        use std::collections::hash_map::Entry;
        match self.map.entry(hero_key.to_owned()) {
            Entry::Occupied(e) => Some(e.into_mut()),
            Entry::Vacant(e) => match fetch(&self.client, hero_key) {
                Ok(data) => {
                    info!("[api] Cached: {} → \"{}\"", hero_key, data.name);
                    Some(e.insert(data))
                }
                Err(err) => {
                    warn!("[api] Failed to fetch {hero_key}: {err}");
                    None
                }
            },
        }
    }
}

fn fetch(client: &reqwest::blocking::Client, hero_key: &str) -> Result<HeroData, Box<dyn std::error::Error>> {
    debug!("[api] Fetching: {hero_key}");

    if let Ok(data) = fetch_by_name(client, hero_key) {
        debug!("[api] Resolved via full key: {hero_key}");
        return Ok(data);
    }

    let stripped = hero_key.trim_start_matches("hero_");
    if let Ok(data) = fetch_by_name(client, stripped) {
        debug!("[api] Resolved via stripped key: {stripped}");
        return Ok(data);
    }

    if let Some(display_name) = dict_lookup(hero_key) {
        debug!("[api] Dict fallback: {hero_key} → \"{display_name}\"");
        if let Ok(data) = fetch_by_name(client, display_name) {
            debug!("[api] Resolved via dict: {display_name}");
            return Ok(data);
        }
    }

    Err(format!("unknown hero: {hero_key}").into())
}

/// Maps asset_key → display name to query the API with (e.g. "hero_geist" → "Lady Geist").
fn dict_lookup(asset_key: &str) -> Option<&'static str> {
    match asset_key {
        "hero_inferno"  => Some("Infernus"),
        "hero_gigawatt_prisoner" => Some("Seven"),
        "hero_hornet"   => Some("Vindicta"),
        "hero_geist"    => Some("Lady Geist"),
        "hero_atlas"    => Some("Abrams"),
        "hero_wraith"   => Some("Wraith"),
        "hero_forge"    => Some("McGinnis"),
        "hero_dynamo"   => Some("Dynamo"),
        "hero_haze"     => Some("Haze"),
        "hero_kelvin"   => Some("Kelvin"),
        "hero_lash"     => Some("Lash"),
        "hero_bebop"    => Some("Bebop"),
        "hero_shiv"     => Some("Shiv"),
        "hero_viscous"  => Some("Viscous"),
        "hero_warden"   => Some("Warden"),
        "hero_yamato"   => Some("Yamato"),
        "hero_archer"    => Some("Grey Talon"),
        "hero_digger"    => Some("Mo & Krill"),
        "hero_synth"    => Some("Pocket"),
        "hero_chrono"   => Some("Paradox"),
        "hero_astro"    => Some("Holliday"),
        "hero_cadence"  => Some("Calico"),
        "hero_werewolf" => Some("Silver"),
        "hero_magician" => Some("Sinclair"),
        "hero_tengu"    => Some("Ivy"),
        _ => None,
    }
}

fn fetch_by_name(client: &reqwest::blocking::Client, name: &str) -> Result<HeroData, Box<dyn std::error::Error>> {
    let url = format!("https://assets.deadlock-api.com/v2/heroes/by-name/{name}");
    debug!("[api] GET {url}");
    let hero: ApiHero = client.get(&url).send()?.json()?;
    let images = hero.images.ok_or("hero not found")?;
    Ok(HeroData {
        name: hero.name.unwrap_or_else(|| name.trim_start_matches("hero_").to_string()),
        hideout_text: hero.hideout_rich_presence.unwrap_or_default(),
        icon_url: images.icon_hero_card.unwrap_or_default(),
    })
}
