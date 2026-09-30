use std::path::Path;

use serde::{Deserialize, Serialize};

use super::rules::{Action, Matcher, WindowRule};
use super::window::ChromeMode;
use super::zone::{NormalizedRect, Zone, ZoneReference};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Global kill switch for all automatic window management.
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub zones: Vec<Zone>,
    pub rules: Vec<WindowRule>,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

fn default_true() -> bool {
    true
}

fn zone_rule(id: &str, name: &str, processes: &[&str]) -> WindowRule {
    WindowRule {
        id: id.into(),
        name: name.into(),
        enabled: true,
        matcher: Matcher::Any(processes.iter().map(|n| Matcher::ProcessName((*n).into())).collect()),
        actions: vec![Action::FullscreenZone {
            zone_id: "zone-left-75".into(),
            chrome: ChromeMode::Keep,
        }],
    }
}

/// Common Windows video players. There is no OS category for "media player", so this is a
/// process list; add another name in the Rules panel when a player is missing.
const MEDIA_PLAYERS: &[&str] = &[
    "vlc.exe",
    "mpv.exe",
    "mpvnet.exe",
    "smplayer.exe",
    "mplayer.exe",
    "ffplay.exe",
    "mpc-hc.exe",
    "mpc-hc64.exe",
    "mpc-be.exe",
    "mpc-be64.exe",
    "mpc-qt.exe",
    "potplayer.exe",
    "potplayer64.exe",
    "potplayermini.exe",
    "potplayermini64.exe",
    "kmplayer.exe",
    "gom.exe",
    "kodi.exe",
    "wmplayer.exe",
    "jellyfinmediaplayer.exe",
    "plex.exe",
    "plex htpc.exe",
    "zplayer.exe",
];

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            enabled: true,
            zones: vec![Zone {
                id: "zone-left-75".into(),
                name: "75% Left".into(),
                rect: NormalizedRect { x: 0.0, y: 0.0, width: 0.75, height: 1.0 },
                monitor: None,
                reference: ZoneReference::Monitor,
            }],
            rules: vec![
                zone_rule("rule-browsers", "Browsers", &[
                    "chrome.exe",
                    "msedge.exe",
                    "brave.exe",
                    "firefox.exe",
                    "librewolf.exe",
                ]),
                zone_rule("rule-media-players", "Media players", MEDIA_PLAYERS),
            ],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("config is not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl AppConfig {
    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    /// Normalizes zone geometry and rejects structurally broken configs (duplicate ids,
    /// rules pointing at missing zones).
    pub fn validated(mut self) -> Result<Self, ConfigError> {
        let mut seen = std::collections::HashSet::new();
        for zone in &mut self.zones {
            if zone.id.trim().is_empty() {
                return Err(ConfigError::Invalid("zone with empty id".into()));
            }
            if !seen.insert(zone.id.clone()) {
                return Err(ConfigError::Invalid(format!("duplicate zone id '{}'", zone.id)));
            }
            zone.rect = zone.rect.sanitized();
        }
        let mut seen = std::collections::HashSet::new();
        for rule in &self.rules {
            if !seen.insert(rule.id.clone()) {
                return Err(ConfigError::Invalid(format!("duplicate rule id '{}'", rule.id)));
            }
            if let Some((zone_id, _)) = rule.fullscreen_zone() {
                if self.zone(zone_id).is_none() {
                    return Err(ConfigError::Invalid(format!(
                        "rule '{}' references unknown zone '{zone_id}'",
                        rule.name
                    )));
                }
            }
        }
        self.version = CONFIG_VERSION;
        Ok(self)
    }

    /// Loads config; a missing file yields defaults. A corrupt file is an error so the caller
    /// can back it up instead of silently overwriting the user's settings.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<AppConfig>(&text)?.validated(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        write_json_atomic(path, self)
    }
}

/// Writes via temp file + rename so a crash mid-write cannot leave a truncated file.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_and_roundtrips() {
        let cfg = AppConfig::default().validated().unwrap();
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<AppConfig>(&json).unwrap(), cfg);
    }

    #[test]
    fn default_media_players_match_common_video_exes() {
        use crate::core::rules::test_identity;
        let cfg = AppConfig::default();
        let media = cfg.rules.iter().find(|r| r.id == "rule-media-players").unwrap();
        assert!(media.matcher.matches(&test_identity("vlc.exe", "Qt6QWindowIcon", "VLC")));
        assert!(media.matcher.matches(&test_identity("PotPlayerMini64.EXE", "", "")));
        assert!(!media.matcher.matches(&test_identity("notepad.exe", "", "")));
        assert_eq!(media.fullscreen_zone().map(|(z, _)| z), Some("zone-left-75"));
    }

    #[test]
    fn rejects_dangling_zone_reference() {
        let mut cfg = AppConfig::default();
        cfg.zones.clear();
        assert!(matches!(cfg.validated(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn rejects_duplicate_zone_ids() {
        let mut cfg = AppConfig::default();
        cfg.zones.push(cfg.zones[0].clone());
        assert!(cfg.validated().is_err());
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("screenbound-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let cfg = AppConfig { enabled: false, ..Default::default() };
        cfg.save(&path).unwrap();
        assert_eq!(AppConfig::load(&path).unwrap(), cfg);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_file_yields_default() {
        let path = std::env::temp_dir().join("screenbound-does-not-exist").join("config.json");
        assert_eq!(AppConfig::load(&path).unwrap(), AppConfig::default());
    }
}
