//! Platform boundary. Everything above this module speaks in core types; everything below is
//! native. Add macOS/Linux by implementing `PlatformWindowManager` in a sibling module.

use crate::core::geometry::Rect;
use crate::core::monitor::MonitorInfo;
use crate::core::window::{NativeStyle, WindowId, WindowIdentity, WindowState};

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("window {0} no longer exists")]
    WindowGone(WindowId),
    #[error("access denied to window {0} (it may belong to an elevated process)")]
    AccessDenied(WindowId),
    #[error("window {0} is not responding")]
    Hung(WindowId),
    #[error("{op} failed: {message} (code {code})")]
    Native {
        op: &'static str,
        code: i32,
        message: String,
    },
    #[error("not supported on this platform")]
    #[cfg_attr(windows, allow(dead_code))]
    Unsupported,
}

pub type PlatformResult<T> = Result<T, PlatformError>;

/// Coarse native notifications. They carry only the window id: consumers re-read state
/// because native events are frequently coalesced, reordered or stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformEvent {
    /// Location, size, visibility, minimize or foreground changed.
    WindowChanged(WindowId),
    WindowDestroyed(WindowId),
    /// User started/finished an interactive move/resize (title-bar drag, frame drag).
    MoveSizeStart(WindowId),
    MoveSizeEnd(WindowId),
    /// Monitor added/removed, resolution/DPI/work-area change.
    DisplayChanged,
}

pub type EventSink = Box<dyn Fn(PlatformEvent) + Send + 'static>;

/// Keeps native event delivery alive; dropping it unhooks and stops the event thread.
pub struct EventSubscription {
    stop: Option<Box<dyn FnOnce() + Send>>,
}

impl EventSubscription {
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Self {
        Self {
            stop: Some(Box::new(stop)),
        }
    }
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

pub trait PlatformWindowManager: Send + Sync {
    fn monitors(&self) -> PlatformResult<Vec<MonitorInfo>>;

    /// Top-level, visible, non-cloaked windows not owned by ScreenBound, in z-order.
    fn enumerate_windows(&self) -> PlatformResult<Vec<WindowId>>;

    fn exists(&self, window: WindowId) -> bool;

    /// Owning process id, without opening the process.
    fn process_id(&self, window: WindowId) -> Option<u32>;

    /// The window that owns `window` (dialogs, tool panels), if any.
    fn owner(&self, window: WindowId) -> Option<WindowId>;

    /// Visible, non-cloaked top-level windows of one process, including owned panels.
    fn process_windows(&self, pid: u32) -> PlatformResult<Vec<WindowId>>;

    /// Moves without resizing, activating, or changing z-order.
    fn set_position(&self, window: WindowId, left: i32, top: i32) -> PlatformResult<()>;
    fn identity(&self, window: WindowId) -> PlatformResult<WindowIdentity>;
    fn state(&self, window: WindowId) -> PlatformResult<WindowState>;
    fn is_hung(&self, window: WindowId) -> bool;

    /// Moves/resizes so the *window rect* equals `bounds`. Restores maximized/minimized first.
    /// Bypasses the app's chance to veto/adjust the rect (zone enforcement must win).
    fn set_bounds(&self, window: WindowId, bounds: Rect) -> PlatformResult<()>;

    /// Strips title bar and resize frame using native styles (never an overlay).
    fn set_borderless(&self, window: WindowId) -> PlatformResult<()>;

    /// Writes back previously captured style bits and refreshes the non-client frame.
    fn set_style(&self, window: WindowId, style: NativeStyle) -> PlatformResult<()>;

    /// Restores styles, bounds and show state from a snapshot.
    fn restore_state(&self, window: WindowId, state: &WindowState) -> PlatformResult<()>;

    /// Inserts the window at the top (or back out) of the topmost z-order band.
    fn set_topmost(&self, window: WindowId, topmost: bool) -> PlatformResult<()>;

    /// Asks the shell to treat `window` as fullscreen, which drops the taskbar below it
    /// while it is active. `hide == false` clears that mark.
    fn set_hides_taskbar(&self, window: WindowId, hide: bool) -> PlatformResult<()>;

    fn subscribe(&self, sink: EventSink) -> PlatformResult<EventSubscription>;
}

pub fn create() -> std::sync::Arc<dyn PlatformWindowManager> {
    #[cfg(windows)]
    {
        std::sync::Arc::new(windows::WindowsPlatform::new())
    }
    #[cfg(not(windows))]
    {
        std::sync::Arc::new(unsupported::UnsupportedPlatform)
    }
}

/// Prepares process-wide native settings; call before any window or thread is created.
pub fn init_process() {
    #[cfg(windows)]
    windows::init_process();
}

/// Per-thread native setup for threads that call platform APIs.
pub fn init_thread() {
    #[cfg(windows)]
    windows::set_thread_dpi_aware();
}
