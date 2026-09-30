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
    classify_managed, compensate_invisible_frame, fullscreen_monitor, is_resting_layout,
    map_companion, snapped_zone_target, ManagedObservation, BOUNDS_TOLERANCE,
};
use crate::core::geometry::Rect;
use crate::core::monitor::{monitor_for_rect, MonitorId, MonitorInfo};
use crate::core::transition::{frame_at, START_DELAY};
use crate::core::window::{
    ChromeMode, ShowState, WindowId, WindowIdentity, WindowInfo, WindowState,
};
use crate::platform::{PlatformError, PlatformEvent, PlatformWindowManager};
use activity::{ActivityEntry, ActivityLevel, ActivityLog};
use journal::JournalEntry;

pub use handle::{EngineHandle, EngineMessage};

/// Collapses bursts of native events (e.g. restore → restyle → resize during a fullscreen
/// transition) into one evaluation, measured from the first event so busy windows can't starve.
const EVENT_SETTLE: Duration = Duration::from_millis(60);
/// YouTube's fullscreen button (Firefox DOM fullscreen) resizes the window onto the monitor
/// several times. Applying on the first of those loses: a later resize puts the monitor rect
/// back, and after a few of those we used to give up. Wait until the rect stops changing.
const FULLSCREEN_SETTLE: Duration = Duration::from_millis(300);
/// Don't wait forever if the app keeps nudging the rect.
const FULLSCREEN_SETTLE_CAP: Duration = Duration::from_millis(1000);
const DISPLAY_SETTLE: Duration = Duration::from_millis(500);
/// How far up an owner chain to look when deciding whether a window belongs to a managed one.
const MAX_OWNER_DEPTH: usize = 8;
/// Loop guard: if an app re-asserts fullscreen this often, stop fighting it.
const REAPPLY_WINDOW: Duration = Duration::from_secs(3);
const MAX_REAPPLIES: usize = 4;
/// Animation step interval (~60 fps). Steps are time-based, so a slow app gets fewer, larger steps.
const ANIMATION_FRAME: Duration = Duration::from_millis(16);
/// Slack when checking that the window is still where the last animation step put it.
const ANIMATION_TOLERANCE: i32 = 4;
/// How often an app may push itself back to the monitor rect mid-animation before we just snap.
const MAX_ANIMATION_RESTARTS: u8 = 3;
const MIN_RESTART_DURATION: Duration = Duration::from_millis(250);

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
    /// Fill the FancyZones zone the window was snapped to instead of `zone_id`.
    use_snapped_zone: bool,
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

/// Where a zone apply ends up, resolved before the window is moved.
#[derive(Debug, Clone)]
struct ZonePlan {
    /// Visible rect of the zone.
    target: Rect,
    /// Rect passed to the platform (zone plus invisible borders).
    window_rect: Rect,
    before: Rect,
    zone_name: String,
    monitor_id: MonitorId,
    monitor_name: String,
}

enum AnimationKind {
    /// Shrinking from the app's own fullscreen rect into the zone.
    Enter(ZonePlan),
    /// Growing from where the app left fullscreen back to the pre-fullscreen window.
    Exit { pre: WindowState, who: String },
}

/// The managed window a panel belongs to, copied out so panels can be placed while `managed`
/// is borrowed.
#[derive(Debug, Clone, Copy)]
struct PanelOwner {
    id: WindowId,
    pid: u32,
    /// The app's own fullscreen rect, which it places its panels against.
    fullscreen: Rect,
    zone: Rect,
}

struct PanelState {
    owner: WindowId,
    /// Where the panel was when we last looked (after our move, if we moved it).
    last_seen: Option<Rect>,
    hidden: bool,
    user_moved: bool,
}

/// A move we drive in steps. While it runs, native events for the window are ignored; each
/// step checks the window is still where the previous step put it.
struct Animation {
    kind: AnimationKind,
    from: Rect,
    to: Rect,
    started: Instant,
    duration: Duration,
    next_step: Instant,
    last_set: Rect,
    restarts: u8,
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
    /// When the current monitor-sized burst started, so the settle wait stays capped.
    fullscreen_since: HashMap<WindowId, Instant>,
    display_pending: Option<Instant>,
    moving: HashSet<WindowId>,
    animations: HashMap<WindowId, Animation>,
    /// Panels of managed windows seen during the current fullscreen session.
    panels: HashMap<WindowId, PanelState>,
    activity: ActivityLog,
    events_active: bool,
}

