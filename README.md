# StarRupture Overlay

A Windows HUD showing StarRupture's current wave stage and the countdown to the next stage.

The HUD uses Oxanium, stage colours and a game-style frame. Its countdown freezes when simulation pause is verified, and the HUD moves below the pause-menu border. It stays over the visible game when focus moves to another monitor. Windows covering the game also cover the HUD, which hides when the game is minimized or closed.

The native Waiting phase displays **STABLE**, and Moving displays **APPROACHING**. Fadeout displays **INCINERATING** for its first 8.5 simulation seconds, then **COOLING**.

## Run

Version 0.1.0 is in development. Release packages use the following installation procedure:

1. Download the Windows x64 ZIP and its matching `.sha256` file from this repository's Releases page.
2. Check the archive hash, extract it to a user-writable folder and double-click `starrupture-wave-overlay.exe`.
3. Start the supported game normally, or choose **Start StarRupture** from the tray menu. Display and position controls appear when the game is detected.

Keep the included licence notices with the executable. Configuration is optional. Run the app with standard user privileges. The current candidate is unsigned; its checksum verifies archive integrity.

## Compatibility

- Windows 10/11 x64, with borderless or windowed gameplay.
- The reader supports one SHA256-pinned Steam game build and detects its executable location from the running process. See [supported builds](docs/COMPATIBILITY.md) before reporting unavailable data.
- Unsupported game builds and maps display unavailable data.

## Build from source

Install Rust through rustup and Visual Studio C++ Build Tools with the Windows SDK. The repository pins Rust **1.99.1**, including Clippy and rustfmt. PowerShell 7, Git and Python 3 are used by the release checks.

```powershell
./scripts/Install-QualityTools.ps1
./scripts/Test-Dependencies.ps1
./scripts/Build.ps1 -OutputDirectory local/candidate
./scripts/Package-Release.ps1 -BuildDirectory local/candidate
```

The build checks formatting, runs Clippy with warnings denied, tests the workspace and verifies vendor hashes. It requires an empty output directory and produces the app, a dependency inventory, licence notices and a file-integrity manifest. Packaging preserves existing archives and requires a clean source checkout by default; `-AllowUncommitted` marks development archives as containing uncommitted changes.

`-Offline` uses previously cached dependencies; `-ToolchainDirectory PATH` selects an installed copy of the pinned compiler. [Contributing](CONTRIBUTING.md) explains quality checks; [releases](docs/RELEASING.md) covers review and draft preparation.

## Install, upgrade and roll back

For a portable installation, extract a reviewed ZIP to a new folder. Exit the old overlay from its tray before starting the new one; keep its folder until the new version is verified.

For a managed per-user installation from a source checkout:

```powershell
./scripts/Deploy.ps1 -ArchivePath PATH-TO-RELEASE.zip
./scripts/Deploy.ps1 -ArchivePath PATH-TO-NEW-RELEASE.zip -Launch
./scripts/Deploy.ps1 -Rollback -Launch
```

Exit the installed app before deployment and place the matching checksum file beside the ZIP. The script verifies the archive and every packaged file, installs under `%LOCALAPPDATA%/Programs/StarRuptureWaveOverlay`, and preserves the previous installation for rollback. Run it with standard user privileges. See [deployment and rollback](docs/DEPLOYMENT.md).

## Settings and data

Right-click the tray icon for **Settings**, **About** and **Exit**. Double-click the icon to open Settings. **Start with Windows** starts the overlay at sign-in for the current user, requires no administrator rights and is off by default. Windows Startup Apps controls can disable automatic startup.

Choose whether to keep the overlay running, ask before exiting or exit automatically after a previously connected game process closes. Keeping it in the tray is the default. Loading screens and unavailable readings leave the app running.

Settings can create or remove a desktop shortcut named **Play StarRupture with Overlay**. It starts the overlay, then requests game launch through Steam, reusing existing instances. Shortcut changes take effect immediately; Save applies startup and game-exit preferences, and Cancel discards unsaved preferences. Game launch requests time out after two minutes so you can retry.

About shows the app version, source revision, supported build, connection status and detected game location. Copy support details omits personal paths and game data. Licence notices are available in About. Game-exit preferences are stored in `%LOCALAPPDATA%/StarRuptureWaveOverlay/preferences.json`.

The app uses default configuration on first run and stores caches under `%LOCALAPPDATA%/StarRuptureWaveOverlay/cache`. To use another cache directory, set `cacheDirectory` in a configuration file and pass its path with `--config PATH`; see [config/example.json](config/example.json).

Countdowns follow simulation time. The countdown shows `--:--` when readings are unavailable or more than five seconds old. See [architecture](docs/ARCHITECTURE.md) for timing details.

## Licence and affiliation

First-party project code is licensed under **PolyForm Strict 1.0.0**, copyright (c) 2026 Alex Jephson. See [LICENSE](LICENSE) for the permitted uses and restrictions. Dependencies and embedded Oxanium fonts retain their own licences; redistribution notices are included in release archives. See [third-party notices](THIRD-PARTY-NOTICES.md).

An unofficial community utility for StarRupture.
