//! Zone-fullscreen engine. Consumes coarse platform events, decides what they mean using the
//! core heuristics, and drives the platform to apply/restore native window state.
//! Runs on a single thread (see `handle.rs`), so it owns its state without locks.

pub mod activity;
mod handle;
pub mod journal;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::core::config::{AppConfig, ConfigError};
use crate::core::fullscreen::{
    classify_managed, compensate_invisible_frame, fullscreen_monitor, ManagedObservation, BOUNDS_TOLERANCE,
};
use crate::core::geometry::Rect;
use crate::core::monitor::{monitor_for_rect, MonitorId, MonitorInfo};
use crate::core::rules::first_match;
use crate::core::window::{ChromeMode, ShowState, WindowId, WindowIdentity, WindowInfo, WindowState};
use crate::platform::{PlatformError, PlatformEvent, PlatformWindowManager};
use activity::{ActivityEntry, ActivityLevel, ActivityLog};
use journal::JournalEntry;

pub use handle::{EngineHandle, EngineMessage};

/// Collapses bursts of native events (e.g. restore → restyle → resize during a fullscreen
/// transition) into one evaluation, measured from the first event so busy windows can't starve.
const EVENT_SETTLE: Duration = Duration::from_millis(60);
const DISPLAY_SETTLE: Duration = Duration::from_millis(500);
/// Loop guard: if an app re-asserts fullscreen this often, stop fighting it.
const REAPPLY_WINDOW: Duration = Duration::from_secs(3);
const MAX_REAPPLIES: usize = 4;

#[derive(Debug, Clone)]
pub enum Notification {
    Activity(ActivityEntry),
    ManagedChanged,
    MonitorsChanged(Vec<MonitorInfo>),
    ConfigChanged(AppConfig),
}

pub type Notifier = Arc<dyn Fn(Notification) + Send + Sync>;

pub struct EnginePaths {
    pub config: PathBuf,
    pub journal: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseReason {
    Requested,
    ConfigChanged,
    Shutdown,
}

struct ManagedWindow {
    id: WindowId,
    identity: WindowIdentity,
    rule_id: String,
    zone_id: String,
    chrome: ChromeMode,
    /// The app's own fullscreen state when we took over.
    detected: WindowState,
    /// Last normal (non-fullscreen) state seen before the transition, if we observed it.
    pre_fullscreen: Option<WindowState>,
    /// What the window looked like right after our last apply.
    applied: WindowState,
    target: Rect,
    monitor: Option<MonitorId>,
    reapplies: VecDeque<Instant>,
    gave_up: bool,
    since_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSummary {
    pub id: WindowId,
    pub id_hex: String,
    pub process_name: String,
    pub title: String,
    pub rule_id: String,
    pub zone_id: String,
    pub chrome: ChromeMode,
    pub monitor: Option<MonitorId>,
    pub target: Rect,
    pub detected_bounds: Rect,
    pub pre_fullscreen_bounds: Option<Rect>,
    pub gave_up: bool,
    pub since_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub enabled: bool,
    pub events_active: bool,
    pub managed: Vec<ManagedSummary>,
}

pub struct Engine {
    platform: Arc<dyn PlatformWindowManager>,
    paths: EnginePaths,
    notify: Notifier,
    config: AppConfig,
    monitors: Vec<MonitorInfo>,
    managed: BTreeMap<WindowId, ManagedWindow>,
    last_normal: HashMap<WindowId, WindowState>,
    /// Fullscreen windows we failed to manage (e.g. elevated); retried once they leave fullscreen.
    rejected: HashSet<WindowId>,
    pending: HashMap<WindowId, Instant>,
    display_pending: Option<Instant>,
    moving: HashSet<WindowId>,
    activity: ActivityLog,
    events_active: bool,
}

fn label(identity: &WindowIdentity, id: WindowId) -> String {
    let name = if identity.process_name.is_empty() { "<unknown>" } else { &identity.process_name };
    format!("{name} {id}")
}

impl Engine {
    pub fn new(platform: Arc<dyn PlatformWindowManager>, paths: EnginePaths, notify: Notifier) -> Self {
        let mut activity = ActivityLog::default();
        let config = match AppConfig::load(&paths.config) {
            Ok(c) => c,
            Err(e) => {
                let backup = paths.config.with_extension(format!("json.broken-{}", activity::now_ms()));
                let _ = std::fs::rename(&paths.config, &backup);
                activity.push(
                    ActivityLevel::Warn,
                    None,
                    format!("Config could not be loaded ({e}); moved to {} and using defaults", backup.display()),
                );
                AppConfig::default()
            }
        };
        let mut engine = Self {
            platform,
            paths,
            notify,
            config,
            monitors: Vec::new(),
            managed: BTreeMap::new(),
            last_normal: HashMap::new(),
            rejected: HashSet::new(),
            pending: HashMap::new(),
            display_pending: None,
            moving: HashSet::new(),
            activity,
            events_active: false,
        };
        engine.refresh_monitors();
        engine
    }

