//! Event-driven change notification: WinEvent hooks for window changes plus a hidden top-level
//! window for display broadcasts. Runs on a dedicated thread with its own message loop; no polling.

use std::cell::RefCell;
use std::sync::mpsc;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor, GetMessageW,
    PeekMessageW, PostThreadMessageW, RegisterClassW, TranslateMessage, CHILDID_SELF,
    EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GA_ROOT, MSG, OBJID_WINDOW, PM_NOREMOVE,
    SPI_SETWORKAREA, WINDOW_EX_STYLE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_QUIT, WM_SETTINGCHANGE, WM_USER, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

use super::window::window_id;
use crate::platform::{EventSink, EventSubscription, PlatformError, PlatformEvent, PlatformResult};

thread_local! {
    // Out-of-context WinEvent callbacks and the wndproc both run on the hook thread.
    static SINK: RefCell<Option<EventSink>> = const { RefCell::new(None) };
}

fn emit(event: PlatformEvent) {
    SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink(event);
        }
    });
}

pub(super) fn subscribe(sink: EventSink) -> PlatformResult<EventSubscription> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let thread = std::thread::Builder::new()
        .name("screenbound-winevents".into())
        .spawn(move || run(sink, ready_tx))
        .map_err(|e| PlatformError::Native {
            op: "spawn event thread",
            code: 0,
            message: e.to_string(),
        })?;

    let thread_id = ready_rx
        .recv()
        .map_err(|_| PlatformError::Native {
            op: "event thread startup",
            code: 0,
            message: "thread exited".into(),
        })?
        .map_err(|message| PlatformError::Native {
            op: "event thread startup",
            code: 0,
            message,
        })?;

    Ok(EventSubscription::new(move || {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        let _ = thread.join();
    }))
}

fn run(sink: EventSink, ready: mpsc::Sender<Result<u32, String>>) {
    super::set_thread_dpi_aware();
    SINK.with(|s| *s.borrow_mut() = Some(sink));

    // Force creation of the thread message queue before anyone posts WM_QUIT to it.
    let mut msg = MSG::default();
    unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
    }

    let ranges = [
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
        (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
        (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
        (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
    ];
    let mut hooks: Vec<HWINEVENTHOOK> = Vec::with_capacity(ranges.len());
    for (min, max) in ranges {
        let hook = unsafe {
            SetWinEventHook(
                min,
                max,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if hook.is_invalid() {
            for h in hooks {
                unsafe {
                    let _ = UnhookWinEvent(h);
                }
            }
            let _ = ready.send(Err(format!("SetWinEventHook({min:#x}..{max:#x}) failed")));
            return;
        }
        hooks.push(hook);
    }

    let display_window = create_display_window();
    if display_window.is_none() {
        tracing::warn!("could not create display-change window; monitor changes will not be detected automatically");
    }

    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    tracing::debug!(hooks = hooks.len(), "native event thread running");

    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    for h in hooks {
        unsafe {
            let _ = UnhookWinEvent(h);
        }
    }
    if let Some(w) = display_window {
        unsafe {
            let _ = DestroyWindow(w);
        }
    }
    SINK.with(|s| s.borrow_mut().take());
    tracing::debug!("native event thread stopped");
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // Ignore caret/cursor/child-object notifications; only whole windows matter.
    if hwnd.is_invalid() || id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 {
        return;
    }
    let id = window_id(hwnd);
    let event = match event {
        EVENT_OBJECT_DESTROY => PlatformEvent::WindowDestroyed(id),
        EVENT_SYSTEM_MOVESIZESTART => PlatformEvent::MoveSizeStart(id),
        EVENT_SYSTEM_MOVESIZEEND => PlatformEvent::MoveSizeEnd(id),
        EVENT_OBJECT_SHOW
        | EVENT_OBJECT_HIDE
        | EVENT_OBJECT_LOCATIONCHANGE
        | EVENT_SYSTEM_FOREGROUND
        | EVENT_SYSTEM_MINIMIZESTART
        | EVENT_SYSTEM_MINIMIZEEND => {
            if unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
                return;
            }
            PlatformEvent::WindowChanged(id)
        }
        _ => return,
    };
    emit(event);
}

/// Hidden top-level window: message-only windows do not receive broadcasts like WM_DISPLAYCHANGE.
fn create_display_window() -> Option<HWND> {
    let instance = unsafe { GetModuleHandleW(None) }.ok()?;
    let class_name = w!("ScreenBoundDisplayWatcher");
    let class = WNDCLASSW {
        lpfnWndProc: Some(display_wndproc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0),
            class_name,
            w!("ScreenBound display watcher"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
    .ok()
}

unsafe extern "system" fn display_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DISPLAYCHANGE | WM_DPICHANGED => emit(PlatformEvent::DisplayChanged),
        WM_SETTINGCHANGE if wparam.0 as u32 == SPI_SETWORKAREA.0 => {
            emit(PlatformEvent::DisplayChanged)
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
