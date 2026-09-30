//! Keeps the OS login-item / Run-key entry aligned with `AppConfig::start_at_boot`.

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

pub fn sync(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let mgr = app.autolaunch();
    let current = mgr.is_enabled().map_err(|e| e.to_string())?;
    if enabled == current {
        return Ok(());
    }
    if enabled {
        mgr.enable().map_err(|e| e.to_string())?;
    } else {
        mgr.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}
