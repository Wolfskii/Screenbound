use serde::{Deserialize, Serialize};

use super::geometry::Rect;
use super::monitor::{MonitorId, MonitorInfo};

/// Rectangle expressed as fractions (0.0–1.0) of a reference rect, so zones stay valid across
/// resolutions and monitors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl NormalizedRect {
    pub const FULL: NormalizedRect = NormalizedRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };

    /// Clamps into the unit square and enforces a minimum size so a zone can never collapse.
    pub fn sanitized(self) -> NormalizedRect {
        const MIN: f64 = 0.02;
        let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
        let width = finite(self.width, 1.0).clamp(MIN, 1.0);
        let height = finite(self.height, 1.0).clamp(MIN, 1.0);
        let x = finite(self.x, 0.0).clamp(0.0, 1.0 - width);
        let y = finite(self.y, 0.0).clamp(0.0, 1.0 - height);
        NormalizedRect { x, y, width, height }
    }

    /// Maps onto `reference` in physical pixels. Edges are rounded independently so adjacent
    /// zones (e.g. 0.75 + 0.25) share an edge without gaps or overlap.
    pub fn resolve(&self, reference: &Rect) -> Rect {
        let n = self.sanitized();
        let w = f64::from(reference.width());
        let h = f64::from(reference.height());
        let edge = |origin: i32, span: f64, frac: f64| origin + (span * frac).round() as i32;
        Rect::new(
            edge(reference.left, w, n.x),
            edge(reference.top, h, n.y),
            edge(reference.left, w, n.x + n.width),
            edge(reference.top, h, n.y + n.height),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoneReference {
    /// Full monitor bounds (what fullscreen normally covers).
    #[default]
    Monitor,
    /// Monitor work area (excludes taskbar).
    WorkArea,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub rect: NormalizedRect,
    /// Pin the zone to a monitor. `None` = apply on whichever monitor the window is on.
    #[serde(default)]
    pub monitor: Option<MonitorId>,
    #[serde(default)]
    pub reference: ZoneReference,
}

impl Zone {
    /// Picks the monitor the zone should land on: the pinned monitor if it is connected,
    /// otherwise the window's current monitor.
    pub fn target_monitor<'a>(
        &self,
        monitors: &'a [MonitorInfo],
        window_monitor: Option<&'a MonitorInfo>,
    ) -> Option<&'a MonitorInfo> {
        self.monitor
            .as_ref()
            .and_then(|id| monitors.iter().find(|m| &m.id == id))
            .or(window_monitor)
    }

    pub fn resolve_on(&self, monitor: &MonitorInfo) -> Rect {
        let reference = match self.reference {
            ZoneReference::Monitor => monitor.bounds,
            ZoneReference::WorkArea => monitor.work_area,
        };
        self.rect.resolve(&reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::monitor::test_monitor;

    fn nr(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
        NormalizedRect { x, y, width, height }
    }

    #[test]
    fn resolves_left_75_on_super_ultrawide() {
        let monitor = Rect::new(0, 0, 5120, 1440);
        assert_eq!(nr(0.0, 0.0, 0.75, 1.0).resolve(&monitor), Rect::new(0, 0, 3840, 1440));
    }

    #[test]
    fn adjacent_zones_share_edges_on_odd_sizes() {
        let monitor = Rect::new(-1366, 0, 0, 768);
        for split in [0.25, 1.0 / 3.0, 0.5, 0.75] {
            let a = nr(0.0, 0.0, split, 1.0).resolve(&monitor);
            let b = nr(split, 0.0, 1.0 - split, 1.0).resolve(&monitor);
            assert_eq!(a.right, b.left, "gap/overlap at split {split}");
            assert_eq!(a.left, monitor.left);
            assert_eq!(b.right, monitor.right);
        }
    }

    #[test]
    fn resolves_with_negative_origin() {
        let monitor = Rect::new(-2560, -1440, 0, 0);
        assert_eq!(nr(0.5, 0.5, 0.5, 0.5).resolve(&monitor), Rect::new(-1280, -720, 0, 0));
    }

    #[test]
    fn sanitize_clamps_invalid_input() {
        let s = nr(0.9, -1.0, 0.5, f64::NAN).sanitized();
        assert_eq!(s.width, 0.5);
        assert_eq!(s.x, 0.5);
        assert_eq!(s.y, 0.0);
        assert_eq!(s.height, 1.0);
        assert!(nr(0.0, 0.0, 0.0, 0.0).sanitized().width > 0.0);
    }

    #[test]
    fn work_area_reference() {
        let m = test_monitor("m", Rect::new(0, 0, 1920, 1080));
        let zone = Zone {
            id: "z".into(),
            name: "z".into(),
            rect: NormalizedRect::FULL,
            monitor: None,
            reference: ZoneReference::WorkArea,
        };
        assert_eq!(zone.resolve_on(&m), m.work_area);
    }

    #[test]
    fn pinned_monitor_falls_back_to_window_monitor_when_disconnected() {
        let a = test_monitor("a", Rect::new(0, 0, 1920, 1080));
        let b = test_monitor("b", Rect::new(1920, 0, 3840, 1080));
        let monitors = vec![a, b];
        let mut zone = Zone {
            id: "z".into(),
            name: "z".into(),
            rect: NormalizedRect::FULL,
            monitor: Some(MonitorId("b".into())),
            reference: ZoneReference::Monitor,
        };
        assert_eq!(zone.target_monitor(&monitors, Some(&monitors[0])).unwrap().id.0, "b");
        zone.monitor = Some(MonitorId("gone".into()));
        assert_eq!(zone.target_monitor(&monitors, Some(&monitors[0])).unwrap().id.0, "a");
    }

    #[test]
    fn zone_json_roundtrip() {
        let json = r#"{"id":"left-75","name":"75% Left","rect":{"x":0.0,"y":0.0,"width":0.75,"height":1.0}}"#;
        let zone: Zone = serde_json::from_str(json).unwrap();
        assert_eq!(zone.monitor, None);
        assert_eq!(zone.reference, ZoneReference::Monitor);
        let back: Zone = serde_json::from_str(&serde_json::to_string(&zone).unwrap()).unwrap();
        assert_eq!(back, zone);
    }
}
