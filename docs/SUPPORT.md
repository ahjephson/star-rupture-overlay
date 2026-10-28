# Help and troubleshooting

## The overlay does not appear

Check the Windows notification area for the overlay's tray icon, including hidden icons. Start StarRupture and load a world, then right-click the icon and check the display controls. Borderless, windowed and the tested fullscreen mode are supported. In fullscreen, the overlays hide while another app has focus and return when you return to the game. If they do not appear on your configuration, try Borderless. The overlay hides when the game is minimized and may be covered by other windows.

Open **About** to check connection status and the supported game version. A game update may need a matching overlay update. See [compatibility](COMPATIBILITY.md).

## The countdown shows `--:--`

The app has no recent validated timing reading. Loading screens, unsupported builds and changing game state can cause this. Wait for the world to finish loading and check About for connection status. The app displays unavailable timing rather than continuing an old countdown.

## Alt+X does nothing

Keep StarRupture focused, open the recipe station and hover over one unfinished recipe row. Check **Settings > Recipe tracker** for setup progress or a shortcut conflict. Try another shortcut if Windows or another app has already registered Alt+X.

Outside the station, Alt+X clears an existing pin. Inside the station, a completed recipe or empty space keeps the current selection.

## Recipe setup cannot finish

The tracker prepares its icons, font and recipe information automatically from the supported installed game. Setup works offline using the built-in Rust asset reader. Derived game assets are cached locally; no components are downloaded and no game assets are uploaded.

Check that Steam has finished installing or updating the game, and load a supported world. Save Settings with Recipe tracker turned off, then enable it and save again to retry, or restart the overlay. Game updates that change the supported build need an overlay compatibility update; retrying cannot add support for an unknown build.

Cached assets are checked before reuse. Missing or changed assets trigger preparation again. The wave countdown can continue if recipe setup fails. A long first setup can temporarily leave its timing reading unavailable while the shared worker finishes preparation.

## Checking a download

Download the ZIP and its matching `.sha256` file from this repository's [Releases page](https://github.com/ahjephson/star-rupture-overlay/releases). To check the ZIP, open PowerShell in your download folder and run:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath '.\THE-DOWNLOADED-FILENAME.zip'
```

Replace the filename with the ZIP you downloaded. Compare the displayed hash with the value in its `.sha256` file. A match confirms that the download has the published bytes; it does not establish publisher identity or remove SmartScreen warnings.

The app is unsigned. If Windows shows **Windows protected your PC**, only choose **More info > Run anyway** after checking the source and deciding you trust the download. You do not need to turn off Windows security or run the app as administrator.

## Local data

Preferences and caches are stored under `%LOCALAPPDATA%/StarRuptureWaveOverlay`. Recipe assets, native type schemas and cache receipts are kept in `recipe-cache`. The app reads the running game and its installed archives without changing game memory, files or saves.

Before deleting local data, exit the overlay. Removing the recipe cache means the next setup prepares the assets again from your installed game. See the [README](../README.md#updating-or-removing-the-app) for removal and shortcut instructions.

## Reporting a bug or requesting a feature

Use the repository's [issue forms](https://github.com/ahjephson/star-rupture-overlay/issues/new/choose). Include **About > Copy support details**, reproduction steps and a redacted screenshot where useful. Keep saves, settings, caches, extracted assets and memory dumps private. Pull requests are not accepted.

For suspected security vulnerabilities, follow [SECURITY.md](../SECURITY.md) rather than posting details publicly.
