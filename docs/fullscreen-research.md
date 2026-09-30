# Fullscreen research notes (Windows)

Record observed behavior here. Mark each entry with how it was verified.

## Chromium (Chrome, Edge, Electron) — source + observed

Source: `ui/views/win/fullscreen_handler.cc`, `ui/views/win/hwnd_message_handler.cc`.

- Fullscreen reuses the same top-level HWND (`Chrome_WidgetWin_1`). On enter it saves style/rect/
  maximized, strips `WS_CAPTION | WS_THICKFRAME` and edge ex-styles, and `SetWindowPos`es to the
  monitor rect. On exit it restores its saved state itself.
- **Observed (Edge, 2026-09):** fullscreen style `0x160B0000`/`0x1E0B0000`, ex `0x00200000`.
- While fullscreen, `OnWindowPosChanging` rewrites same-monitor moves/resizes back to the monitor
  rect. **Observed:** plain `SetWindowPos` to 1440×1200 returned success but bounds stayed
  1920×1200. With `SWP_NOSENDCHANGING` the size stuck (checked for 3 s).
- A first resize during the enter transition may appear to stick, then get overwritten ~400 ms later.
- Background-fullscreen hack: when another window on the same monitor activates, the fullscreen
  window shrinks to monitor height − 1 and is un-marked as fullscreen for the taskbar.
  **Observed:** 1920×1199 after ScreenBound released the window while a terminal had focus.
- `ITaskbarList2::MarkFullscreenWindow` is used to hide the taskbar.

## Firefox / LibreWolf — observed 2026-09

Same top-level `MozillaWindowClass` HWND for HTML5 fullscreen (YouTube in LibreWolf) and for F11.
F11 clears the maximized state (`IsZoomed` is false). YouTube's fullscreen button often does not:
`HideWindowChrome` strips `WS_CAPTION | WS_THICKFRAME` while `showCmd` stays `SW_SHOWMAXIMIZED`,
and the window rect still covers the monitor. Detection used to require a non-maximized window, so
that path was ignored and the video stayed monitor-sized. Maximized borderless windows that cover
a monitor are now treated as fullscreen. A captioned maximized window is still ignored.

`OnWindowPosChanging` rewrites any move+size back to the monitor rect while Firefox's size mode is
fullscreen (`bug 1482920`). `SWP_NOSENDCHANGING` skips that message for our own `SetWindowPos`.
DOM fullscreen (YouTube's button) still calls `SetWindowPos` itself several times during the
enter transition, after the first WinEvent. Applying on that first event lost the race: Firefox
put the monitor rect back, ScreenBound re-applied a few times, then gave up and left the window
monitor-sized. F11 does not repeat the resize, so it stuck. Monitor-sized changes now wait until
the rect stops moving (at most 1 s) before the zone is applied.

The taskbar stays painted over the zone until the video is clicked: the shell
only drops the taskbar below a window that no longer covers the monitor when that window is
activated, and our `SetWindowPos` does not activate. While managed, the window is raised
`HWND_TOPMOST` and re-marked with `ITaskbarList2::MarkFullscreenWindow`, then the shell is nudged
with `HSHELL_RUDEAPPACTIVATED` so it does not wait for a click. Topmost is cleared on release.

Exiting fullscreen restored a smaller window than before the click. `InfallibleMakeFullScreen`
caches `GetScreenBounds()` on every fullscreen entry, so a resize while fullscreen replaces the
rect Firefox puts back. On exit we restore the last framed or maximized window we observed.

The monitor-sized frame was visible for a moment before the zone applied: out-of-context WinEvents
arrive after the resize, and evaluation waited 60 ms to coalesce the transition. Fullscreen-sized
changes are now applied on the same turn, which shortens the flash but cannot remove it: the app
has already presented the monitor-sized frame when the event arrives.

Hiding the window across our move does not work from outside the app. **Observed (2026-09):**
`DwmSetWindowAttribute(DWMWA_CLOAK)` on another process's window returns E_ACCESSDENIED
(`probe_cross_process_cloak`). Removing both jumps (enter and exit) needs code running inside the
target process that rewrites `WM_WINDOWPOSCHANGING`, i.e. an injected hook DLL. For Firefox-family
browsers, `full-screen-api.ignore-widgets = true` avoids the OS resize altogether.

## Detection heuristic (core/fullscreen.rs)

Visible, not cloaked, normal show state, no caption, no thick frame, window rect ⊇ monitor rect.
Excludes shell windows (`Progman`, `WorkerW`, `Shell_TrayWnd`, `Shell_SecondaryTrayWnd`) — the
desktop matches the geometric heuristic otherwise (observed).

## Fullscreen kinds

| Kind | Controllable | Notes |
| --- | --- | --- |
| Borderless / window fullscreen (browsers, most players, many games) | Yes | Target of the MVP |
| Maximized with hidden taskbar | N/A | Has caption → not treated as fullscreen |
| Exclusive DXGI/D3D fullscreen | No | Display mode owned by the app; detect via `SHQueryUserNotificationState` (`QUNS_RUNNING_D3D_FULL_SCREEN`) later |
| UWP / cloaked windows | Unknown | Cloaked windows ignored |
