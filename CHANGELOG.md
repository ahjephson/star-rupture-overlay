# Changelog

## Unreleased

### Added

- Update 3 PTB compatibility for rupture timing and recipe tracking, including Rarely, Normal and Often fire-wave frequency settings.
- Countdown format setting for hours when needed or total minutes, with a fixed panel width.
- Track up to three recipes at once, with optional double-click pinning and individual close buttons.
- Hover feedback for recipe close buttons and materials, with item-name tooltips.

### Changed

- Hide overlays at the main menu and hide the wave panel while waves are stopped.
- Show INCINERATING for seven seconds before switching to COOLING.
- Smaller recipe icons and tighter panel spacing, with a fixed right-edge position and pause-menu scaling only when needed.
- Recipe panels share a common height, with clear two-line names, larger inset close buttons and adornments on the outer panels.

### Fixed

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
