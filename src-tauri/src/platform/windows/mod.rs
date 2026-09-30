//! Win32 implementation of the platform boundary.

mod events;
mod monitors;
mod window;

use std::collections::HashMap;
use std::sync::Mutex;

use windows::Win32::Foundation::{GetLastError, ERROR_ACCESS_DENIED, RECT};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, SetThreadDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

use super::{EventSink, EventSubscription, PlatformError, PlatformResult, PlatformWindowManager};
use crate::core::geometry::Rect;
use crate::core::monitor::{MonitorId, MonitorInfo};
use crate::core::window::{NativeStyle, WindowId, WindowIdentity, WindowState};

pub fn init_process() {
    // Per-monitor-v2 makes every API speak physical pixels. Fails harmlessly if already set.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub(crate) fn set_thread_dpi_aware() {
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // MarkFullscreenWindow needs an STA. S_FALSE means this thread already initialized COM.
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            tracing::warn!("CoInitializeEx failed: {hr}");
        }
    }
}

pub struct WindowsPlatform {
    own_pid: u32,
    /// GDI device name -> stable MonitorId, refreshed by `monitors()`.
    monitor_ids: Mutex<HashMap<String, MonitorId>>,
}

impl WindowsPlatform {
    pub fn new() -> Self {
        Self { own_pid: std::process::id(), monitor_ids: Mutex::new(HashMap::new()) }
    }

    fn monitor_id_for(&self, device_name: String) -> MonitorId {
        let ids = self.monitor_ids.lock().unwrap_or_else(|e| e.into_inner());
        ids.get(&device_name).cloned().unwrap_or(MonitorId(device_name))
    }
}

impl PlatformWindowManager for WindowsPlatform {
    fn monitors(&self) -> PlatformResult<Vec<MonitorInfo>> {
        let monitors = monitors::enumerate()?;
        let mut ids = self.monitor_ids.lock().unwrap_or_else(|e| e.into_inner());
        ids.clear();
        ids.extend(monitors.iter().map(|m| (m.device_name.clone(), m.id.clone())));
        Ok(monitors)
    }

    fn enumerate_windows(&self) -> PlatformResult<Vec<WindowId>> {
        window::enumerate(self.own_pid)
    }

    fn exists(&self, id: WindowId) -> bool {
        window::exists(id)
    }

    fn process_id(&self, id: WindowId) -> Option<u32> {
        window::process_id(id)
    }

    fn owner(&self, id: WindowId) -> Option<WindowId> {
        window::owner(id)
    }

    fn process_windows(&self, pid: u32) -> PlatformResult<Vec<WindowId>> {
        window::process_windows(pid)
    }

    fn set_position(&self, id: WindowId, left: i32, top: i32) -> PlatformResult<()> {
        window::set_position(id, left, top)
    }

    fn identity(&self, id: WindowId) -> PlatformResult<WindowIdentity> {
        window::identity(id)
    }

    fn state(&self, id: WindowId) -> PlatformResult<WindowState> {
        window::state(id, |name| self.monitor_id_for(name))
    }

    fn is_hung(&self, id: WindowId) -> bool {
        window::is_hung(id)
    }

    fn set_bounds(&self, id: WindowId, bounds: Rect) -> PlatformResult<()> {
        window::set_bounds(id, bounds)
    }

    fn set_borderless(&self, id: WindowId) -> PlatformResult<()> {
        window::set_borderless(id)
    }

    fn set_style(&self, id: WindowId, style: NativeStyle) -> PlatformResult<()> {
        window::set_style(id, style)
    }

    fn restore_state(&self, id: WindowId, state: &WindowState) -> PlatformResult<()> {
        window::restore_state(id, state)
    }

    fn set_topmost(&self, id: WindowId, topmost: bool) -> PlatformResult<()> {
        window::set_topmost(id, topmost)
    }

    fn set_hides_taskbar(&self, id: WindowId, hide: bool) -> PlatformResult<()> {
        window::set_hides_taskbar(id, hide)
    }

    fn subscribe(&self, sink: EventSink) -> PlatformResult<EventSubscription> {
        events::subscribe(sink)
    }
}

pub(crate) fn to_rect(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

pub(crate) fn to_win_rect(r: Rect) -> RECT {
    RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

pub(crate) fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Builds an error from the calling thread's last Win32 error.
pub(crate) fn native_error(op: &'static str) -> PlatformError {
    let code = unsafe { GetLastError() };
    let err = windows::core::Error::from_hresult(code.to_hresult());
    PlatformError::Native { op, code: code.0 as i32, message: err.message() }
}

/// Maps a failed call on a specific window to the most useful error variant.
pub(crate) fn window_error(id: WindowId, op: &'static str, err: windows::core::Error) -> PlatformError {
    if !window::exists(id) {
        PlatformError::WindowGone(id)
    } else if err.code() == ERROR_ACCESS_DENIED.to_hresult() {
        PlatformError::AccessDenied(id)
    } else {
        PlatformError::Native { op, code: err.code().0, message: err.message() }
    }
}

/// Read-only probes against the real desktop. Run with `cargo test -- --ignored --nocapture`.
#[cfg(test)]
mod desktop_tests {
    use super::*;
    use crate::core::fullscreen::is_fullscreen_like;

    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn probe_monitors_and_windows() {
        init_process();
        let p = WindowsPlatform::new();
        let monitors = p.monitors().expect("monitors");
        assert!(!monitors.is_empty());
        assert_eq!(monitors.iter().filter(|m| m.is_primary).count(), 1);
        for m in &monitors {
            println!(
                "#{} {} [{}] bounds={} work={} dpi={} primary={} id={}",
                m.number, m.friendly_name, m.device_name, m.bounds, m.work_area, m.dpi, m.is_primary, m.id.0
            );
            assert!(!m.bounds.is_empty());
            assert!(m.bounds.contains_rect(&m.work_area));
        }
        let windows = p.enumerate_windows().expect("windows");
        assert!(!windows.is_empty());
        for id in windows {
            let (Ok(ident), Ok(state)) = (p.identity(id), p.state(id)) else { continue };
            println!(
                "{id} {:<20} {:<28} {:<40.40} {} vis={} max={:?} caption={} fs={} fancyzones={}",
                ident.process_name,
                ident.class_name,
                ident.title,
                state.bounds,
                state.visible_bounds,
                state.show_state,
                state.has_title_bar,
                is_fullscreen_like(&state, &monitors),
                state.zone_snapped
            );
        }
    }

    /// Checks whether DWM lets this process cloak a window owned by another process.
    /// Starts and closes its own `winver.exe`; never touches the user's windows.
    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn probe_cross_process_cloak() {
        init_process();
        let mut child = std::process::Command::new("winver.exe").spawn().expect("spawn winver");
        let mut hwnd = None;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            hwnd = window::owned_top_level(child.id());
            if hwnd.is_some() {
                break;
            }
        }
        let result = hwnd.map(|h| {
            let set = window::set_cloak(h, true);
            let after = window::cloaked(h);
            let clear = window::set_cloak(h, false);
            (set, after, clear, window::cloaked(h))
        });
        let _ = child.kill();
        let _ = child.wait();
        let (set, cloaked_after_set, clear, cloaked_after_clear) = result.expect("winver window not found");
        println!("cloak: {set:?} -> cloaked={cloaked_after_set}; uncloak: {clear:?} -> cloaked={cloaked_after_clear}");
        assert!(!cloaked_after_clear, "window left cloaked");
    }
}
