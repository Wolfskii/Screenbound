mod autostart;
mod commands;
pub mod core;
mod engine;
mod platform;

use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

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
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    platform::init_process();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Two instances would fight over the same windows; show the one already running.
            show_main(app);
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
            let start_at_boot = handle.call(|e| e.config().start_at_boot).map_err(|e| e.to_string())?;
            if let Err(e) = autostart::sync(app.handle(), start_at_boot) {
                tracing::warn!("autostart sync on startup failed: {e}");
            }

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

            // Closing the window hides it. The process keeps managing windows until Exit.
            match install_tray(app.handle()) {
                Ok(icon) => {
                    app.manage(icon);
                    if let Some(window) = app.get_webview_window("main") {
                        let hidden = window.clone();
                        window.on_window_event(move |event| {
                            if let WindowEvent::CloseRequested { api, .. } = event {
                                api.prevent_close();
                                let _ = hidden.hide();
                            }
                        });
                    }
                }
                Err(e) => tracing::error!("tray icon failed; closing the window will quit: {e}"),
            }
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

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn install_tray(app: &AppHandle) -> tauri::Result<tauri::tray::TrayIcon> {
    let show = MenuItemBuilder::with_id("show", "Show ScreenBound").build(app)?;
    let exit = MenuItemBuilder::with_id("exit", "Exit").build(app)?;
    let menu = Menu::with_items(app, &[&show, &PredefinedMenuItem::separator(app)?, &exit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("ScreenBound")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "exit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)
}