fn label(identity: &WindowIdentity, id: WindowId) -> String {
    let name = if identity.process_name.is_empty() {
        "<unknown>"
    } else {
        &identity.process_name
    };
    format!("{name} {id}")
}

impl Engine {
    pub fn new(
        platform: Arc<dyn PlatformWindowManager>,
        paths: EnginePaths,
        notify: Notifier,
    ) -> Self {
        let mut activity = ActivityLog::default();
        let config = match AppConfig::load(&paths.config) {
            Ok(c) => c,
            Err(e) => {
                let backup = paths
                    .config
                    .with_extension(format!("json.broken-{}", activity::now_ms()));
                let _ = std::fs::rename(&paths.config, &backup);
                activity.push(
                    ActivityLevel::Warn,
                    None,
                    format!(
                        "Config could not be loaded ({e}); moved to {} and using defaults",
                        backup.display()
                    ),
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
            fullscreen_since: HashMap::new(),
            display_pending: None,
            moving: HashSet::new(),
            animations: HashMap::new(),
            panels: HashMap::new(),
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
        // Windows growing back to their own size are no longer managed; finish them before exit.
        let exits: Vec<WindowId> = self
            .animations
            .iter()
            .filter(|(_, a)| matches!(a.kind, AnimationKind::Exit { .. }))
            .map(|(id, _)| *id)
            .collect();
        for id in exits {
            if let Some(anim) = self.animations.remove(&id) {
                self.finish_animation(id, anim);
            }
        }
        self.release_all_with(ReleaseReason::Shutdown);
        journal::save(&self.paths.journal, &[]);
    }

    fn recover_journal(&mut self) {
        for entry in journal::load(&self.paths.journal) {
            let still_ours = self
                .platform
                .identity(entry.window)
                .is_ok_and(|i| i.process == entry.process)
                && self
                    .platform
                    .state(entry.window)
                    .is_ok_and(|s| s.bounds.approx_eq(&entry.applied.bounds, BOUNDS_TOLERANCE));
            if !still_ours {
                continue;
            }
            let who = Some(format!("{} {}", entry.process_name, entry.window));
            match self.platform.restore_state(entry.window, &entry.detected) {
                Ok(()) => {
                    if let Err(e) = self.platform.set_hides_taskbar(entry.window, true) {
                        tracing::warn!(window = %entry.window, "could not mark recovered fullscreen window: {e}");
                    }
                    self.log(
                        ActivityLevel::Info,
                        who,
                        "Restored window left constrained by a previous session".into(),
                    );
                }
                Err(e) => self.log(
                    ActivityLevel::Warn,
                    who,
                    format!("Could not restore window from previous session: {e}"),
                ),
            }
        }
        journal::save(&self.paths.journal, &[]);
    }

    fn scan_all(&mut self) {
        match self.platform.enumerate_windows() {
            Ok(ids) => ids.into_iter().for_each(|id| self.evaluate(id)),
            Err(e) => self.log(
                ActivityLevel::Error,
                None,
                format!("Window enumeration failed: {e}"),
            ),
        }
    }

    // ---- event intake ----------------------------------------------------------------------

    pub fn on_platform_event(&mut self, event: PlatformEvent) {
        let now = Instant::now();
        match event {
            PlatformEvent::WindowChanged(id) => {
                if self.moving.contains(&id) || self.animations.contains_key(&id) {
                    // A title-bar drag is the user, not a fullscreen transition.
                } else if self.is_monitor_fullscreen(id) {
                    let started = *self.fullscreen_since.entry(id).or_insert(now);
                    let deadline = (now + FULLSCREEN_SETTLE).min(started + FULLSCREEN_SETTLE_CAP);
                    self.pending.insert(id, deadline);
                } else if self.managed.contains_key(&id) || self.is_managed_app(id) {
                    self.fullscreen_since.remove(&id);
                    self.pending.insert(id, now);
                } else {
                    self.fullscreen_since.remove(&id);
                    self.pending.entry(id).or_insert(now + EVENT_SETTLE);
                }
            }
            PlatformEvent::WindowDestroyed(id) => self.forget(id),
            PlatformEvent::MoveSizeStart(id) => {
                // The user grabbed the window; stop driving it.
                self.animations.remove(&id);
                self.moving.insert(id);
            }
            PlatformEvent::MoveSizeEnd(id) => {
                self.moving.remove(&id);
                if let Some(panel) = self.panels.get_mut(&id) {
                    panel.user_moved = true;
                    panel.last_seen = None;
                }
                self.on_user_moved(id);
            }
            PlatformEvent::DisplayChanged => {
                self.display_pending.get_or_insert(now + DISPLAY_SETTLE);
            }
        }
    }

    /// Another window of an app we are constraining, e.g. a panel it just showed.
    fn is_managed_app(&self, id: WindowId) -> bool {
        !self.managed.is_empty()
            && self
                .platform
                .process_id(id)
                .is_some_and(|pid| self.managed.values().any(|m| m.identity.process.pid == pid))
    }

    fn is_monitor_fullscreen(&self, id: WindowId) -> bool {
        match self.platform.state(id) {
            Ok(state) => fullscreen_monitor(&state, &self.monitors).is_some(),
            Err(_) => false,
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending
            .values()
            .copied()
            .chain(self.display_pending)
            .chain(self.animations.values().map(|a| a.next_step))
            .min()
    }

    pub fn process_due(&mut self, now: Instant) {
        let steps: Vec<WindowId> = self
            .animations
            .iter()
            .filter(|(_, a)| a.next_step <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in steps {
            self.step_animation(id);
        }
        if self.display_pending.is_some_and(|t| t <= now) {
            self.display_pending = None;
            self.on_display_changed();
        }
        let due: Vec<WindowId> = self
            .pending
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            self.pending.remove(&id);
            self.evaluate(id);
        }
    }

    // ---- core state machine ----------------------------------------------------------------

    fn evaluate(&mut self, id: WindowId) {
        self.fullscreen_since.remove(&id);
        if !self.platform.exists(id) {
            self.forget(id);
            return;
        }
        if self.moving.contains(&id) || self.animations.contains_key(&id) {
            return;
        }
        if self.managed.contains_key(&id) {
            self.evaluate_managed(id);
            return;
        }
        if !self.config.enabled {
            return;
        }
        if self.place_companion_of_any(id) {
            return;
        }
        let Ok(identity) = self.platform.identity(id) else {
            return;
        };
        let Some(rule) = self.config.match_rule(&identity) else {
            return;
        };
        let Some(action) = rule.fullscreen_action() else {
            return;
        };
        let (rule_id, zone_id, chrome, use_snapped_zone) = (
            rule.id.clone(),
            action.zone_id.to_string(),
            action.chrome,
            action.use_snapped_zone,
        );

        let state = match self.platform.state(id) {
            Ok(s) => s,
            Err(PlatformError::WindowGone(_)) => return self.forget(id),
            Err(_) => return,
        };
        match fullscreen_monitor(&state, &self.monitors).map(|m| m.id.clone()) {
            Some(monitor) if !self.rejected.contains(&id) => {
                self.manage(
                    id,
                    identity,
                    rule_id,
                    zone_id,
                    chrome,
                    use_snapped_zone,
                    state,
                    monitor,
                );
            }
            Some(_) => {}
            None => {
                self.rejected.remove(&id);
                // Skip borderless frames seen while entering fullscreen; those are smaller than
                // the window the user had, and Firefox restores whatever it last observed.
                if is_resting_layout(&state) {
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
        use_snapped_zone: bool,
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
            use_snapped_zone,
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
        let duration = self.config.transition.duration();
        if duration.is_zero() {
            match self.apply_zone(&mut m, Some(&monitor)) {
                Ok(msg) => {
                    self.managed.insert(id, m);
                    self.place_companions(id);
                    self.save_journal();
                    self.log(ActivityLevel::Info, Some(who), msg);
                    (self.notify)(Notification::ManagedChanged);
                }
                Err(e) => self.reject_fullscreen(id, who, &state, e),
            }
            return;
        }
        match self.plan_zone(&m, Some(&monitor)) {
            Ok(plan) => {
                // Raise it now: the taskbar would otherwise show over the window as it shrinks.
                self.cover_taskbar(id);
                let to = plan.window_rect;
                self.start_animation(id, AnimationKind::Enter(plan), state.bounds, to, duration);
                self.managed.insert(id, m);
                self.save_journal();
                (self.notify)(Notification::ManagedChanged);
            }
            Err(e) => self.reject_fullscreen(id, who, &state, e),
        }
    }

    fn reject_fullscreen(&mut self, id: WindowId, who: String, state: &WindowState, error: String) {
        self.rejected.insert(id);
        // Undo any partial change (e.g. styles stripped but move refused).
        let _ = self.platform.restore_state(id, state);
        self.log(
            ActivityLevel::Warn,
            Some(who),
            format!("Fullscreen detected but could not constrain: {error}"),
        );
    }

    /// Positions the window in its zone. Returns a human-readable summary.
    fn apply_zone(
        &self,
        m: &mut ManagedWindow,
        window_monitor: Option<&MonitorId>,
    ) -> Result<String, String> {
        let plan = self.plan_zone(m, window_monitor)?;
        self.finish_zone(m, &plan)
    }

    /// Resolves the zone rect for this window and strips chrome if the rule asks for it.
    fn plan_zone(
        &self,
        m: &ManagedWindow,
        window_monitor: Option<&MonitorId>,
    ) -> Result<ZonePlan, String> {
        let window_monitor =
            window_monitor.and_then(|id| self.monitors.iter().find(|x| &x.id == id));
        let snapped = window_monitor
            .filter(|_| m.use_snapped_zone)
            .and_then(|monitor| {
                snapped_zone_target(m.pre_fullscreen.as_ref(), &m.detected, &monitor.bounds)
                    .map(|rect| (monitor, rect))
            });
        let (target, zone_name, monitor) = match snapped {
            Some((monitor, rect)) => (rect, "FancyZones zone".to_string(), monitor),
            None => {
                let zone = self
                    .config
                    .zone(&m.zone_id)
                    .ok_or_else(|| format!("zone '{}' no longer exists", m.zone_id))?;
                let monitor = zone
                    .target_monitor(&self.monitors, window_monitor)
                    .ok_or("no monitor available for zone")?;
                (zone.resolve_on(monitor), zone.name.clone(), monitor)
            }
        };

        if m.chrome == ChromeMode::Hide {
            self.platform
                .set_borderless(m.id)
                .map_err(|e| e.to_string())?;
        }
        let before = self.platform.state(m.id).map_err(|e| e.to_string())?;
        Ok(ZonePlan {
            target,
            window_rect: compensate_invisible_frame(target, &before),
            before: before.bounds,
            zone_name,
            monitor_id: monitor.id.clone(),
            monitor_name: monitor.friendly_name.clone(),
        })
    }

    /// Moves the window to the planned rect and verifies the app kept it there.
    fn finish_zone(&self, m: &mut ManagedWindow, plan: &ZonePlan) -> Result<String, String> {
        self.platform
            .set_bounds(m.id, plan.window_rect)
            .map_err(|e| e.to_string())?;
        let after = self.platform.state(m.id).map_err(|e| e.to_string())?;

        if !after
            .visible_bounds
            .approx_eq(&plan.target, BOUNDS_TOLERANCE)
        {
            return Err(format!(
                "application rejected the zone bounds (wanted {}, got {})",
                plan.target, after.visible_bounds
            ));
        }
        // Resize alone leaves the taskbar painted over the zone until the window is clicked.
        self.cover_taskbar(m.id);
        let after = self.platform.state(m.id).unwrap_or(after);
        let summary = format!(
            "Constrained to zone '{}' on {}: {} → {}",
            plan.zone_name, plan.monitor_name, plan.before, after.bounds
        );
        m.applied = after;
        m.target = plan.target;
        m.monitor = Some(plan.monitor_id.clone());
        Ok(summary)
    }

    fn panel_owner(&self, m: &ManagedWindow) -> Option<PanelOwner> {
        (!m.gave_up && !self.animations.contains_key(&m.id)).then_some(PanelOwner {
            id: m.id,
            pid: m.identity.process.pid,
            fullscreen: m.detected.bounds,
            zone: m.target,
        })
    }

    /// If `id` is a panel of a managed window, keeps it in that window's zone.
    /// Returns whether it is one, so it is not treated as an app window of its own.
    fn place_companion_of_any(&mut self, id: WindowId) -> bool {
        if self.managed.is_empty() {
            return false;
        }
        let Some(pid) = self.platform.process_id(id) else {
            return false;
        };
        let owners: Vec<PanelOwner> = self
            .managed
            .values()
            .filter(|m| m.identity.process.pid == pid)
            .filter_map(|m| self.panel_owner(m))
            .collect();
        owners
            .into_iter()
            .any(|owner| self.place_companion(id, owner))
    }

    /// Starts a fresh panel session for this window: panels it shows now or later are put in the
    /// zone again, even ones the user dragged out last time.
    fn place_companions(&mut self, id: WindowId) {
        let Some(owner) = self.managed.get(&id).and_then(|m| self.panel_owner(m)) else {
            return;
        };
        self.panels.retain(|_, p| p.owner != owner.id);
        let Ok(windows) = self.platform.process_windows(owner.pid) else {
            return;
        };
        for panel in windows {
            self.place_companion(panel, owner);
        }
    }

    /// A companion is a window of the same app that is owned by the managed window, or a
    /// borderless panel of it. Apps place those against their own fullscreen rect (the monitor),
    /// which leaves them outside the zone. Framed windows that are not owned (a second
    /// browser window) are separate app windows and never moved.
    ///
    /// Each panel is put in the zone when it shows. A move while it stays visible is the user
    /// (apps like VLC drag their panels themselves, so there is no system move loop to see),
    /// and the panel is then left alone until the next fullscreen.
    fn place_companion(&mut self, id: WindowId, owner: PanelOwner) -> bool {
        if id == owner.id {
            return false;
        }
        let Ok(state) = self.platform.state(id) else {
            return false;
        };
        let known = self.panels.get(&id).is_some_and(|p| p.owner == owner.id);
        if !state.visible || state.cloaked || state.show_state != ShowState::Normal {
            if let Some(panel) = self.panels.get_mut(&id).filter(|_| known) {
                panel.hidden = true;
            }
            return known;
        }
        if !known {
            if !self.is_owned_by(id, owner.id) && state.has_title_bar {
                return false;
            }
            if fullscreen_monitor(&state, &self.monitors).is_some() {
                return false;
            }
        }
        let panel = self.panels.entry(id).or_insert(PanelState {
            owner: owner.id,
            last_seen: None,
            hidden: false,
            user_moved: false,
        });
        let place = match panel.last_seen {
            None => !panel.user_moved,
            Some(last) if last.approx_eq(&state.bounds, 1) => false,
            // Re-shown by the app: follow the zone again unless the user already moved it.
            Some(_) if panel.hidden => !panel.user_moved,
            // Resized in place (content relayout), not dragged.
            Some(last)
                if (last.width(), last.height())
                    != (state.bounds.width(), state.bounds.height()) =>
            {
                false
            }
            Some(_) => {
                panel.user_moved = true;
                false
            }
        };
        panel.hidden = false;
        panel.last_seen = Some(state.bounds);
        if place {
            if let Some(rect) = map_companion(state.bounds, owner.fullscreen, owner.zone) {
                match self.platform.set_position(id, rect.left, rect.top) {
                    Ok(()) => {
                        tracing::info!(window = %id, owner = %owner.id, from = %state.bounds, to = %rect, "moved panel into zone");
                        if let Some(panel) = self.panels.get_mut(&id) {
                            panel.last_seen = Some(rect);
                        }
                    }
                    Err(e) => tracing::debug!(window = %id, "could not move panel into zone: {e}"),
                }
            }
        }
        true
    }

    fn is_owned_by(&self, id: WindowId, owner: WindowId) -> bool {
        let mut current = id;
        for _ in 0..MAX_OWNER_DEPTH {
            match self.platform.owner(current) {
                Some(o) if o == owner => return true,
                Some(o) => current = o,
                None => return false,
            }
        }
        false
    }

    fn start_animation(
        &mut self,
        id: WindowId,
        kind: AnimationKind,
        from: Rect,
        to: Rect,
        duration: Duration,
    ) {
        // Movement starts after the delay; the first step then checks nothing moved meanwhile.
        let started = Instant::now() + START_DELAY;
        self.pending.remove(&id);
        self.animations.insert(
            id,
            Animation {
                kind,
                from,
                to,
                started,
                duration,
                next_step: started,
                last_set: from,
                restarts: 0,
            },
        );
    }

    fn step_animation(&mut self, id: WindowId) {
        let state = match self.platform.state(id) {
            Ok(s) => s,
            Err(PlatformError::WindowGone(_)) => return self.forget(id),
            Err(e) => {
                tracing::debug!(window = %id, "animation state read failed: {e}");
                if let Some(anim) = self.animations.remove(&id) {
                    self.finish_animation(id, anim);
                }
                return;
            }
        };
        let now = Instant::now();
        let Some(anim) = self.animations.get(&id) else {
            return;
        };
        if !state.bounds.approx_eq(&anim.last_set, ANIMATION_TOLERANCE) {
            return self.on_animation_interrupted(id, state, now);
        }
        let (rect, done) = frame_at(
            anim.from,
            anim.to,
            now.saturating_duration_since(anim.started),
            anim.duration,
        );
        if done {
            if let Some(anim) = self.animations.remove(&id) {
                self.finish_animation(id, anim);
            }
            return;
        }
        if let Err(e) = self.platform.set_bounds(id, rect) {
            tracing::debug!(window = %id, "animation step failed: {e}");
            if let Some(anim) = self.animations.remove(&id) {
                self.finish_animation(id, anim);
            }
            return;
        }
        // Apps round or clamp sizes; compare the next step against what the window really took.
        let actual = self.platform.state(id).map(|s| s.bounds).unwrap_or(rect);
        if let Some(anim) = self.animations.get_mut(&id) {
            anim.last_set = actual;
            anim.next_step = now + ANIMATION_FRAME;
        }
    }

    /// Something other than the animation moved the window.
    fn on_animation_interrupted(&mut self, id: WindowId, state: WindowState, now: Instant) {
        let reasserted = fullscreen_monitor(&state, &self.monitors).is_some();
        let Some(anim) = self.animations.get_mut(&id) else {
            return;
        };
        match anim.kind {
            // Chromium finishes its own fullscreen transition a few hundred ms in and puts the
            // window back on the monitor rect. Carry on from there instead of starting over.
            AnimationKind::Enter(_) if reasserted && anim.restarts < MAX_ANIMATION_RESTARTS => {
                let remaining = anim
                    .duration
                    .saturating_sub(now.saturating_duration_since(anim.started));
                anim.from = state.bounds;
                anim.last_set = state.bounds;
                anim.started = now;
                anim.duration = remaining.max(MIN_RESTART_DURATION);
                anim.restarts += 1;
                anim.next_step = now;
            }
            AnimationKind::Enter(_) if reasserted => {
                if let Some(anim) = self.animations.remove(&id) {
                    self.finish_animation(id, anim);
                }
            }
            // The app left or re-entered fullscreen, or the user moved it: let normal handling decide.
            _ => {
                self.animations.remove(&id);
                self.evaluate(id);
            }
        }
    }

    fn finish_animation(&mut self, id: WindowId, anim: Animation) {
        match anim.kind {
            AnimationKind::Enter(plan) => {
                let Some(mut m) = self.managed.remove(&id) else {
                    return;
                };
                let who = label(&m.identity, id);
                match self.finish_zone(&mut m, &plan) {
                    Ok(msg) => {
                        self.managed.insert(id, m);
                        self.place_companions(id);
                        self.save_journal();
                        self.log(ActivityLevel::Info, Some(who), msg);
                    }
                    Err(e) => {
                        let detected = m.detected.clone();
                        self.save_journal();
                        self.reject_fullscreen(id, who, &detected, e);
                    }
                }
                (self.notify)(Notification::ManagedChanged);
            }
            AnimationKind::Exit { pre, who } => self.finish_exit(id, pre, who),
        }
    }

    /// Zone windows no longer cover the monitor, so the shell draws the taskbar on top of them
    /// until something activates the window. Raise the window and mark it fullscreen now.
    fn cover_taskbar(&self, id: WindowId) {
        if let Err(e) = self.platform.set_topmost(id, true) {
            tracing::warn!(window = %id, "could not raise window above the taskbar: {e}");
        }
        if let Err(e) = self.platform.set_hides_taskbar(id, true) {
            tracing::warn!(window = %id, "could not mark window fullscreen for the taskbar: {e}");
        }
    }

    fn restore_taskbar(&self, id: WindowId, topmost: bool) {
        if let Err(e) = self.platform.set_topmost(id, topmost) {
            tracing::warn!(window = %id, "could not restore topmost: {e}");
        }
        if let Err(e) = self.platform.set_hides_taskbar(id, false) {
            tracing::warn!(window = %id, "could not clear taskbar fullscreen mark: {e}");
        }
    }

    fn evaluate_managed(&mut self, id: WindowId) {
        let state = match self.platform.state(id) {
            Ok(s) => s,
            Err(PlatformError::WindowGone(_)) => return self.forget(id),
            Err(e) => return tracing::debug!(window = %id, "state read failed: {e}"),
        };
        let Some(m) = self.managed.get(&id) else {
            return;
        };
        match classify_managed(&state, &m.applied, &self.monitors) {
            ManagedObservation::Unchanged | ManagedObservation::Dormant => {}
            ManagedObservation::Refullscreened => self.reapply(id, state),
            ManagedObservation::Exited => self.release_after_exit(id, state),
        }
    }

    /// The app re-asserted fullscreen (monitor move, display change, or fighting us).
    fn reapply(&mut self, id: WindowId, state: WindowState) {
        let Some(mut m) = self.managed.remove(&id) else {
            return;
        };
        if m.gave_up {
            self.managed.insert(id, m);
            return;
        }
        let now = Instant::now();
        m.reapplies
            .retain(|t| now.duration_since(*t) < REAPPLY_WINDOW);
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
                self.log(
                    ActivityLevel::Warn,
                    Some(who),
                    format!("Could not re-apply zone: {e}"),
                );
                self.save_journal();
            }
        }
        (self.notify)(Notification::ManagedChanged);
    }

    /// The app left fullscreen. Firefox overwrites its saved restore rect when we resize the
    /// fullscreen window, so exiting comes back smaller than the window the user had. Put that
    /// window back ourselves.
    fn release_after_exit(&mut self, id: WindowId, current: WindowState) {
        let Some(m) = self.managed.remove(&id) else {
            return;
        };
        self.animations.remove(&id);
        self.forget_panels_of(id);
        let who = label(&m.identity, id);
        if let Some(pre) = m.pre_fullscreen.clone() {
            if let Err(e) = self.platform.set_hides_taskbar(id, false) {
                tracing::warn!(window = %id, "could not clear taskbar fullscreen mark: {e}");
            }
            tracing::info!(window = %who, app_left_at = %current.bounds, restoring = %pre.bounds, "fullscreen exited");
            let duration = self.config.transition.duration();
            if duration.is_zero() || current.bounds.approx_eq(&pre.bounds, BOUNDS_TOLERANCE) {
                self.finish_exit(id, pre, who);
            } else {
                // Frame and z-order first, so the window grows back with its title bar.
                if let Err(e) = self.platform.set_style(id, pre.style) {
                    tracing::warn!(window = %id, "could not restore styles before animating: {e}");
                }
                if let Err(e) = self.platform.set_topmost(id, pre.topmost) {
                    tracing::warn!(window = %id, "could not restore topmost: {e}");
                }
                let to = pre.bounds;
                self.start_animation(
                    id,
                    AnimationKind::Exit { pre, who },
                    current.bounds,
                    to,
                    duration,
                );
            }
        } else {
            let topmost = m.detected.topmost;
            self.restore_taskbar(id, topmost);
            tracing::info!(window = %who, bounds = %current.bounds, "fullscreen exited without a saved window");
            self.log(
                ActivityLevel::Info,
                Some(who),
                format!("Fullscreen exited; released at {}", current.bounds),
            );
            self.last_normal.insert(id, current);
        }
        self.save_journal();
        (self.notify)(Notification::ManagedChanged);
    }

    /// Puts the pre-fullscreen window back (styles, bounds, maximized state, z-order).
    fn finish_exit(&mut self, id: WindowId, pre: WindowState, who: String) {
        match self.platform.restore_state(id, &pre) {
            Ok(()) => self.log(
                ActivityLevel::Info,
                Some(who),
                format!("Fullscreen exited; restored {}", pre.bounds),
            ),
            Err(e) => self.log(
                ActivityLevel::Warn,
                Some(who),
                format!("Fullscreen exited but could not restore: {e}"),
            ),
        }
        self.last_normal.insert(id, pre);
    }

    /// Hands a still-fullscreen window back to the app's own fullscreen state.
    fn release(&mut self, id: WindowId, reason: ReleaseReason) {
        let Some(m) = self.managed.remove(&id) else {
            return;
        };
        self.forget_panels_of(id);
        let who = label(&m.identity, id);
        // Mid-shrink the window is neither at its fullscreen rect nor in the zone, but it is ours.
        let animating = self.animations.remove(&id).is_some();
        let in_zone = animating
            || self
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
                Ok(()) => {
                    if let Err(e) = self.platform.set_hides_taskbar(id, true) {
                        tracing::warn!(window = %id, "could not mark released fullscreen window: {e}");
                    }
                    self.log(
                        ActivityLevel::Info,
                        Some(who),
                        format!("Released ({reason:?}); restored {}", m.detected.bounds),
                    );
                }
                Err(e) => self.log(
                    ActivityLevel::Warn,
                    Some(who),
                    format!("Release ({reason:?}) could not restore: {e}"),
                ),
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
        let Some(m) = self.managed.get_mut(&id) else {
            return;
        };
        if let Ok(state) = self.platform.state(id) {
            // Respect manual placement while still considering the window managed.
            tracing::info!(window = %id, bounds = %state.bounds, "user moved managed window");
            m.applied = state;
            self.save_journal();
        }
    }

    /// Panel positions belong to one fullscreen session.
    fn forget_panels_of(&mut self, owner: WindowId) {
        self.panels.retain(|_, p| p.owner != owner);
    }

    fn forget(&mut self, id: WindowId) {
        self.pending.remove(&id);
        self.fullscreen_since.remove(&id);
        self.moving.remove(&id);
        self.animations.remove(&id);
        self.panels.remove(&id);
        self.forget_panels_of(id);
        self.last_normal.remove(&id);
        self.rejected.remove(&id);
        if let Some(m) = self.managed.remove(&id) {
            self.log(
                ActivityLevel::Info,
                Some(label(&m.identity, id)),
                "Window closed while managed".into(),
            );
            self.save_journal();
            (self.notify)(Notification::ManagedChanged);
        }
    }

    fn on_display_changed(&mut self) {
        self.refresh_monitors();
        (self.notify)(Notification::MonitorsChanged(self.monitors.clone()));
        self.log(
            ActivityLevel::Info,
            None,
            format!(
                "Display configuration changed ({} monitors)",
                self.monitors.len()
            ),
        );
        self.reapply_all();
    }

    fn reapply_all(&mut self) {
        let ids: Vec<WindowId> = self.managed.keys().copied().collect();
        for id in ids {
            let Some(mut m) = self.managed.remove(&id) else {
                continue;
            };
            self.animations.remove(&id);
            let Ok(state) = self.platform.state(id) else {
                self.forget(id);
                continue;
            };
            // If the monitor the app went fullscreen on is gone, hand back fullscreen on its current one.
            let detected_monitor_alive = m
                .detected
                .monitor
                .as_ref()
                .is_some_and(|mid| self.monitors.iter().any(|x| &x.id == mid));
            if !detected_monitor_alive {
                if let Some(cur) = state
                    .monitor
                    .as_ref()
                    .and_then(|mid| self.monitors.iter().find(|x| &x.id == mid))
                {
                    m.detected.bounds = cur.bounds;
                    m.detected.monitor = Some(cur.id.clone());
                }
            }
            m.gave_up = false;
            m.reapplies.clear();
            if let Err(e) = self.apply_zone(&mut m, state.monitor.as_ref()) {
                self.log(
                    ActivityLevel::Warn,
                    Some(label(&m.identity, id)),
                    format!("Could not re-apply zone: {e}"),
                );
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
                self.activity.push(
                    ActivityLevel::Error,
                    None,
                    format!("Monitor discovery failed: {e}"),
                );
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
        let ids = self
            .platform
            .enumerate_windows()
            .map_err(|e| e.to_string())?;
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
                    matched_rule: self.config.match_rule(&identity).map(|r| r.name.clone()),
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
                .then(|| self.config.match_rule(&self.managed[&id].identity))
                .flatten()
                .and_then(|r| {
                    r.fullscreen_action().map(|a| {
                        (
                            r.id.clone(),
                            a.zone_id.to_string(),
                            a.chrome,
                            a.use_snapped_zone,
                        )
                    })
                });
            let Some((rule_id, zone_id, chrome, use_snapped_zone)) = action else {
                self.release(id, ReleaseReason::ConfigChanged);
                continue;
            };
            let Some(mut m) = self.managed.remove(&id) else {
                continue;
            };
            self.animations.remove(&id);
            if m.chrome == ChromeMode::Hide && chrome == ChromeMode::Keep {
                let _ = self.platform.set_style(id, m.detected.style);
            }
            (m.rule_id, m.zone_id, m.chrome, m.use_snapped_zone) =
                (rule_id, zone_id, chrome, use_snapped_zone);
            m.gave_up = false;
            m.reapplies.clear();
            let monitor = m.monitor.clone();
            if let Err(e) = self.apply_zone(&mut m, monitor.as_ref()) {
                self.log(
                    ActivityLevel::Warn,
                    Some(label(&m.identity, id)),
                    format!("Could not apply updated zone: {e}"),
                );
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
