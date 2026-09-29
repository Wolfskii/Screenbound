use std::ffi::c_void;

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, SetLastError, FILETIME, HWND, LPARAM, RECT, WIN32_ERROR};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONULL};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsHungAppWindow, IsIconic,
    IsWindow, IsWindowVisible, IsZoomed, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos,
    ShowWindow, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, SET_WINDOW_POS_FLAGS, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER,
    SW_MAXIMIZE,
    SW_SHOWNOACTIVATE, WINDOWPLACEMENT, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CAPTION,
    WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE, WS_EX_TOPMOST, WS_EX_WINDOWEDGE,
    WS_MAXIMIZE, WS_MINIMIZE, WS_THICKFRAME, WS_VISIBLE,
};

use super::{monitors, to_rect, to_win_rect, window_error};
use crate::core::geometry::Rect;
use crate::core::monitor::MonitorId;
use crate::core::window::{NativeStyle, ProcessRef, ShowState, WindowId, WindowIdentity, WindowState};
use crate::platform::{PlatformError, PlatformResult};

const BORDERLESS_STRIP: WINDOW_STYLE = WINDOW_STYLE(WS_CAPTION.0 | WS_THICKFRAME.0);
const BORDERLESS_STRIP_EX: WINDOW_EX_STYLE =
    WINDOW_EX_STYLE(WS_EX_DLGMODALFRAME.0 | WS_EX_WINDOWEDGE.0 | WS_EX_CLIENTEDGE.0 | WS_EX_STATICEDGE.0);
/// State bits owned by the window manager; never overwrite them from a stale snapshot.
const LIVE_STATE_BITS: u32 = WS_VISIBLE.0 | WS_MINIMIZE.0 | WS_MAXIMIZE.0;

const BASE_POS_FLAGS: SET_WINDOW_POS_FLAGS =
    SET_WINDOW_POS_FLAGS(SWP_NOZORDER.0 | SWP_NOACTIVATE.0 | SWP_NOOWNERZORDER.0);

pub(super) fn hwnd(id: WindowId) -> HWND {
    HWND(id.0 as usize as *mut c_void)
}

pub(super) fn window_id(h: HWND) -> WindowId {
    WindowId(h.0 as usize as u64)
}

pub(super) fn exists(id: WindowId) -> bool {
    unsafe { IsWindow(Some(hwnd(id))) }.as_bool()
}

pub(super) fn is_hung(id: WindowId) -> bool {
    unsafe { IsHungAppWindow(hwnd(id)) }.as_bool()
}

fn ensure_exists(id: WindowId) -> PlatformResult<HWND> {
    if exists(id) {
        Ok(hwnd(id))
    } else {
        Err(PlatformError::WindowGone(id))
    }
}

fn ensure_responsive(id: WindowId) -> PlatformResult<HWND> {
    let h = ensure_exists(id)?;
    // Cross-process SetWindowPos blocks until the target thread pumps messages.
    if is_hung(id) {
        return Err(PlatformError::Hung(id));
    }
    Ok(h)
}

fn is_cloaked(h: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
    }
    .is_ok()
        && cloaked != 0
}

fn owner_pid(h: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

pub(super) fn enumerate(own_pid: u32) -> PlatformResult<Vec<WindowId>> {
    let mut all: Vec<HWND> = Vec::new();
    unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
        let all = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
        all.push(h);
        BOOL(1)
    }
    unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize)) }
        .map_err(|e| PlatformError::Native { op: "EnumWindows", code: e.code().0, message: e.message() })?;

    Ok(all
        .into_iter()
        .filter(|&h| unsafe { IsWindowVisible(h) }.as_bool())
        .filter(|&h| unsafe { GetWindow(h, GW_OWNER) }.map_or(true, |o| o.is_invalid()))
        .filter(|&h| !is_cloaked(h))
        .filter(|&h| owner_pid(h) != own_pid)
        .filter(|&h| !is_shell_window(h))
        .map(window_id)
        .collect())
}

