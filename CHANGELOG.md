# Changelog

## Unreleased

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

Version 0.1.0 is in development.
