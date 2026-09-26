# Changelog

All notable changes to Aragon are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Aragon adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: ../../compare/v1.0.0...HEAD
[1.0.0]: ../../releases/tag/v1.0.0
