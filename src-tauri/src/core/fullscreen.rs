//! Platform-independent fullscreen heuristics. Platforms supply `WindowState` snapshots;
//! the decisions about what those snapshots mean live here so they can be unit tested.

use super::geometry::Rect;
use super::monitor::{monitor_for_rect, MonitorInfo};
use super::window::{ShowState, WindowState};

/// Pixel slack when comparing our applied bounds with what the window reports back.
pub const BOUNDS_TOLERANCE: i32 = 2;

/// A window is "fullscreen-like" when it is a visible, borderless, non-maximized window whose
/// bounds cover an entire monitor. This is how Chromium and Firefox implement both HTML5 and
/// F11 fullscreen on Windows (same top-level HWND, caption/frame stripped, bounds = monitor),
/// and it also matches borderless-windowed games/players. Exclusive (DXGI/D3D) fullscreen
/// can look identical but cannot be controlled this way; see docs/fullscreen-research.md.
pub fn fullscreen_monitor<'a>(state: &WindowState, monitors: &'a [MonitorInfo]) -> Option<&'a MonitorInfo> {
    if !state.visible || state.cloaked || state.show_state != ShowState::Normal {
        return None;
    }
    if state.has_title_bar || state.has_resize_frame {
        return None;
    }
    let monitor = monitor_for_rect(monitors, &state.bounds)?;
    state.bounds.contains_rect(&monitor.bounds).then_some(monitor)
}

pub fn is_fullscreen_like(state: &WindowState, monitors: &[MonitorInfo]) -> bool {
    fullscreen_monitor(state, monitors).is_some()
}

/// A window the user was actually looking at, as opposed to a borderless in-between rect
/// Firefox and Chromium pass through while entering fullscreen. Those transients are smaller
/// than the pre-fullscreen window and must not be what we restore on exit.
pub fn is_resting_layout(state: &WindowState) -> bool {
    state.visible
        && !state.cloaked
        && state.show_state != ShowState::Minimized
        && (state.has_title_bar || state.show_state == ShowState::Maximized)
}

/// Chromium shrinks a background fullscreen window by 1px so the taskbar stops treating it as
/// fullscreen; for windows we already manage, that still means "the app wants fullscreen".
const NEAR_FULLSCREEN_TOLERANCE: i32 = 2;

fn is_near_fullscreen(state: &WindowState, monitors: &[MonitorInfo]) -> bool {
    state.visible
        && !state.cloaked
        && state.show_state == ShowState::Normal
        && state.is_borderless()
        && monitors.iter().any(|m| m.bounds.approx_eq(&state.bounds, NEAR_FULLSCREEN_TOLERANCE))
}

/// What a state change on a window we are currently managing means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedObservation {
    /// The window still looks exactly like we left it (typically our own SetWindowPos echo).
    Unchanged,
    /// The app re-asserted fullscreen (possibly on another monitor): re-apply the zone.
    Refullscreened,
    /// Minimized/hidden/cloaked: keep managing but do nothing now.
    Dormant,
    /// Bounds or chrome changed to a non-fullscreen state: the app left fullscreen.
    Exited,
}

pub fn classify_managed(current: &WindowState, applied: &WindowState, monitors: &[MonitorInfo]) -> ManagedObservation {
    if !current.visible || current.cloaked || current.show_state == ShowState::Minimized {
        return ManagedObservation::Dormant;
    }
    // Compare chrome flags rather than raw styles: apps toggle state bits like WS_VISIBLE
    // transiently (Chromium's redraw lock), which must not read as "exited".
    if current.bounds.approx_eq(&applied.bounds, BOUNDS_TOLERANCE)
        && current.has_title_bar == applied.has_title_bar
        && current.has_resize_frame == applied.has_resize_frame
        && current.show_state == applied.show_state
    {
        return ManagedObservation::Unchanged;
    }
    if is_fullscreen_like(current, monitors) || is_near_fullscreen(current, monitors) {
        return ManagedObservation::Refullscreened;
    }
    ManagedObservation::Exited
}

/// The external zone (FancyZones) the window sat in before going fullscreen, as a visible rect.
/// Only used when that zone is on the monitor the window went fullscreen on; a window snapped
/// on another monitor, or maximized, has no zone to fill here.
pub fn snapped_zone_target(pre: Option<&WindowState>, detected: &WindowState, monitor: &Rect) -> Option<Rect> {
    let pre = pre?;
    let snapped = pre.zone_snapped || detected.zone_snapped;
    let zone = pre.visible_bounds;
    (snapped && pre.show_state == ShowState::Normal && !zone.is_empty() && monitor.contains_rect(&zone)).then_some(zone)
}