/// Desktop/taskbar windows cover monitors borderlessly and would read as "fullscreen".
fn is_shell_window(h: HWND) -> bool {
    const SHELL_CLASSES: [&str; 4] = ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(h, &mut class) }.max(0) as usize;
    let class = String::from_utf16_lossy(&class[..len]);
    SHELL_CLASSES.contains(&class.as_str())
}

pub(super) fn identity(id: WindowId) -> PlatformResult<WindowIdentity> {
    let h = ensure_exists(id)?;
    let pid = owner_pid(h);
    let (executable_path, start_time) = process_info(pid);
    let process_name = executable_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_string();

    let mut class = [0u16; 256];
    let class_len = unsafe { GetClassNameW(h, &mut class) }.max(0) as usize;

    // GetWindowText on another process's window reads the cached caption without sending
    // WM_GETTEXT, so it cannot hang on an unresponsive app.
    let title_len = unsafe { GetWindowTextLengthW(h) }.clamp(0, 1024) as usize;
    let mut title = vec![0u16; title_len + 1];
    let copied = unsafe { GetWindowTextW(h, &mut title) }.max(0) as usize;

    Ok(WindowIdentity {
        process: ProcessRef { pid, start_time },
        process_name,
        executable_path,
        class_name: String::from_utf16_lossy(&class[..class_len]),
        title: String::from_utf16_lossy(&title[..copied.min(title_len)]),
    })
}

/// Returns (executable path, creation time as FILETIME ticks). Empty/0 when access is denied,
/// e.g. elevated processes while ScreenBound is unelevated.
fn process_info(pid: u32) -> (String, u64) {
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return (String::new(), 0);
    };
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let path = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }
        .map(|_| String::from_utf16_lossy(&buf[..len as usize]))
        .unwrap_or_default();

    let (mut created, mut exited, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    let start_time = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }
        .map(|_| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
        .unwrap_or(0);
    unsafe {
        let _ = CloseHandle(handle);
    }
    (path, start_time)
}

fn styles(h: HWND) -> (u32, u32) {
    unsafe {
        (
            GetWindowLongPtrW(h, GWL_STYLE) as u32,
            GetWindowLongPtrW(h, GWL_EXSTYLE) as u32,
        )
    }
}

pub(super) fn state(id: WindowId, monitor_id: impl Fn(String) -> MonitorId) -> PlatformResult<WindowState> {
    let h = ensure_exists(id)?;

    let mut rect = RECT::default();
    unsafe { GetWindowRect(h, &mut rect) }.map_err(|e| window_error(id, "GetWindowRect", e))?;
    let bounds = to_rect(rect);

    let mut frame = RECT::default();
    let visible_bounds = unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .map(|_| to_rect(frame))
    .unwrap_or(bounds);

    let mut placement = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
    let restore_bounds = unsafe { GetWindowPlacement(h, &mut placement) }
        .map(|_| to_rect(placement.rcNormalPosition))
        .unwrap_or(bounds);

    let show_state = if unsafe { IsIconic(h) }.as_bool() {
        ShowState::Minimized
    } else if unsafe { IsZoomed(h) }.as_bool() {
        ShowState::Maximized
    } else {
        ShowState::Normal
    };

    let (style, ex_style) = styles(h);
    let monitor = unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONULL) };
    let monitor = (!monitor.is_invalid())
        .then(|| monitors::device_name(monitor))
        .flatten()
        .map(monitor_id);

    Ok(WindowState {
        bounds,
        visible_bounds,
        restore_bounds,
        show_state,
        visible: unsafe { IsWindowVisible(h) }.as_bool(),
        cloaked: is_cloaked(h),
        has_title_bar: style & WS_CAPTION.0 == WS_CAPTION.0,
        has_resize_frame: style & WS_THICKFRAME.0 != 0,
        topmost: ex_style & WS_EX_TOPMOST.0 != 0,
        style: NativeStyle { primary: u64::from(style), extended: u64::from(ex_style) },
        monitor,
        dpi: unsafe { GetDpiForWindow(h) },
    })
}

