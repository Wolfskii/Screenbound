mod commands;
pub mod core;
mod engine;
mod platform;

use std::sync::Mutex;
use std::time::Duration;

use tauri::{Emitter, Manager, RunEvent};

use engine::{Engine, EngineHandle, EngineMessage, EnginePaths, Notification};
use platform::EventSubscription;

/// Keeps the native event subscription alive for the app's lifetime.
struct Subscription(Mutex<Option<EventSubscription>>);

pub mod events {
    pub const ACTIVITY: &str = "sb://activity";
    pub const MANAGED_CHANGED: &str = "sb://managed-changed";
    pub const MONITORS_CHANGED: &str = "sb://monitors-changed";
    pub const CONFIG_CHANGED: &str = "sb://config-changed";
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("screenbound_lib=info,warn"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    platform::init_process();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Two instances would fight over the same windows; focus the existing one instead.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let paths = EnginePaths {
                config: config_dir.join("config.json"),
                journal: data_dir.join("managed-windows.json"),
            };
            tracing::info!(config = %paths.config.display(), journal = %paths.journal.display(), "starting ScreenBound");

            let emitter = app.handle().clone();
            let notify: engine::Notifier = std::sync::Arc::new(move |n| {
                let _ = match n {
                    Notification::Activity(entry) => emitter.emit(events::ACTIVITY, entry),
                    Notification::ManagedChanged => emitter.emit(events::MANAGED_CHANGED, ()),
                    Notification::MonitorsChanged(m) => emitter.emit(events::MONITORS_CHANGED, m),
                    Notification::ConfigChanged(c) => emitter.emit(events::CONFIG_CHANGED, c),
                };
            });

            let platform = platform::create();
            let handle = EngineHandle::spawn(Engine::new(platform.clone(), paths, notify))?;

            let engine_tx = handle.sender();
            let subscription = match platform.subscribe(Box::new(move |ev| {
                let _ = engine_tx.send(EngineMessage::Platform(ev));
            })) {
                Ok(s) => {
                    let _ = handle.call(|e| e.set_events_active(true));
                    Some(s)
                }
                Err(e) => {
                    tracing::error!("native event subscription failed: {e}");
                    None
                }
            };

            app.manage(handle);
            app.manage(Subscription(Mutex::new(subscription)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_monitors,
            commands::get_config,
            commands::set_config,
            commands::set_enabled,
            commands::get_status,
            commands::list_windows,
            commands::get_activity,
            commands::release_window,
            commands::release_all,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ScreenBound");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            // Restore every managed window before the process goes away.
            if let Some(handle) = app.try_state::<EngineHandle>() {
                handle.stop(Duration::from_secs(3));
            }
            if let Some(sub) = app.try_state::<Subscription>() {
                sub.0.lock().unwrap_or_else(|e| e.into_inner()).take();
            }
        }
    });
}