/// Where to put a panel the app placed relative to its own fullscreen rect (e.g. VLC's
/// fullscreen controls): the same relative spot in the zone, same size, kept inside the zone.
/// `None` when the panel is not on that fullscreen rect, or its centre is already in the zone
/// (menus and popups anchored to the constrained window).
pub fn map_companion(panel: Rect, fullscreen: Rect, zone: Rect) -> Option<Rect> {
    if panel.is_empty() || fullscreen.is_empty() || zone.is_empty() {
        return None;
    }
    let cx = f64::from(panel.left) + f64::from(panel.width()) / 2.0;
    let cy = f64::from(panel.top) + f64::from(panel.height()) / 2.0;
    let inside = |r: &Rect| {
        cx >= f64::from(r.left) && cx < f64::from(r.right) && cy >= f64::from(r.top) && cy < f64::from(r.bottom)
    };
    if !inside(&fullscreen) || inside(&zone) {
        return None;
    }
    let fx = (cx - f64::from(fullscreen.left)) / f64::from(fullscreen.width());
    let fy = (cy - f64::from(fullscreen.top)) / f64::from(fullscreen.height());
    let place = |start: i32, span: i32, fraction: f64, size: i32| {
        let centred = (f64::from(start) + fraction * f64::from(span) - f64::from(size) / 2.0).round() as i32;
        centred.clamp(start, (start + span - size).max(start))
    };
    let left = place(zone.left, zone.width(), fx, panel.width());
    let top = place(zone.top, zone.height(), fy, panel.height());
    Some(Rect::from_xywh(left, top, panel.width(), panel.height()))
}

/// Adjusts a desired *visible* rect into the rect to pass to the platform, compensating for
/// invisible resize borders (Windows 10+ draws ~7px transparent borders on framed windows).
pub fn compensate_invisible_frame(target_visible: Rect, state: &WindowState) -> Rect {
    use super::geometry::Insets;
    let insets = Insets::between(&state.bounds, &state.visible_bounds);
    if insets.is_zero() || insets.left < 0 || insets.top < 0 || insets.right < 0 || insets.bottom < 0 {
        target_visible
    } else {
        target_visible.expand(insets)
    }
}

