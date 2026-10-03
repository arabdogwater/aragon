# Changelog

All notable changes to Aragon are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Aragon adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.3] - 2026-09-30

### Fixed

- **Importing a game from Studio could silently write only part of it.** The hub applied a Studio write as one all-or-nothing batch, so the first instance it couldn't write aborted everything after it. A place with Wally packages (`ReplicatedStorage.Packages`) lost every script that came after them: 35 of 209 scripts reached disk, and ServerScriptService and StarterPlayer never did. Each change is now applied on its own, and a failure is logged without dropping the rest
- Project folders declared with `$path` that don't exist yet (like `Packages` before `wally install`) are created on the first write from Studio instead of failing with "The system cannot find the path specified"
- A Studio write whose parent has no source on disk no longer crashes the whole hub (it used to `panic!`, and the crash handler exits the process)

### Added

- The hub writes a log to `~/.aragon/logs/hub.log` (timestamped, at least info level, restarted after 4 MB). The tray app has no console, so its sync errors used to be invisible

## [1.0.2] - 2026-09-28

### Changed

- **Idle dashboard uses ~0% CPU and GPU** (was ~10% CPU / 8% GPU): no looping animations, clouds and bees are baked into one static background, no permanent GPU layers
- **WebView2 memory ~62 MB → ~19 MB** with the dashboard open and idle
- Minimizing the dashboard suspends WebView2 immediately; after 30 s minimized it is released entirely (0 MB) and rebuilt on the same page when restored
- The hub only pushes state to the dashboard when something visible changed, instead of re-rendering the whole page every 30 s
- Unused browser features (SmartScreen checks, autofill, status bar, gestures) are turned off

## [1.0.1] - 2026-09-26

### Fixed

- Finishing onboarding for a new place could leave the dashboard on "Studio is connecting right now" until Studio was manually disconnected and reconnected. Mapping now moves the Studio session out of setup right away, and the dialog always closes after its celebration
- The Studio connector no longer misses a reconnect request that arrives while it's mid-hello, retries hellos that hang, and can't silently stop while its heartbeat keeps a stale session alive
- The hub never waits on a terminal prompt (like "apply 20 000 changes from Studio?" during a big import, or the Wally install question) that nobody can answer

### Added

- The plugin's Connecting page shows what the first sync is doing (downloading, comparing, sending N changes), so big imports visibly make progress

## [1.0.0] - 2026-09-26

First release of Aragon, built on Argon 2.0.29.

### Added

- **Hub**: one local server (`localhost:7373`) that syncs every open place at once, each with its own sync core under `/p/<placeId>/`
- **Dashboard**: native window (WebView2) with Places, Analytics, Code and Settings, plus a tray icon. Closing the window frees WebView2 and the hub keeps syncing at ~30 MB
- Places found automatically from the Studio user ID through public Roblox APIs, grouped by owning user or group, with names, icons, place and universe IDs
- Optional Open Cloud API key (stored in Windows Credential Manager) to include private group experiences
- Automatic folder mapping (`Projects/<owner>/<game>`) and a setup dialog when an unmapped place connects
- Most-worked-on ranking and analytics: lines written per day, lines of code, languages, time synced and streaks
- Built-in Monaco code editor. Studio's *Open In Editor* opens files in the dashboard
- Every Argon sync and plugin setting editable from the dashboard, globally and per place
- One-click actions: run Luau in Studio, build places, Wally install, open in Studio or Creator Hub
- First-run welcome that installs the bundled Studio plugin and can restart Studio gracefully
- Single instance: launching Aragon again focuses the open dashboard

### Changed

- The Studio plugin is now a connect-only connector with a hand-drawn town scene that scales to fit instead of scrolling
- New global defaults: `InitialSyncPriority = Client` (Studio wins on first sync) and `KeepUnknowns = true`
- Stats are only stored locally and are never uploaded
- Self-update checks this repository's releases

### Removed

- VS Code extension integration. Aragon never needs an external editor
- Plugin Settings, Help and Project widgets (everything moved to the dashboard)

[Unreleased]: ../../compare/v1.0.2...HEAD
[1.0.2]: ../../compare/v1.0.1...v1.0.2
[1.0.1]: ../../compare/v1.0.0...v1.0.1
[1.0.0]: ../../releases/tag/v1.0.0
