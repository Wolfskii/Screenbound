use serde::{Deserialize, Serialize};

use super::geometry::Rect;

/// Stable-ish monitor identifier. On Windows this is the monitor device interface path, which
/// survives reboots and display re-numbering better than the GDI device name (`\\.\DISPLAY1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonitorId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: MonitorId,
    /// OS-level device name, e.g. `\\.\DISPLAY1`. Not stable across reconfiguration.
    pub device_name: String,
    /// Human-readable name, e.g. "Odyssey G9". Falls back to the device name.
    pub friendly_name: String,
    /// 1-based display number as shown by the OS where available.
    pub number: u32,
    pub is_primary: bool,
    /// Full monitor bounds in physical desktop pixels.
    pub bounds: Rect,
    /// Bounds excluding taskbar/app bars.
    pub work_area: Rect,
    /// Effective DPI (96 == 100%).
    pub dpi: u32,
}

impl MonitorInfo {
    pub fn scale_factor(&self) -> f64 {
        f64::from(self.dpi) / 96.0
    }
}

/// Picks the monitor that shares the largest area with `rect`, mirroring `MonitorFromRect`
/// with `MONITOR_DEFAULTTONEAREST` semantics for overlapping rects.
pub fn monitor_for_rect<'a>(monitors: &'a [MonitorInfo], rect: &Rect) -> Option<&'a MonitorInfo> {
    monitors
        .iter()
        .map(|m| (m, m.bounds.intersection(rect).map_or(0, |i| i.area())))
        .filter(|(_, area)| *area > 0)
        .max_by_key(|(_, area)| *area)
        .map(|(m, _)| m)
}

#[cfg(test)]
pub(crate) fn test_monitor(id: &str, bounds: Rect) -> MonitorInfo {
    MonitorInfo {
        id: MonitorId(id.into()),
        device_name: id.into(),
        friendly_name: id.into(),
        number: 1,
        is_primary: bounds.left == 0 && bounds.top == 0,
        bounds,
        work_area: Rect::new(bounds.left, bounds.top, bounds.right, bounds.bottom - 48),
        dpi: 96,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_monitor_with_largest_overlap_including_negative_coords() {
        let left = test_monitor("left", Rect::new(-2560, -360, 0, 1080));
        let main = test_monitor("main", Rect::new(0, 0, 5120, 1440));
        let monitors = vec![left, main];

        let on_left = Rect::new(-2560, -360, 0, 1080);
        assert_eq!(monitor_for_rect(&monitors, &on_left).unwrap().id.0, "left");

        let straddling_mostly_main = Rect::new(-100, 0, 2000, 1000);
        assert_eq!(
            monitor_for_rect(&monitors, &straddling_mostly_main)
                .unwrap()
                .id
                .0,
            "main"
        );

        let off_screen = Rect::new(10_000, 10_000, 10_100, 10_100);
        assert!(monitor_for_rect(&monitors, &off_screen).is_none());
    }
}