fn set_window_pos(id: WindowId, h: HWND, r: Rect, flags: SET_WINDOW_POS_FLAGS) -> PlatformResult<()> {
    unsafe { SetWindowPos(h, None, r.left, r.top, r.width(), r.height(), BASE_POS_FLAGS | flags) }
        .map_err(|e| window_error(id, "SetWindowPos", e))
}

/// Leaves minimized/maximized state without activating, so explicit bounds apply cleanly.
fn ensure_normal_show_state(h: HWND) {
    if unsafe { IsIconic(h).as_bool() || IsZoomed(h).as_bool() } {
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
    }
}

pub(super) fn set_bounds(id: WindowId, bounds: Rect) -> PlatformResult<()> {
    let h = ensure_responsive(id)?;
    ensure_normal_show_state(h);
    // Skipping WM_WINDOWPOSCHANGING stops apps (e.g. Chromium in fullscreen) from rewriting
    // the rect back to the monitor; they still get WM_WINDOWPOSCHANGED/WM_SIZE to relayout.
    set_window_pos(id, h, bounds, SWP_FRAMECHANGED | SWP_NOSENDCHANGING)
}

pub(super) fn set_borderless(id: WindowId) -> PlatformResult<()> {
    let h = ensure_responsive(id)?;
    let (style, ex_style) = styles(h);
    let new_style = style & !BORDERLESS_STRIP.0;
    let new_ex = ex_style & !BORDERLESS_STRIP_EX.0;
    if new_style == style && new_ex == ex_style {
        return Ok(());
    }
    write_styles(id, h, new_style, new_ex)
}

pub(super) fn set_style(id: WindowId, style: NativeStyle) -> PlatformResult<()> {
    let h = ensure_responsive(id)?;
    let (live, live_ex) = styles(h);
    let target = (style.primary as u32 & !LIVE_STATE_BITS) | (live & LIVE_STATE_BITS);
    let target_ex = style.extended as u32;
    if target == live && target_ex == live_ex {
        return Ok(());
    }
    write_styles(id, h, target, target_ex)
}

fn write_styles(id: WindowId, h: HWND, style: u32, ex_style: u32) -> PlatformResult<()> {
    for (index, value, op) in [
        (GWL_STYLE, style, "SetWindowLongPtrW(GWL_STYLE)"),
        (GWL_EXSTYLE, ex_style, "SetWindowLongPtrW(GWL_EXSTYLE)"),
    ] {
        unsafe { SetLastError(WIN32_ERROR(0)) };
        let previous = unsafe { SetWindowLongPtrW(h, index, value as i32 as isize) };
        if previous == 0 {
            let err = windows::core::Error::from_thread();
            if err.code().is_err() {
                return Err(window_error(id, op, err));
            }
        }
    }
    // Style changes only take visual effect after SWP_FRAMECHANGED.
    unsafe {
        SetWindowPos(
            h,
            None,
            0,
            0,
            0,
            0,
            BASE_POS_FLAGS | SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
        )
    }
    .map_err(|e| window_error(id, "SetWindowPos(FRAMECHANGED)", e))
}

pub(super) fn restore_state(id: WindowId, state: &WindowState) -> PlatformResult<()> {
    let h = ensure_responsive(id)?;
    set_style(id, state.style)?;
    match state.show_state {
        ShowState::Maximized => {
            let placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                showCmd: SW_MAXIMIZE.0 as u32,
                rcNormalPosition: to_win_rect(state.restore_bounds),
                ..Default::default()
            };
            unsafe { SetWindowPlacement(h, &placement) }.map_err(|e| window_error(id, "SetWindowPlacement", e))
        }
        ShowState::Normal | ShowState::Minimized => {
            ensure_normal_show_state(h);
            set_window_pos(id, h, state.bounds, SWP_FRAMECHANGED)
        }
    }
}
