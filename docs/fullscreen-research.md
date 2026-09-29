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

## Firefox — not yet verified

Expected: `MozillaWindowClass`, same-HWND fullscreen with chrome hidden. Also see pref
`full-screen-api.ignore-widgets` (content-only fullscreen inside the window) as a fallback.

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
