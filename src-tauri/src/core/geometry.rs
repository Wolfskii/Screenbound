use serde::{Deserialize, Serialize};

/// Axis-aligned rectangle in physical desktop pixels. `right`/`bottom` are exclusive,
/// matching Win32 `RECT` semantics. Coordinates may be negative on multi-monitor setups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }

    pub const fn from_xywh(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub const fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }

    pub const fn contains_rect(&self, other: &Rect) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let r = Rect::new(
            self.left.max(other.left),
            self.top.max(other.top),
            self.right.min(other.right),
            self.bottom.min(other.bottom),
        );
        (!r.is_empty()).then_some(r)
    }

    pub fn area(&self) -> i64 {
        if self.is_empty() {
            0
        } else {
            i64::from(self.width()) * i64::from(self.height())
        }
    }

    /// True when every edge is within `tolerance` pixels of `other`.
    pub fn approx_eq(&self, other: &Rect, tolerance: i32) -> bool {
        (self.left - other.left).abs() <= tolerance
            && (self.top - other.top).abs() <= tolerance
            && (self.right - other.right).abs() <= tolerance
            && (self.bottom - other.bottom).abs() <= tolerance
    }

    /// Grows each edge outward by the given insets (used to compensate invisible resize borders).
    pub const fn expand(&self, insets: Insets) -> Rect {
        Rect::new(
            self.left - insets.left,
            self.top - insets.top,
            self.right + insets.right,
            self.bottom + insets.bottom,
        )
    }
}

impl std::fmt::Display for Rect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}) {}x{}", self.left, self.top, self.width(), self.height())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    /// Insets of `inner` relative to `outer` (how far each edge of `inner` sits inside `outer`).
    pub const fn between(outer: &Rect, inner: &Rect) -> Insets {
        Insets {
            left: inner.left - outer.left,
            top: inner.top - outer.top,
            right: outer.right - inner.right,
            bottom: outer.bottom - inner.bottom,
        }
    }

    pub const fn is_zero(&self) -> bool {
        self.left == 0 && self.top == 0 && self.right == 0 && self.bottom == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_with_negative_origin() {
        let r = Rect::new(-1920, -200, 0, 880);
        assert_eq!(r.width(), 1920);
        assert_eq!(r.height(), 1080);
        assert_eq!(r.to_string(), "(-1920, -200) 1920x1080");
    }

    #[test]
    fn containment_and_intersection() {
        let monitor = Rect::new(0, 0, 3440, 1440);
        assert!(monitor.contains_rect(&Rect::new(0, 0, 3440, 1440)));
        assert!(!monitor.contains_rect(&Rect::new(-1, 0, 3440, 1440)));
        assert_eq!(
            monitor.intersection(&Rect::new(3000, 1000, 4000, 2000)),
            Some(Rect::new(3000, 1000, 3440, 1440))
        );
        assert_eq!(monitor.intersection(&Rect::new(3440, 0, 4000, 10)), None);
    }

    #[test]
    fn insets_roundtrip() {
        let visible = Rect::new(100, 100, 900, 700);
        let window = Rect::new(93, 100, 907, 707);
        let insets = Insets::between(&window, &visible);
        assert_eq!(insets, Insets { left: 7, top: 0, right: 7, bottom: 7 });
        assert_eq!(visible.expand(insets), window);
    }
}
