use serde::{Deserialize, Serialize};

use super::geometry::Rect;
use super::monitor::MonitorId;

/// Opaque native window handle. On Windows this is the HWND value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowId(pub u64);

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:08X}", self.0)
    }
}

/// Owning process identity. `start_time` guards against PID reuse when re-validating
/// a window after a restart (e.g. crash recovery).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRef {
    pub pid: u32,
    pub start_time: u64,
}

/// What a window "is" — the inputs used for rule matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowIdentity {
    pub process: ProcessRef,
    /// Executable file name, e.g. `chrome.exe`. Empty if the process could not be queried
    /// (e.g. elevated process while ScreenBound runs unelevated).
    pub process_name: String,
    pub executable_path: String,
    pub class_name: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShowState {
    Normal,
    Minimized,
    Maximized,
}

/// Platform style bits, opaque to the core. On Windows: `GWL_STYLE` / `GWL_EXSTYLE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeStyle {
    pub primary: u64,
    pub extended: u64,
}

/// A point-in-time snapshot of everything ScreenBound may change and later needs to restore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    /// Full window rect, including invisible resize borders.
    pub bounds: Rect,
    /// Visible frame rect (DWM extended frame bounds on Windows). Equals `bounds` for
    /// borderless windows.
    pub visible_bounds: Rect,
    /// Restored ("normal") position as the OS stores it; used to restore maximized windows.
    /// Platform-defined coordinate space — only round-trip it back to the platform.
    pub restore_bounds: Rect,
    pub show_state: ShowState,
    pub visible: bool,
    /// Hidden by the compositor (e.g. suspended UWP windows, other virtual desktops).
    pub cloaked: bool,
    pub has_title_bar: bool,
    pub has_resize_frame: bool,
    pub topmost: bool,
    pub style: NativeStyle,
    pub monitor: Option<MonitorId>,
    pub dpi: u32,
}

impl WindowState {
    pub fn is_borderless(&self) -> bool {
        !self.has_title_bar && !self.has_resize_frame
    }
}

/// How the window's native chrome (title bar + frame) is handled inside a zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChromeMode {
    /// Leave native styles untouched. Browsers already drop their chrome in fullscreen.
    #[default]
    Keep,
    /// Strip title bar and resize frame via native styles (borderless).
    Hide,
}

/// Diagnostics row for one top-level window.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: WindowId,
    pub id_hex: String,
    pub identity: WindowIdentity,
    pub state: WindowState,
    pub monitor_name: Option<String>,
    pub fullscreen_like: bool,
    pub hung: bool,
    pub managed: bool,
    pub matched_rule: Option<String>,
}
