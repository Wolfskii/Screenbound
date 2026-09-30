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
    pub const FULL: NormalizedRect = NormalizedRect {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };

    /// Clamps into the unit square and enforces a minimum size so a zone can never collapse.
    pub fn sanitized(self) -> NormalizedRect {
        const MIN: f64 = 0.02;
        let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
        let width = finite(self.width, 1.0).clamp(MIN, 1.0);
        let height = finite(self.height, 1.0).clamp(MIN, 1.0);
        let x = finite(self.x, 0.0).clamp(0.0, 1.0 - width);
        let y = finite(self.y, 0.0).clamp(0.0, 1.0 - height);
        NormalizedRect {
            x,
            y,
            width,
            height,
        }
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

/// Rectangle in physical pixels, offset from the reference rect's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl PixelRect {
    pub const MIN_SIZE: i32 = 32;

    pub fn sanitized(self) -> PixelRect {
        PixelRect {
            x: self.x.max(0),
            y: self.y.max(0),
            width: self.width.max(Self::MIN_SIZE),
            height: self.height.max(Self::MIN_SIZE),
        }
    }

    /// Maps onto `reference`, shrinking then shifting so the zone always fits a smaller monitor.
    pub fn resolve(&self, reference: &Rect) -> Rect {
        let p = self.sanitized();
        let ref_w = reference.width().max(1);
        let ref_h = reference.height().max(1);
        let width = p.width.min(ref_w);
        let height = p.height.min(ref_h);
        let x = p.x.min(ref_w - width);
        let y = p.y.min(ref_h - height);
        Rect::new(
            reference.left + x,
            reference.top + y,
            reference.left + x + width,
            reference.top + y + height,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoneUnit {
    /// `rect` fractions of the reference; adapts to any resolution.
    #[default]
    Percent,
    /// `pixels` exact physical size and offset.
    Pixels,
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
    #[serde(default)]
    pub unit: ZoneUnit,
    /// Used when `unit` is `Pixels`; falls back to `rect` if missing.
    #[serde(default)]
    pub pixels: Option<PixelRect>,
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
        match (self.unit, self.pixels) {
            (ZoneUnit::Pixels, Some(pixels)) => pixels.resolve(&reference),
            _ => self.rect.resolve(&reference),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::monitor::test_monitor;

    fn nr(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
        NormalizedRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn resolves_left_75_on_super_ultrawide() {
        let monitor = Rect::new(0, 0, 5120, 1440);
        assert_eq!(
            nr(0.0, 0.0, 0.75, 1.0).resolve(&monitor),
            Rect::new(0, 0, 3840, 1440)
        );
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
        assert_eq!(
            nr(0.5, 0.5, 0.5, 0.5).resolve(&monitor),
            Rect::new(-1280, -720, 0, 0)
        );
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
            unit: ZoneUnit::Percent,
            pixels: None,
            monitor: None,
            reference: ZoneReference::WorkArea,
        };
        assert_eq!(zone.resolve_on(&m), m.work_area);
    }

    fn px(x: i32, y: i32, width: i32, height: i32) -> PixelRect {
        PixelRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn pixel_zone_is_offset_from_reference_origin() {
        let monitor = Rect::new(-5120, 0, 0, 1440);
        assert_eq!(
            px(640, 0, 3840, 1440).resolve(&monitor),
            Rect::new(-4480, 0, -640, 1440)
        );
    }

    #[test]
    fn pixel_zone_shrinks_and_shifts_to_fit_smaller_monitor() {
        let monitor = Rect::new(0, 0, 1920, 1080);
        assert_eq!(
            px(1000, 0, 3840, 1440).resolve(&monitor),
            Rect::new(0, 0, 1920, 1080)
        );
        assert_eq!(
            px(1500, 100, 1000, 500).resolve(&monitor),
            Rect::new(920, 100, 1920, 600)
        );
    }

    #[test]
    fn pixel_zone_sanitizes_negative_and_tiny_values() {
        let s = px(-10, -5, 0, 5).sanitized();
        assert_eq!(s, px(0, 0, PixelRect::MIN_SIZE, PixelRect::MIN_SIZE));
    }

    #[test]
    fn pixel_unit_without_pixels_falls_back_to_rect() {
        let m = test_monitor("m", Rect::new(0, 0, 2000, 1000));
        let mut zone = Zone {
            id: "z".into(),
            name: "z".into(),
            rect: nr(0.0, 0.0, 0.5, 1.0),
            unit: ZoneUnit::Pixels,
            pixels: None,
            monitor: None,
            reference: ZoneReference::Monitor,
        };
        assert_eq!(zone.resolve_on(&m), Rect::new(0, 0, 1000, 1000));
        zone.pixels = Some(px(100, 0, 1200, 900));
        assert_eq!(zone.resolve_on(&m), Rect::new(100, 0, 1300, 900));
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
            unit: ZoneUnit::Percent,
            pixels: None,
            monitor: Some(MonitorId("b".into())),
            reference: ZoneReference::Monitor,
        };
        assert_eq!(
            zone.target_monitor(&monitors, Some(&monitors[0]))
                .unwrap()
                .id
                .0,
            "b"
        );
        zone.monitor = Some(MonitorId("gone".into()));
        assert_eq!(
            zone.target_monitor(&monitors, Some(&monitors[0]))
                .unwrap()
                .id
                .0,
            "a"
        );
    }

    #[test]
    fn zone_json_roundtrip() {
        let json = r#"{"id":"left-75","name":"75% Left","rect":{"x":0.0,"y":0.0,"width":0.75,"height":1.0}}"#;
        let zone: Zone = serde_json::from_str(json).unwrap();
        assert_eq!(zone.monitor, None);
        assert_eq!(zone.reference, ZoneReference::Monitor);
        assert_eq!(zone.unit, ZoneUnit::Percent);
        assert_eq!(zone.pixels, None);
        let back: Zone = serde_json::from_str(&serde_json::to_string(&zone).unwrap()).unwrap();
        assert_eq!(back, zone);
    }
}