#[cfg(test)]
pub(crate) fn test_state(bounds: Rect) -> WindowState {
    use super::window::NativeStyle;
    WindowState {
        bounds,
        visible_bounds: bounds,
        restore_bounds: bounds,
        show_state: ShowState::Normal,
        visible: true,
        cloaked: false,
        has_title_bar: false,
        has_resize_frame: false,
        topmost: false,
        style: NativeStyle { primary: 0x1600_0000, extended: 0 },
        monitor: None,
        dpi: 96,
        zone_snapped: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::monitor::test_monitor;
    use crate::core::window::NativeStyle;

    fn monitors() -> Vec<MonitorInfo> {
        vec![
            test_monitor("side", Rect::new(-1920, 0, 0, 1080)),
            test_monitor("main", Rect::new(0, 0, 5120, 1440)),
        ]
    }

    #[test]
    fn borderless_monitor_sized_window_is_fullscreen() {
        let ms = monitors();
        let s = test_state(Rect::new(0, 0, 5120, 1440));
        assert_eq!(fullscreen_monitor(&s, &ms).unwrap().id.0, "main");
        let s = test_state(Rect::new(-1920, 0, 0, 1080));
        assert_eq!(fullscreen_monitor(&s, &ms).unwrap().id.0, "side");
    }

    #[test]
    fn resting_layout_ignores_borderless_transition() {
        let mut framed = test_state(Rect::new(100, 100, 1400, 900));
        framed.has_title_bar = true;
        framed.has_resize_frame = true;
        assert!(is_resting_layout(&framed));

        let mut maximized = framed.clone();
        maximized.show_state = ShowState::Maximized;
        assert!(is_resting_layout(&maximized));

        let entering = test_state(Rect::new(100, 100, 400, 300));
        assert!(!is_resting_layout(&entering));
    }

    #[test]
    fn framed_maximized_or_partial_windows_are_not_fullscreen() {
        let ms = monitors();
        let mut s = test_state(Rect::new(0, 0, 5120, 1440));
        s.has_title_bar = true;
        assert!(!is_fullscreen_like(&s, &ms));

        let mut s = test_state(Rect::new(-8, -8, 5128, 1400));
        s.show_state = ShowState::Maximized;
        assert!(!is_fullscreen_like(&s, &ms));

        let s = test_state(Rect::new(0, 0, 3840, 1440));
        assert!(!is_fullscreen_like(&s, &ms));

        let mut s = test_state(Rect::new(0, 0, 5120, 1440));
        s.cloaked = true;
        assert!(!is_fullscreen_like(&s, &ms));
    }

    #[test]
    fn managed_classification() {
        let ms = monitors();
        let applied = test_state(Rect::new(0, 0, 3840, 1440));

        let mut echo = applied.clone();
        echo.bounds.right += 1;
        assert_eq!(classify_managed(&echo, &applied, &ms), ManagedObservation::Unchanged);

        let refs = test_state(Rect::new(0, 0, 5120, 1440));
        assert_eq!(classify_managed(&refs, &applied, &ms), ManagedObservation::Refullscreened);

        let moved_monitor = test_state(Rect::new(-1920, 0, 0, 1080));
        assert_eq!(classify_managed(&moved_monitor, &applied, &ms), ManagedObservation::Refullscreened);

        let background_hack = test_state(Rect::new(0, 0, 5120, 1439));
        assert!(!is_fullscreen_like(&background_hack, &ms));
        assert_eq!(classify_managed(&background_hack, &applied, &ms), ManagedObservation::Refullscreened);

        let mut redraw_lock = applied.clone();
        redraw_lock.style.primary &= !0x1000_0000;
        assert_eq!(classify_managed(&redraw_lock, &applied, &ms), ManagedObservation::Unchanged);

        let mut exited = test_state(Rect::new(200, 200, 1800, 1000));
        exited.has_title_bar = true;
        exited.has_resize_frame = true;
        exited.style = NativeStyle { primary: 0x16CF_0000, extended: 0x100 };
        assert_eq!(classify_managed(&exited, &applied, &ms), ManagedObservation::Exited);

        let mut restored_style_same_bounds = applied.clone();
        restored_style_same_bounds.style.primary |= 0x00C0_0000;
        restored_style_same_bounds.has_title_bar = true;
        assert_eq!(
            classify_managed(&restored_style_same_bounds, &applied, &ms),
            ManagedObservation::Exited
        );

        let mut minimized = applied.clone();
        minimized.show_state = ShowState::Minimized;
        assert_eq!(classify_managed(&minimized, &applied, &ms), ManagedObservation::Dormant);
    }

    #[test]
    fn snapped_zone_is_the_pre_fullscreen_visible_rect() {
        let monitor = Rect::new(0, 0, 5120, 1440);
        let detected = test_state(monitor);
        let mut pre = test_state(Rect::new(1273, -7, 3847, 1447));
        pre.visible_bounds = Rect::new(1280, 0, 3840, 1440);
        pre.has_title_bar = true;

        assert_eq!(snapped_zone_target(Some(&pre), &detected, &monitor), None);
        pre.zone_snapped = true;
        assert_eq!(snapped_zone_target(Some(&pre), &detected, &monitor), Some(Rect::new(1280, 0, 3840, 1440)));

        let other_monitor = Rect::new(-1920, 0, 0, 1080);
        assert_eq!(snapped_zone_target(Some(&pre), &detected, &other_monitor), None);

        let mut maximized = pre.clone();
        maximized.show_state = ShowState::Maximized;
        assert_eq!(snapped_zone_target(Some(&maximized), &detected, &monitor), None);

        assert_eq!(snapped_zone_target(None, &detected, &monitor), None);
    }

    #[test]
    fn companions_keep_their_relative_spot_in_the_zone() {
        let fullscreen = Rect::new(0, 0, 5120, 1440);
        let zone = Rect::new(0, 0, 2560, 1440);

        // Bottom-right controls on the monitor land bottom-right in the zone, same size.
        let controls = Rect::from_xywh(3400, 1300, 1400, 80);
        let placed = map_companion(controls, fullscreen, zone).unwrap();
        assert_eq!((placed.width(), placed.height()), (1400, 80));
        assert!(zone.contains_rect(&placed));
        assert_eq!(placed.bottom, 1380);
        assert_eq!(placed.right, 2560);

        // Centred on the monitor → centred in the zone.
        let dialog = Rect::from_xywh(2260, 520, 600, 400);
        assert_eq!(map_companion(dialog, fullscreen, zone), Some(Rect::from_xywh(980, 520, 600, 400)));
        let right_half = Rect::from_xywh(3540, 520, 600, 400);
        // Centre at 75% of the monitor → centre at 75% of the zone (1920).
        assert_eq!(map_companion(right_half, fullscreen, zone).unwrap().left, 1620);

        // Already in the zone, or on another monitor: left alone.
        assert_eq!(map_companion(Rect::from_xywh(100, 100, 300, 200), fullscreen, zone), None);
        assert_eq!(map_companion(Rect::from_xywh(-1500, 100, 300, 200), fullscreen, zone), None);

        // Wider than the zone: pinned to the zone's left edge.
        let wide = Rect::from_xywh(2800, 1300, 3000, 80);
        assert_eq!(map_companion(wide, fullscreen, zone).unwrap().left, 0);
    }

    #[test]
    fn compensates_invisible_borders() {
        let mut s = test_state(Rect::new(93, 0, 1007, 707));
        s.visible_bounds = Rect::new(100, 0, 1000, 700);
        let target = Rect::new(0, 0, 3840, 1440);
        assert_eq!(compensate_invisible_frame(target, &s), Rect::new(-7, 0, 3847, 1447));

        let borderless = test_state(Rect::new(0, 0, 10, 10));
        assert_eq!(compensate_invisible_frame(target, &borderless), target);
    }
}
