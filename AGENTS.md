# AGENTS.md — engineering rules for ScreenBound

Read this and [PLAN.md](PLAN.md) before changing code. PLAN.md is the source of truth for status.

## What ScreenBound is

A native window-behavior manager. MVP: when a window enters fullscreen, constrain the *real native
window* to a user-defined zone of the monitor. Not browser PiP, not an overlay, not fake fullscreen.

## Architecture (keep these boundaries)

```
Svelte UI (src/)              config, visualization, diagnostics — no window logic
  │ invoke / events (src/lib/api.ts)
Tauri commands (src-tauri/src/commands.rs)   thin forwarding only
  │ EngineHandle::call (channel → single engine thread)
Engine (src-tauri/src/engine/)               state machine: detect, apply, restore, journal
  │ PlatformWindowManager trait
Core (src-tauri/src/core/)                   pure logic: geometry, zones, rules, heuristics, config
Platform (src-tauri/src/platform/windows/)   Win32 only: monitors, window state/styles, WinEvent hooks
```

- `core/` must never call native APIs. Anything decidable from data belongs here, with unit tests.
- Platform-specific code lives only under `platform/<os>/`. No `cfg(windows)` in core or engine.
- The engine is single-threaded and owns its state; talk to it through `EngineHandle`.
- No app-specific branches (`if chrome`). Apps are targeted by `WindowRule` matchers in config.
  A native workaround is acceptable only when generic (e.g. `SWP_NOSENDCHANGING`) and documented.

## Native rules

- Event-driven only (WinEvent hooks + display broadcasts). No periodic polling of windows.
- Physical pixels everywhere (process is Per-Monitor-V2 DPI aware). Never assume monitor origin,
  ordering, sign of coordinates, equal DPI, or that fullscreen means the primary monitor.
- Capture state before modifying a window; journal it to disk; restore on release/exit/next start.
- External windows vanish at any time: return `PlatformError`, never panic.
- Never modify a window you cannot re-validate (pid + process start time).

## Done means tested on real Windows

A feature is not "done" because it compiles or has unit tests. Native behavior must be exercised
against real applications (see [docs/manual-testing.md](docs/manual-testing.md)) and findings
recorded in PLAN.md / [docs/fullscreen-research.md](docs/fullscreen-research.md).

## Commands

[Task](https://taskfile.dev) (`Taskfile.yml`). `task --list` prints these.

| Task | Command |
| --- | --- |
| Install npm dependencies | `task install` |
| Run app (dev) | `task dev` |
| Verbose logs | `task dev:logs` |
| Rust unit tests | `task test` |
| Desktop probe (read-only, real monitors/windows) | `task probe` |
| Resize one window and print bounds | `task probe:resize PID=<pid>` |
| Lint | `task lint` |
| Frontend type check | `task check` |
| Typecheck, test, and lint | `task ci` |
| Release build | `task release` |
| Install that release for the current user | `task deploy` |

`scripts/probe-resize.ps1 -ProcessId <pid> [-NoSendChanging]` resizes one process's main window and
prints how its bounds evolve — useful for checking whether an app vetoes external resizes.

## Conventions

- Rust: `cargo clippy` clean, serde `camelCase` for anything crossing to the UI.
- TS types in `src/lib/types.ts` mirror the Rust serde types — update both together.
- Config lives in `%APPDATA%\com.screenbound.app\config.json` (zones, rules, seen apps); journal in `managed-windows.json`. The NSIS uninstaller leaves that folder in place unless **Delete app data** is checked.
- Keep comments short; explain *why*, not *what*.
