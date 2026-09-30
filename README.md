# ScreenBound

<p align="center">
  <img src="assets/logo.webp" alt="ScreenBound" width="160" />
</p>

Make fullscreen mean fullscreen *inside the part of the screen you choose*.

ScreenBound watches for applications entering fullscreen and constrains their real native window
to a configurable zone of the monitor — e.g. a video in the left 75% of a super-ultrawide while
Discord lives in the remaining 25%. Windows first; built with Tauri 2, Svelte 5 and Rust (Win32).

## Getting started

Prerequisites: Rust (stable, MSVC), Node 20+, [Task](https://taskfile.dev), WebView2 (preinstalled on Windows 11).

```powershell
task dev
```

`task dev` installs npm dependencies first when `node_modules` is missing.

`task --list` shows the rest (`task test`, `task release`, `task deploy`, …).

`task release` writes three artifacts into `dist/`:

- `ScreenBound_*_x64-setup.exe` — NSIS installer
- `ScreenBound_*_x64.msi` — MSI installer
- `ScreenBound_*_x64-portable.exe` — no-install single binary (needs WebView2)

Defaults: zone "75% Left" and a rule for every app you turn on. The **Groups** tab names groups and gives them a color and icon; its switch turns that group on or off. The **Apps** tab turns individual apps on and assigns them to a group. A rule can target all apps that are on, specific groups, or specific apps. Settings stay in `%APPDATA%\com.screenbound.app` across reinstalls unless uninstall **Delete app data** is checked.

## Docs

- [AGENTS.md](AGENTS.md) — architecture and engineering rules
- [PLAN.md](PLAN.md) — status, decisions, limitations
- [docs/fullscreen-research.md](docs/fullscreen-research.md) — observed native behavior
- [docs/manual-testing.md](docs/manual-testing.md) — test procedures
