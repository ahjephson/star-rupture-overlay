# StarRupture Wave Overlay — Windows x64

Extract this complete folder to a user-writable location and double-click starrupture-wave-overlay.exe. Run it with standard user privileges. Version, source revision and per-file hashes are recorded in manifest.json.

Right-click the tray icon to show/hide, unlock/move/lock, reset position or exit. Exit there before starting a different version. The app moves below the pause-menu border and freezes its countdown when simulation pause is verified. STABLE is the game's native waiting phase; APPROACHING is Moving. FADEOUT displays INCINERATING for its first 8.5 simulation seconds, then COOLING.

The HUD stays immediately above the ordinary game window. Other windows covering the game also cover it, and it remains visible over the game when focus moves to another monitor. Unavailable/stale timer evidence displays --:--.

## Supported game

Windows 10/11 x64 with borderless/windowed gameplay. The reader supports the default C-drive Steam installation at:

C:\Program Files (x86)\Steam\steamapps\common\StarRupture\StarRupture\Binaries\Win64\StarRuptureGameSteam-Win64-Shipping.exe

Executable SHA256: 95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1

Unknown builds, alternate installation paths or unsupported runtime states produce unavailable data. Game updates require a verified compatibility update.

## Data and updates

The app reads game memory through a query/read-only process handle. Discovery caches live under %LOCALAPPDATA%/StarRuptureWaveOverlay/cache and contain runtime references checked against current process and session identities before reuse.

For updates, compare the ZIP checksum with its matching .sha256 file, extract to a new folder and keep the previous version until verified. The candidate is unsigned; the checksum verifies archive integrity. Uninstall by exiting the app and deleting its extracted folder; remove the cache separately if desired.

## Licence and support

First-party code: PolyForm Strict 1.0.0, copyright (c) 2026 Alex Jephson; see LICENSE. Keep THIRD-PARTY-NOTICES.md and licenses/ with the executable. dependency-inventory.json lists dependency versions and licence declarations.

An unofficial community utility for StarRupture.

Source, documentation and issue reports: https://github.com/ahjephson/star-rupture-overlay

When reporting a problem, include overlay/game versions and reproduction steps. Keep saves, memory dumps, caches and personal settings private.
