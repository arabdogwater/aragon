# Changelog

All notable changes to Aragon are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Aragon adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.1] - 2026-11-07

### Fixed

- **Removed places came back.** Removing a place only deleted it until something re-created it: Studio opening it,
  the Roblox account refresh or the project scan. A removed place now stays removed. If Studio opens it, the plugin
  says it was removed instead of syncing. Settings → *Removed places* adds it back (it gets a new setup) or forgets it
  for good. Adding it by place ID brings it back too. Places removed with an older version need removing once more
- **Two places could share one folder.** Auto-mapping and the project scan could hand a folder that already belongs to
  a place to a second one. A folder now belongs to exactly one place, and setup refuses a folder another place owns
  unless you carry it over

### Added

- **Carry a folder over to a remade place.** Setup suggests the folders of your other places (same name first) under
  *Remade this game? Carry over a folder*. Picking one, or browsing to a folder another place owns, switches to *Files
  win* so the code is kept. Carrying over unlinks the old place (it needs a new setup) and removes it from the
  dashboard unless you untick that. `placeIds` / `gameId` pinned in the project file are pointed at the new place
- **Right-click menus** on places (Places page, top tiles, the live strip, the place page and its new … button): open
  in Studio, folder, editor or code, move to a group, copy IDs or the folder path, pause sync, change or unmap the
  folder, remove from the dashboard
- **Organize the Places page your way.** Drag places between groups and into any order, drag whole groups up and down,
  and make your own groups (*New group*, or right-click → Move to group → New group). Right-click a group to rename or
  delete it. The place page still shows the real owner, plus your group as a badge

### Changed

- Long paths in Settings wrap instead of widening the page

## [1.1.0] - 2026-11-07

### Added

- **One-click updates.** A new release shows up in a corner card with its headlines. *Update now* downloads it with a
  live progress bar, checks it against the release's SHA-256 checksums, swaps the exe and restarts Aragon by itself.
  Studio reconnects on its own and the new Studio plugin is installed with it. Settings → Updates checks on demand
- **Safety net.** Every file Studio overwrites or deletes on disk is kept for 14 days, even with nothing committed to
  git. Place → *History* groups them by when it happened ("Studio deleted 29 files"), compares any copy with the file
  as it is now, and puts back one file or a whole batch, which syncs straight into Studio
- **Studio's Output in the dashboard, and for your AI.** Prints, warnings and errors from Studio (edit mode and
  playtests, server and client) show up in Place → *Output*. Script references like `ServerScriptService.Shop:42`
  are links that open the file at that line. Everything is also written to `.aragon/studio-output.log` in the project,
  each error tagged with its file, so an AI agent can read runtime errors itself. *Copy for AI* copies the latest
  errors with their files, ready to paste
- **Sync problems in plain words.** Warnings that used to hide in a log file now show up as cards on the place, with a
  one-click fix: two Studio instances sharing one file (*Select them in Studio*), a file locked by another program,
  Wally packages not installed (*Run wally install*), Studio and the files out of step (*Reconnect*). Your AI can read
  the same list in `.aragon/sync-problems.md`
- **Live sync activity.** Place → *Activity* lists every file synced and in which direction, so "did that edit reach
  Studio?" takes a glance. The home page shows the last synced file under every place open in Studio
- **Command palette.** `Ctrl K` (or `Ctrl P`) anywhere: jump to a place, a page, any file in any project, or run an
  action (open in Studio, reconnect, build, output, history, check for updates)
- **Your editor, at the right line.** Aragon finds VS Code, Cursor, Windsurf, Zed and Sublime Text. Pick one in
  Settings → Code editor and Studio's *Open In Editor*, output links and the palette open files there, at the line,
  inside the project's window. The place page and the built-in editor get an *Open in …* button
- *Run code* shows the output of what you ran right under it

### Fixed

- After an update the new Studio plugin could wait up to an hour to be installed (the local plugin check was throttled
  together with the online update check)
- Studio's *Open In Editor* ignored the line it was asked to open

## [1.0.6] - 2026-11-07

### Fixed

- **Folders duplicated in Studio (`Data`, `Data`, `Modules`, `Modules`, ...).** With Two-way sync on, the plugin
  treated an instance leaving its parent as deleted on the spot, and the same instance coming back as a brand new one
  with a new ID. A script that swaps a folder for a fresh copy and puts the original back (or an undo + redo) sent
  "remove old ID + add new ID" in one batch. The hub applied the add first and skipped it because the folder was
  still on disk ("already exists! Skipping"), then applied the removal and **deleted the folder from disk**. Studio
  kept the folder under an ID the hub never knew, so every edit to its scripts was dropped ("Attempted to update
  instance that doesn't exist"), and putting the files back made the hub add the folder again: a twin in Studio.
  Fixed in three places:
  - The plugin decides a removal only when the batch is sent: an instance back under the same parent sends nothing,
    one added and destroyed in the same batch cancels out, and a real move is sent as remove + add
  - The hub applies Studio's removals before its additions, so replacing an instance (delete `X`, create a new `X`)
    writes the new one instead of deleting both
  - When the files add something Studio already has untracked (kept by Keep unknowns, restored by an undo), the
    plugin adopts it instead of creating a second one, and applying the same instance twice (LiveHydrate fetching a
    missing parent with its children) no longer duplicates the child that triggered it
- A removed instance now stops being tracked together with its descendants. They stayed tracked before, so when it
  came back its scripts were left out of what was sent to the hub
- The Studio watcher dropped only the removed instance's listeners, not its descendants', so an instance that was
  moved and came back reported every change twice

## [1.0.5] - 2026-10-17

### Fixed

- **"Keep unknowns" didn't keep anything when the files removed or re-created an instance.** The setting only
  applied when an instance's own keep flag changed, so a removal from disk destroyed every Studio-only instance
  inside it: deleting the last file of a folder took the Studio-only VFX in that folder with it. With the setting on
  (or `$keepUnknowns` on the node), a removed instance that holds Studio-only instances now stays in Studio, detached
  from sync (synced children with nothing Studio-only inside are still removed). When the same batch re-creates it
  (same parent, name and class), its Studio-only children move into the new one
- **Instances synced from Studio into a project file were deleted from Studio on the next edit of that file.** An
  instance with children added under a project node (no `$path`) was written to the project file correctly, but its
  children were registered under its parent in the sync tree. The next time the project file was read nothing
  paired, so Studio was told to remove the whole subtree and re-add it from disk, losing whatever wasn't on disk (a
  StarterCharacter's configured Animate lost all its animation values). Children now sit under their own parent, and
  the instance's project node keeps them

## [1.0.4] - 2026-10-09

### Fixed

- **Folders linked into a project (symlinks / Windows junctions) never synced their edits.** Windows' recursive file watcher stops at a junction, so a change inside a linked folder (a shared module library linked into several games) reached Studio only after the hub restarted. Every link inside a watched `$path` (or a `$path` that is itself a link) now gets its own watch on the real folder, and its events are reported under the link path the project knows. Links nested inside linked folders are followed too

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
