# StarRupture Overlay — Windows x64

Extract this complete folder to a user-writable location and double-click starrupture-wave-overlay.exe. Run it with standard user privileges. Version, source revision and per-file hashes are recorded in manifest.json.

Right-click the tray icon to start StarRupture when it is absent, or show/hide and position the HUD when it is running. Settings, About and Exit are always available. Double-click the icon to open Settings. Unlock the HUD to move it, then lock it when finished. Exit through the tray before starting a different version.

The HUD moves below the pause-menu border and freezes its countdown when simulation pause is verified. STABLE is the game's native Waiting phase; APPROACHING is Moving. Fadeout displays INCINERATING for its first 8.5 simulation seconds, then COOLING.

The HUD stays immediately above the game window. Windows covering the game also cover the HUD. It remains visible over the game when focus moves to another monitor and hides when the game is minimized or closed. Unavailable readings or timer evidence older than five seconds display `--:--`.

## Supported game

Windows 10/11 x64 with borderless/windowed gameplay. The reader detects the installation location of the running `StarRuptureGameSteam-Win64-Shipping.exe` process.

Executable SHA256: `95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1`

Unknown builds or unsupported runtime states produce unavailable data. Game updates require a verified compatibility update.

## Settings

Start with Windows runs the overlay at sign-in for the current user and requires no administrator rights. It is off by default and waits in the tray for the game. Windows Startup Apps controls can disable automatic startup. The desktop shortcut starts the overlay and game through Steam. Creating or removing the shortcut takes effect immediately. Save applies startup and game-exit preferences; Cancel discards unsaved preferences.

Choose Keep running, Ask before exiting or Exit automatically when a connected game process closes. Keep running is the default. Game-exit preferences are stored in `%LOCALAPPDATA%/StarRuptureWaveOverlay/preferences.json`. About provides connection information, licence notices and support details that omit personal paths and game data.

## Data and updates

The app reads game memory with query/read permissions under standard user privileges. It stores discovery caches under `%LOCALAPPDATA%/StarRuptureWaveOverlay/cache` and validates cached references against the current process and session before reuse.

For updates, compare the ZIP checksum with its matching .sha256 file, extract to a new folder and keep the previous version until verified. The candidate is unsigned; the checksum verifies archive integrity. Before moving or uninstalling the app, disable Start with Windows and remove its desktop shortcut in Settings. After moving it, recreate those entries from the new copy. Uninstall by exiting the app and deleting its extracted folder; remove preferences and caches separately if desired.

## Licence and support

First-party code: PolyForm Strict 1.0.0, copyright (c) 2026 Alex Jephson; see LICENSE. Keep THIRD-PARTY-NOTICES.md and licenses/ with the executable. dependency-inventory.json lists dependency versions and licence declarations.

An unofficial community utility for StarRupture.

Source, documentation and issue reports: https://github.com/ahjephson/star-rupture-overlay

When reporting a problem, include overlay/game versions and reproduction steps. Keep saves, memory dumps, caches and personal settings private.
