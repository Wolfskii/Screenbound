use std::collections::HashMap;

use windows::core::BOOL;
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

use super::{from_wide, native_error, to_rect};
use crate::core::monitor::{MonitorId, MonitorInfo};
use crate::platform::PlatformResult;

struct DisplayNames {
    friendly: String,
    device_path: String,
}

pub(super) fn enumerate() -> PlatformResult<Vec<MonitorInfo>> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let handles = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
        handles.push(h);
        BOOL(1)
    }
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut handles as *mut _ as isize),
        )
    };
    if !ok.as_bool() {
        return Err(native_error("EnumDisplayMonitors"));
    }

    let names = display_names().unwrap_or_default();
    let mut monitors: Vec<MonitorInfo> = handles
        .into_iter()
        .filter_map(|h| describe(h, &names))
        .collect();
    monitors.sort_by_key(|m| (m.number, m.bounds.left, m.bounds.top));
    Ok(monitors)
}

/// GDI device name of the monitor (`\\.\DISPLAY1`), cheap enough to call per window snapshot.
pub(super) fn device_name(h: HMONITOR) -> Option<String> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    unsafe { GetMonitorInfoW(h, &mut info as *mut _ as *mut MONITORINFO) }
        .as_bool()
        .then(|| from_wide(&info.szDevice))
}

fn describe(h: HMONITOR, names: &HashMap<String, DisplayNames>) -> Option<MonitorInfo> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !unsafe { GetMonitorInfoW(h, &mut info as *mut _ as *mut MONITORINFO) }.as_bool() {
        tracing::warn!("GetMonitorInfoW failed for monitor {:?}", h.0);
        return None;
    }
    let device_name = from_wide(&info.szDevice);
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    if unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_err() {
        dpi_x = 96;
    }
    let named = names.get(&device_name);
    let number = device_name
        .trim_start_matches(r"\\.\DISPLAY")
        .parse()
        .unwrap_or(0);
    Some(MonitorInfo {
        id: MonitorId(
            named
                .map(|n| n.device_path.clone())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| device_name.clone()),
        ),
        friendly_name: named
            .map(|n| n.friendly.clone())
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| format!("Display {number}")),
        device_name,
        number,
        is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        bounds: to_rect(info.monitorInfo.rcMonitor),
        work_area: to_rect(info.monitorInfo.rcWork),
        dpi: dpi_x,
    })
}

/// Maps GDI device names (`\\.\DISPLAY1`) to EDID friendly names and stable device paths via
/// the DisplayConfig API.
fn display_names() -> Option<HashMap<String, DisplayNames>> {
    let mut path_count = 0u32;
    let mut mode_count = 0u32;
    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO>;
    let mut modes: Vec<DISPLAYCONFIG_MODE_INFO>;
    // Retry: the topology can change between the size query and the actual query.
    let mut attempts = 0;
    loop {
        attempts += 1;
        if unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        } != ERROR_SUCCESS
        {
            return None;
        }
        paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        let rc = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if rc == ERROR_SUCCESS {
            paths.truncate(path_count as usize);
            break;
        }
        if attempts >= 3 {
            return None;
        }
    }

    let mut out = HashMap::new();
    for path in &paths {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        source.header.adapterId = path.sourceInfo.adapterId;
        source.header.id = path.sourceInfo.id;
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
            continue;
        }

        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        target.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        target.header.adapterId = path.targetInfo.adapterId;
        target.header.id = path.targetInfo.id;
        if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != 0 {
            continue;
        }

        out.entry(from_wide(&source.viewGdiDeviceName))
            .or_insert(DisplayNames {
                friendly: from_wide(&target.monitorFriendlyDeviceName),
                device_path: from_wide(&target.monitorDevicePath),
            });
    }
    Some(out)
}
