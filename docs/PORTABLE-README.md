# StarRupture Overlay - Windows x64

A rupture countdown, compass and recipe research sidebar for StarRupture. Extract this complete folder to a location you want to keep and open `starrupture-overlay.exe`. Run normally, without administrator rights, then start StarRupture and load your world.

Right-click the overlay icon in the Windows notification area for Settings, positioning controls, About and Exit. Double-click it to open Settings. The icon may be under the hidden icons arrow. You can start the game from the tray menu or create a Play StarRupture with Overlay desktop shortcut in Settings.

## Recipe tracker

With the game focused, hover over an unfinished recipe in the recipe station and press Alt+X, or double-click its icon or name. Pin up to three recipes. Selecting an already pinned recipe unpins it; pressing the shortcut outside the station clears all pins. Each panel also has a close button and closes automatically when research completes. Your pins are remembered separately for each local world.

The tracker prepares its icons, names and font automatically from your installed game. Start the game and load a world to complete setup. Setup works offline, with no extra tools or downloads. Later launches reuse the local cache. Settings > Recipe tracker shows setup progress.

Counters show submitted/required research amounts. Bag quantities show what you carry and turn green when you hold enough to finish that requirement. A tick marks a completed requirement. Hover over a material for its name. Submit materials in the game.

## Compass and settings

The compass shows your heading and the direction and distance to your personal map pin. It works without a pin too. The marker and text turn blue when you arrive. Arrival defaults to 25 metres and clears beyond 35 metres; change either distance or turn off the cue in Settings > Compass.

The compass hides during pause and game menus. Recipe panels shrink when needed to fit beside the pause menu. The HUD hides while another application has focus.

Settings has a General tab and a tab for each feature. Turn features on or off independently and adjust the countdown format, compass arrival distances, recipe shortcut, placement and scale.

## Requirements and help

Windows 10/11 x64, a supported Steam game version (check the release notes for main-branch or PTB support), and borderless, windowed or the supported fullscreen mode. The app finds the running game automatically. Game updates may require an overlay compatibility update. About shows connection and version information; Copy support details prepares information for a bug report.

The countdown follows game time and pauses when simulation pause is verified. It shows `--:--` when timing is unavailable. The overlay hides with a minimized or closed game, and other windows can cover it.

The app is unsigned. Windows may show a SmartScreen warning on the first launch of a downloaded build. Only use More info > Run anyway after checking the source and deciding you trust the app. The release checksum checks download integrity; it does not remove that warning.

## Updates and removal

Exit the overlay before starting a new version. Extract updates into a new folder and keep the previous version for rollback. Open the new copy's Settings and choose Update shortcut to point an existing desktop shortcut to this copy, or Create shortcut if it is missing. If Start with Windows is enabled, disable it in the old copy's Settings and enable it in the new copy.

To uninstall, disable startup and remove the shortcut in Settings, exit the app and delete its extracted folder. Preferences and caches are stored separately under `%LOCALAPPDATA%/StarRuptureWaveOverlay`; delete that folder too if you want to remove local data. The app reads game memory and installed archives without changing the game or its saves.

## Support and licence

Help: https://github.com/ahjephson/star-rupture-overlay/blob/main/docs/SUPPORT.md

Bug reports and feature requests: https://github.com/ahjephson/star-rupture-overlay/issues

Pull requests and code contributions are not accepted. Keep saves, settings, caches, memory dumps and extracted assets out of public reports.

Licensed under PolyForm Strict 1.0.0, copyright (c) 2026 Alex Jephson; see LICENSE. Keep THIRD-PARTY-NOTICES.md and licenses/ with the app. An unofficial community utility, unaffiliated with StarRupture's developers or publisher.
