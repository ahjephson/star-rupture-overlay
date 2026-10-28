# StarRupture Overlay - Windows x64

A rupture countdown and recipe research sidebar for StarRupture. Extract this complete folder to a location you want to keep and open `starrupture-overlay.exe`. Run normally, without administrator rights, then start StarRupture and load your world.

Right-click the overlay icon in the Windows notification area for Settings, positioning controls, About and Exit. Double-click it to open Settings. The icon may be under the hidden icons arrow. You can start the game from the tray menu or create a Play StarRupture with Overlay desktop shortcut in Settings.

## Recipe tracker

With the game focused, hover over an unfinished recipe in the recipe station and press Alt+X. Press it over a different unfinished recipe to switch, over the same unfinished recipe to clear, or outside the station to clear. A completed recipe or empty space inside the station keeps the current pin.

The tracker prepares its icons, font and recipe definitions automatically from your installed game. Setup works offline using the built-in Rust asset reader. Progress appears in Settings > Recipe tracker; later launches reuse the local cache. No manual extraction or separate SDK is required.

Counters show submitted/required research amounts. White text means you have some material in your bag, and the orange number shows that quantity. A tick marks a completed requirement. Submit materials in the game.

## Requirements and help

Windows 10/11 x64, the supported Steam game build, and borderless, windowed or the supported fullscreen mode. The app finds the running game automatically. Game updates may require an overlay compatibility update. About shows connection and version information; Copy support details prepares information for a bug report.

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
