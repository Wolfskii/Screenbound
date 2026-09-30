# Manual test procedures

Run with `task dev` and keep the **Diagnostics** tab open. For isolated browser tests use a
throwaway profile so your real session is untouched, e.g.
`msedge.exe --user-data-dir=%TEMP%\sb-test --no-first-run https://www.youtube.com`.

For each case record result + date in PLAN.md / docs/fullscreen-research.md.

## Core flow (repeat for Chrome, Edge, Firefox)

1. Zone "75% Left", rule "Browsers" enabled, chrome mode Keep.
2. Play a video, enter fullscreen (video button, then separately F11).
3. Expect: activity log "Constrained to zone…", window occupies left 75%, remaining 25% usable.
4. Click a window in the remaining 25%, then click back on the video. Expect the zone to hold (no flicker loop, no "app refused").
5. Exit fullscreen (Esc). Expect "Fullscreen exited; released", browser back at its previous size/position/maximized state.
6. Repeat with chrome mode Hide.

## Companion panels (VLC)

1. VLC enabled on the Apps tab, fullscreen a video.
2. Move the mouse so the fullscreen controls appear. Expect them inside the zone, at the same
   relative spot they use on the full monitor (bottom-centre-ish), not on the rest of the screen.
3. Drag the controls out of the zone; expect them to stay there, also after they hide and
   reappear. Leave fullscreen and enter it again; expect them back inside the zone.
4. In a browser, open a menu or bubble near the zone edge while fullscreen; expect it untouched.
5. With a second browser window on the same monitor, expect that window untouched.

## Lifecycle

- Close the browser while constrained → "Window closed while managed", journal cleared.
- Close ScreenBound while constrained → browser returns to full-monitor fullscreen.
- Kill ScreenBound (Task Manager) while constrained, restart → "Restored window left constrained by a previous session", then re-constrained.
- Change the zone while constrained → window moves immediately.
- Disable ScreenBound (header toggle) while constrained → window released.
- "Release" button in Diagnostics → window released and not re-captured until it re-enters fullscreen.

## Monitors

- Two monitors, fullscreen on each; zone unpinned → applies on the fullscreen monitor.
- Secondary monitor left of / above primary (negative coordinates).
- Mixed DPI (100% + 150%).
- Zone pinned to monitor B while fullscreen happens on A.
- Unplug / change resolution while constrained → re-applied on the new geometry.
- Taskbar auto-hide on/off; zone "Relative to: Work area".

## Idle cost

- With no fullscreen windows, ScreenBound CPU in Task Manager should read ~0%.
- Drag windows around rapidly; CPU should stay low (events are coalesced per window).
