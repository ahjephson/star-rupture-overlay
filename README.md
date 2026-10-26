# StarRupture Wave Overlay

A Windows HUD showing StarRupture's current wave stage and the countdown to the next stage.

The HUD uses Oxanium, stage colours and a game-style frame. Its countdown freezes when simulation pause is verified, and the HUD moves below the pause-menu border. It stays over the visible game when focus moves to another monitor. Windows covering the game also cover the HUD, which hides when the game is minimized or closed.

The native Waiting phase displays **STABLE**, and Moving displays **APPROACHING**. Fadeout displays **INCINERATING** for its first 8.5 simulation seconds, then **COOLING**.

## Run

Version 0.1.0 is in development. Release packages use the following installation procedure:

1. Download the Windows x64 ZIP and its matching `.sha256` file from this repository's Releases page.
2. Check the archive hash, extract it to a user-writable folder and double-click `starrupture-wave-overlay.exe`.
3. Start the supported game normally. Use the overlay's tray menu to show/hide, unlock/move/lock, reset its position or exit.

Keep the included licence notices with the executable. Configuration is optional. Run the app with standard user privileges. The current candidate is unsigned; its checksum verifies archive integrity.

## Compatibility

- Windows 10/11 x64, with borderless or windowed gameplay.
- The current reader supports one SHA256-pinned Steam game build at its default C-drive installation path. See [supported builds](docs/COMPATIBILITY.md) before reporting unavailable data.
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

The app starts with default settings and stores caches under `%LOCALAPPDATA%/StarRuptureWaveOverlay/cache`. To use another cache directory, set `cacheDirectory` in a configuration file and pass its path with `--config PATH`; see [config/example.json](config/example.json).

Countdowns follow simulation time. The countdown shows `--:--` when readings are unavailable or more than five seconds old. See [architecture](docs/ARCHITECTURE.md) for timing details.

## Licence and affiliation

First-party project code is licensed under **PolyForm Strict 1.0.0**, copyright (c) 2026 Alex Jephson. See [LICENSE](LICENSE) for the permitted uses and restrictions. Dependencies and embedded Oxanium fonts retain their own licences; redistribution notices are included in release archives. See [third-party notices](THIRD-PARTY-NOTICES.md).

An unofficial community utility for StarRupture.
