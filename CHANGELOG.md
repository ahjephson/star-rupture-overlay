# Changelog

## 1.1.1 — 2026-11-19

This Windows x64 update supports the public Steam release of StarRupture Update 3.

### Codex

- Corporation level headings now show a cyan checkmark for unlocked levels, an amber **Next level** label and a padlock for later levels.
- The Corporations tab follows the Corporate Terminal order. Training requirements show their training milestone.
- **Produced items** follows each machine’s game-data recipe order, so entries stay in place as recipes unlock.

### Recipe pins

- Completing recipe research now removes its pin and frees the slot. Pins already converted to production pins by the previous version can be closed individually.
- Raw materials such as Titanium Ore and Helium-3 stay visible in ingredient lists. Their own entries omit **Pin recipe**.

## 1.1.0 — 2026-11-19

Windows x64 release for StarRupture Update 3: Custom Game. The overlay runs alongside the Steam game and reads game data locally.

### Codex and recipe pins

- Press **K** in gameplay to open the searchable Codex. Browse production recipes, buildings, interiors, equipment, and corporation levels and rewards. The shortcut is configurable in Settings.
- Compare recipes for the same output and follow links between ingredients, machines, and corporation unlocks. Base Core shows upgrade costs for unlocked levels and the next available upgrade.
- Pin production recipes, machine upgrades, or Base Core upgrade costs from the Codex. Track up to three pins across the Codex and Recipe Station, and restore your selections per local world. Recipe pins have individual close buttons, material tooltips, and optional double-click pinning in the Recipe Station.

### Compass, wave timer, and startup

- The compass shows your heading, personal map marker, distance, and a configurable arrival cue.
- The wave timer supports Rarely, Normal, and Often fire-wave settings and can show hours when needed or total minutes. The Pre-wave countdown now appears before the first rupture.
- Startup shows a brief splash and an optional Windows notification. The app prepares the Codex in the background after connecting to a loaded world.
- Manage the wave timer, compass, recipe tracker, and Codex separately in Settings.

## 1.1.0-update3.1 — 2026-11-17

### Added

- Update 3 PTB support, including Rarely, Normal and Often fire-wave settings.
- A compass with your personal map pin, distance and a visual arrival cue. Arrival and departure distances are configurable, defaulting to 25 and 35 metres.
- Up to three pinned recipes, remembered separately for each local world, with optional double-click pinning and individual close buttons.
- Recipe item hover feedback and name tooltips.
- Separate Wave timer, Compass and Recipe tracker settings, with independent enable controls.
- A countdown format choice: hours when needed or total minutes.

### Improved

- Faster compass and recipe startup, with responsive countdowns during recipe preparation.
- Disabled features release their resources and skip preparation.
- Compact recipe panels with aligned heights, readable names and pause-menu scaling that preserves their right-edge position.
- Green inventory amounts when your bag holds enough to complete a pinned requirement.
- More compact Settings and About windows, grouped controls and keypress capture for the recipe shortcut.
- A personal-waypoint flag matching the map marker, with blue arrival text and marker.

### Fixed

- Restore per-world recipe pins on Update 3 PTR2, including after restarting the overlay and switching worlds.

- Hide the HUD at the main menu and keep the compass hidden during pause, game menus and use of other applications.
- Show the Pre-wave countdown before the first rupture, and show INCINERATING for seven seconds.
- Keep recipe close buttons clear of the border and steady on refresh.
- Show the supported game version in About and copied support details.
- Preserve valid recipe shortcuts when an unsupported key is pressed.

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
