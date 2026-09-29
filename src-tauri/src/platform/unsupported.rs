use super::{EventSink, EventSubscription, PlatformError, PlatformResult, PlatformWindowManager};
use crate::core::geometry::Rect;
use crate::core::monitor::MonitorInfo;
use crate::core::window::{NativeStyle, WindowId, WindowIdentity, WindowState};

pub struct UnsupportedPlatform;

impl PlatformWindowManager for UnsupportedPlatform {
    fn monitors(&self) -> PlatformResult<Vec<MonitorInfo>> {
        Err(PlatformError::Unsupported)
    }
    fn enumerate_windows(&self) -> PlatformResult<Vec<WindowId>> {
        Err(PlatformError::Unsupported)
    }
    fn exists(&self, _: WindowId) -> bool {
        false
    }
    fn identity(&self, _: WindowId) -> PlatformResult<WindowIdentity> {
        Err(PlatformError::Unsupported)
    }
    fn state(&self, _: WindowId) -> PlatformResult<WindowState> {
        Err(PlatformError::Unsupported)
    }
    fn is_hung(&self, _: WindowId) -> bool {
        false
    }
    fn set_bounds(&self, _: WindowId, _: Rect) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    fn set_borderless(&self, _: WindowId) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    fn set_style(&self, _: WindowId, _: NativeStyle) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    fn restore_state(&self, _: WindowId, _: &WindowState) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    fn subscribe(&self, _: EventSink) -> PlatformResult<EventSubscription> {
        Err(PlatformError::Unsupported)
    }
}
