# Changelog

## Unreleased

### Added

- Visual waypoint arrival cue near your personal pin.
- Remember pinned recipes separately for each local world and refresh their progress when you return.

- Floating compass with a personal-pin marker, horizontal distance and a setting to turn it off; hidden during pause.

- Update 3 PTB compatibility for rupture timing and recipe tracking, including Rarely, Normal and Often fire-wave frequency settings.
- Countdown format setting for hours when needed or total minutes, with a fixed panel width.
- Track up to three recipes at once, with optional double-click pinning and individual close buttons.
- Hover feedback for recipe close buttons and materials, with item-name tooltips.

### Changed

- Ingredient inventory amounts turn green when your bag holds enough to finish that requirement.
- More compact Settings and About windows, with aligned sections, standard-sized desktop buttons and a Recipe tracker tab that fits its content.
- Set the recipe shortcut by pressing the key combination in Settings.
- Hide overlays at the main menu and hide the wave panel while waves are stopped.
- Show INCINERATING for seven seconds before switching to COOLING.
- Smaller recipe icons and tighter panel spacing, with a fixed right-edge position and pause-menu scaling only when needed.
- Recipe panels share a common height, with clear two-line names, larger inset close buttons and adornments on the outer panels.

### Fixed

- Keep the compass hidden while typing in another application, and restore it when the game regains focus.

- Keep the wave panel hidden on the main menu and while waves are stopped when switching windows.

- Reject unsupported captured shortcut keys instead of changing them to another key.
- Allow disabling recipe tracking while a captured shortcut is invalid, keeping the previous valid shortcut.
- Bound recipe-cache reads before loading their contents and retry failed tooltip/close-button positioning.

- Show the supported game version in About and copied support details.
- Show the Pre-wave countdown before the first rupture start time is set.
- Keep recipe close buttons clear of the scaled border, steady on refresh and using the game's cursor appearance when available.

## 1.0.0 — 2026-10-28

### Features

- Rupture stage and countdown overlay that follows game time and pauses with the game.
- Recipe research sidebar with material icons, submitted/required amounts, completion ticks and bag quantities.
- Alt+X to pin, switch or clear recipes, with automatic closing when research completes.
- Automatic offline recipe setup using the installed game, with cached assets for later launches.
- Tray controls and Settings for visibility, positioning, recipe scale and keyboard shortcuts.
- Optional startup with Windows, a desktop shortcut to launch the app and game, and a choice of what happens when the game closes.

### Improvements

- Fullscreen support, including Alt-Tab and switching back to Borderless.
- Lower memory use during startup and regular play.
- Faster recipe selection and clearing, including clearing outside the recipe station.
- Update existing desktop shortcuts to point to the current app copy.
- Recipe counters and panel styling that match the game's presentation.
- Clearer Settings appearance and populated app details in Windows.
