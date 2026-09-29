use serde::{Deserialize, Serialize};

use super::window::{ChromeMode, WindowIdentity};

/// Predicate over a window's identity. Composable so rules can express
/// "chrome.exe OR msedge.exe" or "process X AND class Y" without app-specific code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Matcher {
    /// Executable file name, case-insensitive (`chrome.exe`).
    ProcessName(String),
    /// Full executable path, case-insensitive.
    ExecutablePath(String),
    /// Exact native window class (`Chrome_WidgetWin_1`), case-sensitive.
    WindowClass(String),
    /// Case-insensitive substring of the window title.
    TitleContains(String),
    Any(Vec<Matcher>),
    All(Vec<Matcher>),
}

impl Matcher {
    pub fn matches(&self, id: &WindowIdentity) -> bool {
        match self {
            Matcher::ProcessName(name) => !name.is_empty() && id.process_name.eq_ignore_ascii_case(name.trim()),
            Matcher::ExecutablePath(path) => {
                !path.is_empty() && id.executable_path.eq_ignore_ascii_case(path.trim())
            }
            Matcher::WindowClass(class) => id.class_name == *class,
            Matcher::TitleContains(needle) => {
                !needle.is_empty() && id.title.to_lowercase().contains(&needle.to_lowercase())
            }
            Matcher::Any(ms) => ms.iter().any(|m| m.matches(id)),
            Matcher::All(ms) => !ms.is_empty() && ms.iter().all(|m| m.matches(id)),
        }
    }
}

/// Behavior applied to matching windows. Only `FullscreenZone` is implemented in the MVP;
/// new variants (fixed size, aspect ratio, always-on-top, …) slot in here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// When the window becomes fullscreen, constrain it to `zone_id`.
    #[serde(rename_all = "camelCase")]
    FullscreenZone {
        zone_id: String,
        #[serde(default)]
        chrome: ChromeMode,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub matcher: Matcher,
    pub actions: Vec<Action>,
}

fn default_true() -> bool {
    true
}

impl WindowRule {
    // find_map stays correct once more Action variants exist.
    #[allow(clippy::unnecessary_find_map)]
    pub fn fullscreen_zone(&self) -> Option<(&str, ChromeMode)> {
        self.actions.iter().find_map(|a| match a {
            Action::FullscreenZone { zone_id, chrome } => Some((zone_id.as_str(), *chrome)),
        })
    }
}

/// First enabled rule wins, so users control priority by ordering.
pub fn first_match<'a>(rules: &'a [WindowRule], id: &WindowIdentity) -> Option<&'a WindowRule> {
    rules.iter().find(|r| r.enabled && r.matcher.matches(id))
}

#[cfg(test)]
pub(crate) fn test_identity(process_name: &str, class_name: &str, title: &str) -> WindowIdentity {
    use super::window::ProcessRef;
    WindowIdentity {
        process: ProcessRef { pid: 1, start_time: 1 },
        process_name: process_name.into(),
        executable_path: format!(r"C:\Apps\{process_name}"),
        class_name: class_name.into(),
        title: title.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browsers() -> Matcher {
        Matcher::Any(vec![
            Matcher::ProcessName("chrome.exe".into()),
            Matcher::ProcessName("msedge.exe".into()),
            Matcher::ProcessName("firefox.exe".into()),
            Matcher::ProcessName("librewolf.exe".into()),
        ])
    }

    #[test]
    fn process_name_is_case_insensitive() {
        let id = test_identity("Chrome.EXE", "Chrome_WidgetWin_1", "YouTube");
        assert!(browsers().matches(&id));
        assert!(!browsers().matches(&test_identity("vlc.exe", "Qt5QWindowIcon", "")));
    }

    #[test]
    fn composite_matchers() {
        let id = test_identity("firefox.exe", "MozillaWindowClass", "Big Buck Bunny — Mozilla Firefox");
        let all = Matcher::All(vec![
            Matcher::ProcessName("firefox.exe".into()),
            Matcher::WindowClass("MozillaWindowClass".into()),
            Matcher::TitleContains("bunny".into()),
        ]);
        assert!(all.matches(&id));
        assert!(!Matcher::All(vec![]).matches(&id));
        assert!(!Matcher::Any(vec![]).matches(&id));
        assert!(!Matcher::WindowClass("mozillawindowclass".into()).matches(&id));
        assert!(!Matcher::ProcessName(String::new()).matches(&test_identity("", "", "")));
    }

    #[test]
    fn first_enabled_rule_wins() {
        let rule = |id: &str, enabled: bool| WindowRule {
            id: id.into(),
            name: id.into(),
            enabled,
            matcher: browsers(),
            actions: vec![Action::FullscreenZone { zone_id: id.into(), chrome: ChromeMode::Keep }],
        };
        let rules = vec![rule("disabled", false), rule("a", true), rule("b", true)];
        let id = test_identity("msedge.exe", "Chrome_WidgetWin_1", "");
        assert_eq!(first_match(&rules, &id).unwrap().id, "a");
        assert_eq!(first_match(&rules, &id).unwrap().fullscreen_zone(), Some(("a", ChromeMode::Keep)));
    }

    #[test]
    fn rule_json_shape() {
        let json = r#"{
            "id": "r", "name": "Browsers",
            "matcher": {"type": "any", "value": [{"type": "processName", "value": "chrome.exe"}]},
            "actions": [{"type": "fullscreenZone", "zoneId": "left-75", "chrome": "hide"}]
        }"#;
        let rule: WindowRule = serde_json::from_str(json).unwrap();
        assert!(rule.enabled);
        assert_eq!(rule.fullscreen_zone(), Some(("left-75", ChromeMode::Hide)));
        let back: WindowRule = serde_json::from_str(&serde_json::to_string(&rule).unwrap()).unwrap();
        assert_eq!(back, rule);
    }
}