    // ---- lifecycle -------------------------------------------------------------------------

    pub fn startup(&mut self) {
        self.recover_journal();
        self.scan_all();
    }

    pub fn set_events_active(&mut self, active: bool) {
        self.events_active = active;
    }

    pub fn shutdown(&mut self) {
        self.release_all_with(ReleaseReason::Shutdown);
        journal::save(&self.paths.journal, &[]);
    }

    fn recover_journal(&mut self) {
        for entry in journal::load(&self.paths.journal) {
            let still_ours = self.platform.identity(entry.window).is_ok_and(|i| i.process == entry.process)
                && self
                    .platform
                    .state(entry.window)
                    .is_ok_and(|s| s.bounds.approx_eq(&entry.applied.bounds, BOUNDS_TOLERANCE));
            if !still_ours {
                continue;
            }
            let who = Some(format!("{} {}", entry.process_name, entry.window));
            match self.platform.restore_state(entry.window, &entry.detected) {
                Ok(()) => self.log(ActivityLevel::Info, who, "Restored window left constrained by a previous session".into()),
                Err(e) => self.log(ActivityLevel::Warn, who, format!("Could not restore window from previous session: {e}")),
            }
        }
        journal::save(&self.paths.journal, &[]);
    }

    fn scan_all(&mut self) {
        match self.platform.enumerate_windows() {
            Ok(ids) => ids.into_iter().for_each(|id| self.evaluate(id)),
            Err(e) => self.log(ActivityLevel::Error, None, format!("Window enumeration failed: {e}")),
        }
    }

    // ---- event intake ----------------------------------------------------------------------

