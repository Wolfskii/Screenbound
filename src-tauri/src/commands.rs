//! Tauri command surface. Thin: validates/forwards to the engine thread and returns JSON.

use tauri::State;

use crate::core::config::AppConfig;
use crate::core::monitor::MonitorInfo;
use crate::core::window::{WindowId, WindowInfo};
use crate::engine::activity::ActivityEntry;
use crate::engine::{EngineHandle, EngineStatus};

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub async fn get_monitors(engine: State<'_, EngineHandle>) -> CmdResult<Vec<MonitorInfo>> {
    engine.call(|e| e.monitors())
}

#[tauri::command]
pub async fn get_config(engine: State<'_, EngineHandle>) -> CmdResult<AppConfig> {
    engine.call(|e| e.config())
}

#[tauri::command]
pub async fn set_config(engine: State<'_, EngineHandle>, config: AppConfig) -> CmdResult<AppConfig> {
    engine.call(move |e| e.set_config(config))?
}

#[tauri::command]
pub async fn set_enabled(engine: State<'_, EngineHandle>, enabled: bool) -> CmdResult<AppConfig> {
    engine.call(move |e| {
        let mut config = e.config();
        config.enabled = enabled;
        e.set_config(config)
    })?
}

#[tauri::command]
pub async fn get_status(engine: State<'_, EngineHandle>) -> CmdResult<EngineStatus> {
    engine.call(|e| e.status())
}

#[tauri::command]
pub async fn list_windows(engine: State<'_, EngineHandle>) -> CmdResult<Vec<WindowInfo>> {
    engine.call(|e| e.list_windows())?
}

#[tauri::command]
pub async fn get_activity(engine: State<'_, EngineHandle>) -> CmdResult<Vec<ActivityEntry>> {
    engine.call(|e| e.activity())
}

#[tauri::command]
pub async fn release_window(engine: State<'_, EngineHandle>, id: u64) -> CmdResult<()> {
    engine.call(move |e| e.release_window(WindowId(id)))
}

#[tauri::command]
pub async fn release_all(engine: State<'_, EngineHandle>) -> CmdResult<()> {
    engine.call(|e| e.release_all())
}
