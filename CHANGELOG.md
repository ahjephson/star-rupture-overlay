# Changelog

## Unreleased

- Optional recipe research tracker with Alt+X to pin a hovered unfinished station recipe, switch recipes or clear the current pin. Outside the station, Alt+X clears the panel; invalid and completed station targets preserve it.
- Narrow vertical panel with native recipe/material icons, submitted/required counters, completion ticks and a downward header frame. Counters match the game's left alignment, two-line partial layout, outline and shadow.
- Bag availability colours and an orange in-tile quantity, with unavailable readings kept distinct from zero stock. A new panel appears only after its first successful read.
- Recipe tracker settings for enablement, shortcut, position and scale, preserving existing preferences and shortcut conflict reporting.
- Read-only recipe and bag observations on the existing worker. Connection setup prepares reflected bindings; normal capture, clearing and menu reopening follow player-owned UI references without global object scans.
- Local game asset importer, process-private Nippo font loading and finite recipe reader/render diagnostics. Extracted game assets remain outside source control and standard release archives.

- StarRupture Overlay naming, status-aware tray controls, Settings and About dialogs.
- Game detection across installation locations with existing build and identity checks.
- Optional per-user startup at sign-in, a desktop shortcut for overlay/game launch, and configurable handling when a connected game closes.
- Cached development CI, with release packaging and deployment checks in a separate verification workflow.
- Windows x64 HUD displaying wave stages and simulation-based countdowns.
- Game-style frame, Oxanium typography, stage colours and embedded Explorer, window, taskbar and tray icons in nine native sizes.
- STABLE for native Waiting and APPROACHING for Moving. Fadeout displays INCINERATING for its first 8.5 simulation seconds, then COOLING, with countdowns to each display transition.
- Verified simulation pause, pause-menu layout and unavailable or stale timer handling.
- Window ordering immediately above the game, preserving visibility when focus moves elsewhere and allowing covering windows to cover the HUD.
- Queued Windows focus and restore notifications that repair HUD ordering independently of the UI timer and reader lock.
- Tray controls, startup with default settings and single-instance protection.
- Typed observations, named native settings and runtime memory bindings, bounded payload storage and typed retry errors.
- Interruptible sampling waits and reusable GDI render surfaces with owned resource cleanup.
- Pinned build and quality checks, portable packaging with licence notices, checksums and reversible per-user deployment.
- Dependency advisory fixes and bounded local cache reads.

Version 0.2.6 is in development. The tracker passed manual use, including the corrected counter appearance and fast outside-station clearing on 2026-10-27. Full release acceptance remains separate.
