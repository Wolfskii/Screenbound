//! Animated moves between the app's own fullscreen rect, the zone, and the pre-fullscreen window.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::geometry::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransitionSpeed {
    /// Move in one step.
    Instant,
    Fast,
    #[default]
    Normal,
    Slow,
}

impl TransitionSpeed {
    pub const fn duration(self) -> Duration {
        match self {
            Self::Instant => Duration::ZERO,
            Self::Fast => Duration::from_millis(1000),
            Self::Normal => Duration::from_millis(3000),
            Self::Slow => Duration::from_millis(5000),
        }
    }
}

/// Pause before an animated move starts, so the app's own transition has settled first.
/// Not counted in `TransitionSpeed::duration`.
pub const START_DELAY: Duration = Duration::from_millis(500);

/// Quadratic ease-in: starts slowly and speeds up into the target.
pub fn ease_in(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

/// Rect `progress` of the way from `from` to `to` (0.0 = `from`, 1.0 = `to`), each edge rounded.
pub fn interpolate(from: Rect, to: Rect, progress: f64) -> Rect {
    let p = progress.clamp(0.0, 1.0);
    let lerp = |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * p).round() as i32;
    Rect::new(
        lerp(from.left, to.left),
        lerp(from.top, to.top),
        lerp(from.right, to.right),
        lerp(from.bottom, to.bottom),
    )
}

/// The rect to show `elapsed` into a transition of `duration`, and whether it is the last step.
pub fn frame_at(from: Rect, to: Rect, elapsed: Duration, duration: Duration) -> (Rect, bool) {
    if duration.is_zero() || elapsed >= duration {
        return (to, true);
    }
    let t = elapsed.as_secs_f64() / duration.as_secs_f64();
    (interpolate(from, to, ease_in(t)), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: Rect = Rect::new(0, 0, 5120, 1440);
    const ZONE: Rect = Rect::new(0, 0, 3840, 1440);

    #[test]
    fn easing_is_bounded_and_monotonic() {
        assert_eq!(ease_in(0.0), 0.0);
        assert_eq!(ease_in(1.0), 1.0);
        assert_eq!(ease_in(-1.0), 0.0);
        assert_eq!(ease_in(2.0), 1.0);
        assert!((ease_in(0.5) - 0.25).abs() < 1e-9);
        let mut last = 0.0;
        for i in 1..=100 {
            let v = ease_in(f64::from(i) / 100.0);
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn interpolates_each_edge() {
        assert_eq!(interpolate(FULL, ZONE, 0.0), FULL);
        assert_eq!(interpolate(FULL, ZONE, 1.0), ZONE);
        assert_eq!(interpolate(FULL, ZONE, 0.5), Rect::new(0, 0, 4480, 1440));
        let moved = interpolate(
            Rect::new(-1920, 0, 0, 1080),
            Rect::new(100, 100, 900, 700),
            0.5,
        );
        assert_eq!(moved, Rect::new(-910, 50, 450, 890));
    }

    #[test]
    fn frames_end_on_target() {
        let d = TransitionSpeed::Normal.duration();
        assert_eq!(frame_at(FULL, ZONE, Duration::ZERO, d), (FULL, false));
        let (mid, done) = frame_at(FULL, ZONE, d / 2, d);
        assert!(!done);
        // A quarter of the way at half time: slow start.
        assert_eq!(mid.right, 4800);
        assert_eq!(frame_at(FULL, ZONE, d, d), (ZONE, true));
        assert_eq!(
            frame_at(
                FULL,
                ZONE,
                Duration::ZERO,
                TransitionSpeed::Instant.duration()
            ),
            (ZONE, true)
        );
    }
}
