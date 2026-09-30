# ScreenBound plan & status

Legend: ✅ verified on real Windows · 🧪 implemented, unit-tested, awaiting real-world verification ·
⬜ not started. Only mark ✅ after testing against actual applications.

## Phases

| Phase | Scope | Status |
| --- | --- | --- |
| 0 | Tauri 2 + Svelte 5 (SvelteKit SPA) + Rust bootstrap, single-instance | ✅ app starts |
| 1 | Monitor discovery (bounds, work area, DPI, friendly name, stable device path) | ✅ 1 monitor · ⬜ multi/negative/mixed DPI |
| 2 | Zone model (normalized rects, monitor pinning, work-area reference) | ✅ unit tests |
| 3 | Window discovery + diagnostics view | ✅ enumeration/state probe · 🧪 UI |
| 4 | Fullscreen detection (Chrome / Edge / Firefox) | ✅ Edge (`--start-fullscreen`) · ⬜ Chrome · ⬜ Firefox · ⬜ HTML5 video fullscreen |
| 5 | State preservation / restoration + crash journal | ✅ restore on exit · ✅ journal recovery · ⬜ exit-fullscreen restore |
| 6 | Zone fullscreen | ✅ Edge, single monitor |
| 7 | Native chrome removal (Keep / Hide) | 🧪 |
| 8 | MVP config UI (visual zone editor, presets, rules) | 🧪 |
| 9 | Hardening & manual test matrix | ⬜ see [docs/manual-testing.md](docs/manual-testing.md) |

## MVP definition-of-done tracker

1. Starts ✅ · 2. Monitors detected ✅ (single) · 3. Geometry displayed 🧪 · 4. Custom zone 🧪 ·
2. Chrome ⬜ · 6. Edge ✅ · 7. Firefox ⬜ · 8. Native window identified ✅ · 9. State captured ✅ ·
3. Constrained to zone ✅ · 11. Borderless 🧪 · 12/13. Title-bar toggle, default off 🧪 ·
4. Exit restores ⬜ · 15. Multi-monitor ⬜ · 16. Diagnostics 🧪 · 17. Low idle CPU 🧪 (no polling
by design; measure) · 18. Generic structure ✅

## Design decisions (and deviations from the original brief)

- **Chrome mode semantics.** `Keep` = don't touch native styles (default); `Hide` = strip
  `WS_CAPTION | WS_THICKFRAME` + edge ex-styles. Browsers already drop their chrome in fullscreen,
  so `Keep` still looks borderless for them; the option matters for apps that keep a frame.
  Re-adding a caption to a Chromium fullscreen window is not offered: Chromium owns its non-client
  area (`WM_NCCALCSIZE`) and would not render a real title bar.
- **`SWP_NOSENDCHANGING` for zone enforcement.** Verified: Chromium rewrites any same-monitor
  `WM_WINDOWPOSCHANGING` to the monitor rect while fullscreen (`HWNDMessageHandler::OnWindowPosChanging`).
  Skipping that message makes the zone stick; the app still receives `WM_WINDOWPOSCHANGED`/`WM_SIZE`
  and relays out. Generic, not browser-specific.
- **Apply is verified.** After positioning, the engine re-reads bounds; if the app did not end up
  in the zone the attempt fails, the app's own state is restored, and the window is left alone
  until it leaves fullscreen.
- **Loop guard.** >4 re-asserts in 3 s → stop fighting, log, show "app refused" in diagnostics.
- **Detected vs pre-fullscreen state.** We hand back the *app's own fullscreen state* when we let go
  while the app is still fullscreen (exit, disable, crash recovery). When the app exits fullscreen
  itself we put back the last framed (or maximized) window we saw. Firefox rewrites its saved
  restore rect when the fullscreen window is resized, so trusting the app comes back smaller.
- **Crash safety.** Every change is journaled (`managed-windows.json`, atomic write). On start,
  entries whose window still exists, belongs to the same process (pid + start time) and is still at
  our applied bounds are restored. Panics inside the engine are caught per message.
- **Monitor identity.** Monitors are keyed by DisplayConfig device path (stable across reboots),
  not `\\.\DISPLAYn`.
- **Invisible borders.** Zone targets are visible rects; framed windows are expanded by their DWM
  invisible-border insets so there is no gap.

## Known limitations / open questions

- Exclusive (DXGI/D3D) fullscreen can't be constrained with window APIs. Detect & report only.
- Elevated windows can't be modified from an unelevated ScreenBound (UIPI) → logged as access denied.
- Chromium marks fullscreen windows via `ITaskbarList2::MarkFullscreenWindow`; taskbar may stay
  hidden while the constrained window is active.
- LibreWolf/Firefox HTML5 fullscreen (YouTube, 2026-09): after the zone resize the taskbar stayed
  painted over the window until the video was clicked. Constrained windows are now raised above
  the taskbar and re-marked fullscreen immediately. Exiting fullscreen also restored a smaller
  window than before the click; we now put back the last framed window ourselves. 🧪 confirm both
  on LibreWolf.
- Chromium's background-fullscreen hack shrinks the window by 1 px when another window on the same
  monitor activates, and restores monitor bounds when it re-activates. Treated as "re-fullscreen" and
  re-applied; needs real testing for flicker.
- Win+Arrow snapping a managed window is currently interpreted as "exited fullscreen".
- Pinning a zone to a *different* monitor than the fullscreen one is untested (DPI change mid-move).
- Hard kill of ScreenBound (e.g. killing `tauri dev`) skips exit restore; journal recovers on next start.

## Next steps

1. Manual matrix for Chrome, Firefox, HTML5 video fullscreen (YouTube), exit via Esc/F11.
2. Multi-monitor + negative coordinates + mixed DPI.
3. Measure idle CPU; review `EVENT_OBJECT_LOCATIONCHANGE` volume.
4. Tray icon + close-to-tray 🧪 (Exit on the tray menu quits; autostart still open).
5. Hide the one-frame real-fullscreen flash 🧪 (immediate apply + DWM cloak across the zone move).
6. Then post-MVP: generic rules UI (class/title matchers), fixed size, aspect ratio, generic
   borderless, always-on-top, profiles, macOS/Linux backends.
