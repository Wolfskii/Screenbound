# ScreenBound

Make fullscreen mean fullscreen *inside the part of the screen you choose*.

ScreenBound watches for applications entering fullscreen and constrains their real native window
to a configurable zone of the monitor — e.g. a video in the left 75% of a super-ultrawide while
Discord lives in the remaining 25%. Windows first; built with Tauri 2, Svelte 5 and Rust (Win32).

## Getting started

Prerequisites: Rust (stable, MSVC), Node 20+, WebView2 (preinstalled on Windows 11).

```powershell
npm install
npm run tauri dev
```

Defaults: zone "75% Left", rule "Browsers" (chrome.exe, msedge.exe, firefox.exe).

## Docs

- [AGENTS.md](AGENTS.md) — architecture and engineering rules
- [PLAN.md](PLAN.md) — status, decisions, limitations
- [docs/fullscreen-research.md](docs/fullscreen-research.md) — observed native behavior
- [docs/manual-testing.md](docs/manual-testing.md) — test procedures