    pub fn on_platform_event(&mut self, event: PlatformEvent) {
        let now = Instant::now();
        match event {
            PlatformEvent::WindowChanged(id) => {
                self.pending.entry(id).or_insert(now + EVENT_SETTLE);
            }
            PlatformEvent::WindowDestroyed(id) => self.forget(id),
            PlatformEvent::MoveSizeStart(id) => {
                self.moving.insert(id);
            }
            PlatformEvent::MoveSizeEnd(id) => {
                self.moving.remove(&id);
                self.on_user_moved(id);
            }
            PlatformEvent::DisplayChanged => {
                self.display_pending.get_or_insert(now + DISPLAY_SETTLE);
            }
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().copied().chain(self.display_pending).min()
    }

    pub fn process_due(&mut self, now: Instant) {
        if self.display_pending.is_some_and(|t| t <= now) {
            self.display_pending = None;
            self.on_display_changed();
        }
        let due: Vec<WindowId> = self.pending.iter().filter(|(_, t)| **t <= now).map(|(id, _)| *id).collect();
        for id in due {
            self.pending.remove(&id);
            self.evaluate(id);
        }
    }

    // ---- core state machine ----------------------------------------------------------------

    fn evaluate(&mut self, id: WindowId) {
        if !self.platform.exists(id) {
            self.forget(id);
            return;
        }
        if self.moving.contains(&id) {
            return;
        }
        if self.managed.contains_key(&id) {
            self.evaluate_managed(id);
            return;
        }
        if !self.config.enabled {
            return;
        }
        let Ok(identity) = self.platform.identity(id) else { return };
        let Some(rule) = first_match(&self.config.rules, &identity) else { return };
        let Some((zone_id, chrome)) = rule.fullscreen_zone() else { return };
        let (rule_id, zone_id) = (rule.id.clone(), zone_id.to_string());

        let state = match self.platform.state(id) {
            Ok(s) => s,
            Err(PlatformError::WindowGone(_)) => return self.forget(id),
            Err(_) => return,
        };
        match fullscreen_monitor(&state, &self.monitors).map(|m| m.id.clone()) {
            Some(monitor) if !self.rejected.contains(&id) => {
                self.manage(id, identity, rule_id, zone_id, chrome, state, monitor);
            }
            Some(_) => {}
            None => {
                self.rejected.remove(&id);
                if state.visible && !state.cloaked && state.show_state != ShowState::Minimized {
                    self.last_normal.insert(id, state);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn manage(
        &mut self,
        id: WindowId,
        identity: WindowIdentity,
        rule_id: String,
        zone_id: String,
        chrome: ChromeMode,
        state: WindowState,
        monitor: MonitorId,
    ) {
        let who = label(&identity, id);
        let mut m = ManagedWindow {
            id,
            identity,
            rule_id,
            zone_id,
            chrome,
            detected: state.clone(),
            pre_fullscreen: self.last_normal.remove(&id),
            applied: state.clone(),
            target: state.bounds,
            monitor: Some(monitor.clone()),
            reapplies: VecDeque::new(),
            gave_up: false,
            since_ms: activity::now_ms(),
        };
        tracing::info!(
            window = %who,
            class = %m.identity.class_name,
            fullscreen_bounds = %state.bounds,
            pre_fullscreen = ?m.pre_fullscreen.as_ref().map(|s| s.bounds.to_string()),
            style = format_args!("{:#010x}/{:#010x}", state.style.primary, state.style.extended),
            "detected fullscreen transition"
        );
        match self.apply_zone(&mut m, Some(&monitor)) {
            Ok(msg) => {
                self.managed.insert(id, m);
                self.save_journal();
                self.log(ActivityLevel::Info, Some(who), msg);
                (self.notify)(Notification::ManagedChanged);
            }
            Err(e) => {
                self.rejected.insert(id);
                // Undo any partial change (e.g. styles stripped but move refused).
                let _ = self.platform.restore_state(id, &state);
                self.log(ActivityLevel::Warn, Some(who), format!("Fullscreen detected but could not constrain: {e}"));
            }
        }
    }

    /// Positions the window in its zone. Returns a human-readable summary.
    fn apply_zone(&self, m: &mut ManagedWindow, window_monitor: Option<&MonitorId>) -> Result<String, String> {
        let zone = self.config.zone(&m.zone_id).ok_or_else(|| format!("zone '{}' no longer exists", m.zone_id))?;
        let window_monitor = window_monitor.and_then(|id| self.monitors.iter().find(|x| &x.id == id));
        let monitor = zone
            .target_monitor(&self.monitors, window_monitor)
            .ok_or("no monitor available for zone")?;
        let target = zone.resolve_on(monitor);

        if m.chrome == ChromeMode::Hide {
            self.platform.set_borderless(m.id).map_err(|e| e.to_string())?;
        }
        let before = self.platform.state(m.id).map_err(|e| e.to_string())?;
        self.platform
            .set_bounds(m.id, compensate_invisible_frame(target, &before))
            .map_err(|e| e.to_string())?;
        let after = self.platform.state(m.id).map_err(|e| e.to_string())?;

        if !after.visible_bounds.approx_eq(&target, BOUNDS_TOLERANCE) {
            return Err(format!(
                "application rejected the zone bounds (wanted {target}, got {})",
                after.visible_bounds
            ));
        }
        let summary = format!(
            "Constrained to zone '{}' on {}: {} → {}",
            zone.name, monitor.friendly_name, before.bounds, after.bounds
        );
        m.applied = after;
        m.target = target;
        m.monitor = Some(monitor.id.clone());
        Ok(summary)
    }

    fn evaluate_managed(&mut self, id: WindowId) {
        let state = match self.platform.state(id) {
            Ok(s) => s,
            Err(PlatformError::WindowGone(_)) => return self.forget(id),
            Err(e) => return tracing::debug!(window = %id, "state read failed: {e}"),
        };
        let Some(m) = self.managed.get(&id) else { return };
        match classify_managed(&state, &m.applied, &self.monitors) {
            ManagedObservation::Unchanged | ManagedObservation::Dormant => {}
            ManagedObservation::Refullscreened => self.reapply(id, state),
            ManagedObservation::Exited => self.release_after_exit(id, state),
        }
    }

    /// The app re-asserted fullscreen (monitor move, display change, or fighting us).
    fn reapply(&mut self, id: WindowId, state: WindowState) {
        let Some(mut m) = self.managed.remove(&id) else { return };
        if m.gave_up {
            self.managed.insert(id, m);
            return;
        }
        let now = Instant::now();
        m.reapplies.retain(|t| now.duration_since(*t) < REAPPLY_WINDOW);
        m.reapplies.push_back(now);
        let who = label(&m.identity, id);
        if m.reapplies.len() > MAX_REAPPLIES {
            m.gave_up = true;
            self.managed.insert(id, m);
            self.log(
                ActivityLevel::Warn,
                Some(who),
                "Application keeps re-asserting fullscreen; leaving it at its own size until it exits fullscreen".into(),
            );
            (self.notify)(Notification::ManagedChanged);
            return;
        }
        let monitor = monitor_for_rect(&self.monitors, &state.bounds).map(|x| x.id.clone());
        // Keep the app's true fullscreen state for restoration, not transient near-fullscreen shrinks.
        if fullscreen_monitor(&state, &self.monitors).is_some() {
            m.detected = state;
        }
        match self.apply_zone(&mut m, monitor.as_ref()) {
            Ok(msg) => {
                tracing::info!(window = %who, "re-applied zone: {msg}");
                self.managed.insert(id, m);
                self.save_journal();
            }
            Err(e) => {
                self.rejected.insert(id);
                self.log(ActivityLevel::Warn, Some(who), format!("Could not re-apply zone: {e}"));
                self.save_journal();
            }
        }
        (self.notify)(Notification::ManagedChanged);
    }

    /// The app left fullscreen on its own and restored its own bounds. Only undo style changes
    /// that are still exactly as we left them (i.e. the app did not restore styles itself).
    fn release_after_exit(&mut self, id: WindowId, current: WindowState) {
        let Some(m) = self.managed.remove(&id) else { return };
        let who = label(&m.identity, id);
        if current.style == m.applied.style && m.applied.style != m.detected.style {
            let style = m.pre_fullscreen.as_ref().map_or(m.detected.style, |p| p.style);
            if let Err(e) = self.platform.set_style(id, style) {
                self.log(ActivityLevel::Warn, Some(who.clone()), format!("Could not restore window styles: {e}"));
            }
        }
        tracing::info!(
            window = %who,
            bounds = %current.bounds,
            pre_fullscreen = ?m.pre_fullscreen.as_ref().map(|s| s.bounds.to_string()),
            "fullscreen exited"
        );
        self.log(ActivityLevel::Info, Some(who), format!("Fullscreen exited; released at {}", current.bounds));
        self.last_normal.insert(id, current);
        self.save_journal();
        (self.notify)(Notification::ManagedChanged);
    }

    /// Hands a still-fullscreen window back to the app's own fullscreen state.
    fn release(&mut self, id: WindowId, reason: ReleaseReason) {
        let Some(m) = self.managed.remove(&id) else { return };
        let who = label(&m.identity, id);
        let in_zone = self
            .platform
            .state(id)
            .is_ok_and(|s| s.bounds.approx_eq(&m.applied.bounds, BOUNDS_TOLERANCE));
        if in_zone && !m.gave_up {
            tracing::info!(
                window = %who,
                original_bounds = %m.detected.bounds,
                original_style = format_args!("{:#010x}/{:#010x}", m.detected.style.primary, m.detected.style.extended),
                ?reason,
                "restoring window"
            );
            match self.platform.restore_state(id, &m.detected) {
                Ok(()) => self.log(ActivityLevel::Info, Some(who), format!("Released ({reason:?}); restored {}", m.detected.bounds)),
                Err(e) => self.log(ActivityLevel::Warn, Some(who), format!("Release ({reason:?}) could not restore: {e}")),
            }
        }
        // Suppress immediate re-capture of the (still fullscreen) window.
        self.rejected.insert(id);
        self.save_journal();
    }

    fn release_all_with(&mut self, reason: ReleaseReason) {
        let ids: Vec<WindowId> = self.managed.keys().copied().collect();
        for id in ids {
            self.release(id, reason);
        }
        (self.notify)(Notification::ManagedChanged);
    }

    fn on_user_moved(&mut self, id: WindowId) {
        let Some(m) = self.managed.get_mut(&id) else { return };
        if let Ok(state) = self.platform.state(id) {
            // Respect manual placement while still considering the window managed.
            tracing::info!(window = %id, bounds = %state.bounds, "user moved managed window");
            m.applied = state;
            self.save_journal();
        }
    }

    fn forget(&mut self, id: WindowId) {
        self.pending.remove(&id);
        self.moving.remove(&id);
        self.last_normal.remove(&id);
        self.rejected.remove(&id);
        if let Some(m) = self.managed.remove(&id) {
            self.log(ActivityLevel::Info, Some(label(&m.identity, id)), "Window closed while managed".into());
            self.save_journal();
            (self.notify)(Notification::ManagedChanged);
        }
    }

    fn on_display_changed(&mut self) {
        self.refresh_monitors();
        (self.notify)(Notification::MonitorsChanged(self.monitors.clone()));
        self.log(ActivityLevel::Info, None, format!("Display configuration changed ({} monitors)", self.monitors.len()));
        self.reapply_all();
    }

    fn reapply_all(&mut self) {
        let ids: Vec<WindowId> = self.managed.keys().copied().collect();
        for id in ids {
            let Some(mut m) = self.managed.remove(&id) else { continue };
            let Ok(state) = self.platform.state(id) else {
                self.forget(id);
                continue;
            };
            // If the monitor the app went fullscreen on is gone, hand back fullscreen on its current one.
            let detected_monitor_alive =
                m.detected.monitor.as_ref().is_some_and(|mid| self.monitors.iter().any(|x| &x.id == mid));
            if !detected_monitor_alive {
                if let Some(cur) = state.monitor.as_ref().and_then(|mid| self.monitors.iter().find(|x| &x.id == mid)) {
                    m.detected.bounds = cur.bounds;
                    m.detected.monitor = Some(cur.id.clone());
                }
            }
            m.gave_up = false;
            m.reapplies.clear();
            if let Err(e) = self.apply_zone(&mut m, state.monitor.as_ref()) {
                self.log(ActivityLevel::Warn, Some(label(&m.identity, id)), format!("Could not re-apply zone: {e}"));
            }
            self.managed.insert(id, m);
        }
        self.save_journal();
        (self.notify)(Notification::ManagedChanged);
    }

    fn refresh_monitors(&mut self) {
        match self.platform.monitors() {
            Ok(m) => self.monitors = m,
            Err(e) => {
                self.activity.push(ActivityLevel::Error, None, format!("Monitor discovery failed: {e}"));
            }
        }
    }

    fn save_journal(&self) {
        let entries: Vec<JournalEntry> = self
            .managed
            .values()
            .map(|m| JournalEntry {
                window: m.id,
                process: m.identity.process.clone(),
                process_name: m.identity.process_name.clone(),
                detected: m.detected.clone(),
                applied: m.applied.clone(),
            })
            .collect();
        journal::save(&self.paths.journal, &entries);
    }

    fn log(&mut self, level: ActivityLevel, window: Option<String>, message: String) {
        let entry = self.activity.push(level, window, message);
        (self.notify)(Notification::Activity(entry));
    }

    // ---- queries & commands (called from the UI via EngineHandle) --------------------------

    pub fn config(&self) -> AppConfig {
        self.config.clone()
    }

    pub fn monitors(&mut self) -> Vec<MonitorInfo> {
        self.refresh_monitors();
        self.monitors.clone()
    }

    pub fn activity(&self) -> Vec<ActivityEntry> {
        self.activity.entries()
    }

    pub fn status(&self) -> EngineStatus {
        EngineStatus {
            enabled: self.config.enabled,
            events_active: self.events_active,
            managed: self
                .managed
                .values()
                .map(|m| ManagedSummary {
                    id: m.id,
                    id_hex: m.id.to_string(),
                    process_name: m.identity.process_name.clone(),
                    title: m.identity.title.clone(),
                    rule_id: m.rule_id.clone(),
                    zone_id: m.zone_id.clone(),
                    chrome: m.chrome,
                    monitor: m.monitor.clone(),
                    target: m.target,
                    detected_bounds: m.detected.bounds,
                    pre_fullscreen_bounds: m.pre_fullscreen.as_ref().map(|s| s.bounds),
                    gave_up: m.gave_up,
                    since_ms: m.since_ms,
                })
                .collect(),
        }
    }

    pub fn list_windows(&mut self) -> Result<Vec<WindowInfo>, String> {
        self.refresh_monitors();
        let ids = self.platform.enumerate_windows().map_err(|e| e.to_string())?;
        Ok(ids
            .into_iter()
            .filter_map(|id| {
                let identity = self.platform.identity(id).ok()?;
                let state = self.platform.state(id).ok()?;
                let monitor_name = state
                    .monitor
                    .as_ref()
                    .and_then(|mid| self.monitors.iter().find(|m| &m.id == mid))
                    .map(|m| m.friendly_name.clone());
                Some(WindowInfo {
                    id,
                    id_hex: id.to_string(),
                    fullscreen_like: fullscreen_monitor(&state, &self.monitors).is_some(),
                    hung: self.platform.is_hung(id),
                    managed: self.managed.contains_key(&id),
                    matched_rule: first_match(&self.config.rules, &identity).map(|r| r.name.clone()),
                    identity,
                    state,
                    monitor_name,
                })
            })
            .collect())
    }

    pub fn set_config(&mut self, config: AppConfig) -> Result<AppConfig, String> {
        let config = config.validated().map_err(|e: ConfigError| e.to_string())?;
        config.save(&self.paths.config).map_err(|e| e.to_string())?;
        self.config = config;
        self.reconcile_managed();
        self.rejected.clear();
        (self.notify)(Notification::ConfigChanged(self.config.clone()));
        if self.config.enabled {
            self.scan_all();
        }
        Ok(self.config.clone())
    }

    /// Re-targets managed windows after a config edit (zone resized, rule changed/removed).
    fn reconcile_managed(&mut self) {
        let ids: Vec<WindowId> = self.managed.keys().copied().collect();
        for id in ids {
            let action = self
                .config
                .enabled
                .then(|| first_match(&self.config.rules, &self.managed[&id].identity))
                .flatten()
                .and_then(|r| r.fullscreen_zone().map(|(z, c)| (r.id.clone(), z.to_string(), c)));
            let Some((rule_id, zone_id, chrome)) = action else {
                self.release(id, ReleaseReason::ConfigChanged);
                continue;
            };
            let Some(mut m) = self.managed.remove(&id) else { continue };
            if m.chrome == ChromeMode::Hide && chrome == ChromeMode::Keep {
                let _ = self.platform.set_style(id, m.detected.style);
            }
            (m.rule_id, m.zone_id, m.chrome) = (rule_id, zone_id, chrome);
            m.gave_up = false;
            m.reapplies.clear();
            let monitor = m.monitor.clone();
            if let Err(e) = self.apply_zone(&mut m, monitor.as_ref()) {
                self.log(ActivityLevel::Warn, Some(label(&m.identity, id)), format!("Could not apply updated zone: {e}"));
            }
            self.managed.insert(id, m);
        }
        self.save_journal();
        (self.notify)(Notification::ManagedChanged);
    }

    pub fn release_window(&mut self, id: WindowId) {
        self.release(id, ReleaseReason::Requested);
        (self.notify)(Notification::ManagedChanged);
    }

    pub fn release_all(&mut self) {
        self.release_all_with(ReleaseReason::Requested);
    }
}
