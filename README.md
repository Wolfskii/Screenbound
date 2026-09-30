# ScreenBound

<p align="center">
  <img src="assets/logo.webp" alt="ScreenBound" width="160" />
</p>

<p align="center">
  <a href="https://www.buymeacoffee.com/wolfskii">
    <img src="https://img.buymeacoffee.com/button-api/?text=Buy%20me%20a%20coffee&emoji=&slug=wolfskii&button_colour=FFDD00&font_colour=000000&font_family=Cookie&outline_colour=000000&coffee_colour=ffffff" alt="Buy me a coffee" />
  </a>
</p>

<p align="center"><strong>Make fullscreen mean fullscreen <em>inside the part of the screen you choose</em>.</strong></p>

Fullscreen on an ultrawide is mostly wasted space. A 16:9 video stretched across a 32:9 monitor
ends up as a small picture between two huge black bars, and your chat, notes and music are hidden
behind it.

ScreenBound fixes that. When an app goes fullscreen, it stays in the area you picked. Everything
else stays visible next to it.

## Why you'll like it

- **No more black bars.** Fit fullscreen video to a 16:9 area and use the rest of the monitor for
  other things.
- **Keep your side apps visible.** Discord, Spotify, a stream chat or a guide stays on screen while
  the video, game or slideshow plays.
- **Just press fullscreen.** Double-click the video, press F11 or hit the fullscreen button like you
  always do. ScreenBound handles the rest.
- **Real fullscreen, not an overlay.** It moves the app's actual window: no picture-in-picture, no
  capture, no lag.
- **Works with PowerToys FancyZones.** Already snap windows with Microsoft PowerToys FancyZones?
  Snap an app into a zone, go fullscreen, and it fills that same FancyZones zone. No extra setup.
- **You choose which apps.** Turn it on for browsers and media players and leave games or anything
  else alone.
- **Undoes itself.** Exit fullscreen or quit ScreenBound and every window goes back to where it was.

## Who it's for

- **Ultrawide and super-ultrawide owners** (21:9, 32:9) who want videos to fill one area instead of
  the whole monitor.
- **Streamers and multitaskers** who want a big video or preview plus chat and tools beside it.
- **Anyone with a large display** who likes fullscreen but doesn't want it to take over the whole
  screen.

## How it works

1. **Draw a zone.** Drag it out on a preview of your monitor, pick a preset, or type an exact size
   in **percent** (scales with any resolution) or **pixels** (e.g. exactly 3840 × 2160).
2. **Choose your apps.** Turn apps on in the **Apps** tab, or sort them into **Groups** like
   "Browsers" or "Media" and turn a whole group on at once.
3. **Go fullscreen as usual.** The app fills your zone instead of the whole monitor. Leave
   fullscreen and it goes back to normal.

ScreenBound runs quietly in the system tray, reacts only when windows change (it doesn't poll in
the background), and keeps your settings across updates. Turn on **Start at boot** in Settings if
you want it ready from sign-in without opening the window.

## What to expect

**You may see a brief fullscreen flash.** When you hit fullscreen, the app still asks Windows for
real fullscreen first. ScreenBound only finds out *after* that and then moves the window into your
zone. That tiny gap can look like a quick flicker of the monitor filling up before it snaps into
place.

**Why we can't remove it (yet).** ScreenBound works from the outside: it watches window changes
and resizes the real window. It does not run inside the app or block Windows from honouring a
fullscreen request. To shrink the window, the app has to enter fullscreen first so we know it
happened. We apply the zone as fast as we can; on most setups the flash is very short.

## Install

Download one of these from a release:

| File | Use it when |
| --- | --- |
| `ScreenBound_*_x64-setup.exe` | You want a normal install (recommended) |
| `ScreenBound_*_x64.msi` | You deploy with MSI tooling |
| `ScreenBound_*_x64-portable.exe` | You want to run it without installing |

Windows 10/11 with WebView2 (preinstalled on Windows 11). Settings live in
`%APPDATA%\com.screenbound.app` and survive reinstalls unless you check **Delete app data** when
uninstalling.

## Build from source

Prerequisites: Rust (stable, MSVC), Node 20+, [Task](https://taskfile.dev), WebView2.

```powershell
task dev        # run locally
task release    # NSIS + MSI + portable exe into dist/
task deploy     # build and install for the current user
```

`task --list` shows everything else (`task test`, `task lint`, `task ci`, …).

## Docs

- [AGENTS.md](AGENTS.md) — architecture and engineering rules
- [PLAN.md](PLAN.md) — status, decisions, limitations
- [docs/fullscreen-research.md](docs/fullscreen-research.md) — observed native behavior
- [docs/manual-testing.md](docs/manual-testing.md) — test procedures
