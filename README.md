# StarRupture Wave Overlay

A Windows HUD showing StarRupture's current wave stage and the countdown to the next stage.

The HUD uses Oxanium, stage-specific colors and a game-style frame. It freezes with verified simulation pause, moves below the pause-menu border, stays over the visible game when focus moves to another monitor, and lets windows covering the game also cover the HUD. It hides when the game is minimized or closed. The native waiting phase is displayed as **STABLE**, and Moving as **APPROACHING**. FADEOUT displays **INCINERATING** for its first 8.5 simulation seconds, then **COOLING**.

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

The build runs formatting checks, strict Clippy, tests and vendor verification, then produces a candidate containing the app, dependency inventory, licence notices and a file-integrity manifest. Build/package output uses a fresh directory, preserving existing installations. The package step requires a clean source checkout by default. For development-only packaging, `-AllowUncommitted` marks the archive accordingly.

`-Offline` uses previously cached dependencies; `-ToolchainDirectory PATH` selects an installed copy of the pinned compiler. [Contributing](CONTRIBUTING.md) explains quality checks; [releases](docs/RELEASING.md) covers review and draft preparation.

## Install, upgrade and roll back

Portable deployment is simply extracting a reviewed ZIP to a new folder. Exit the old overlay from its tray before starting the new one; keep its folder until the new version is verified.

For a managed per-user installation from a source checkout:

```powershell
./scripts/Deploy.ps1 -ArchivePath PATH-TO-RELEASE.zip
./scripts/Deploy.ps1 -ArchivePath PATH-TO-NEW-RELEASE.zip -Launch
./scripts/Deploy.ps1 -Rollback -Launch
```

The matching checksum file must sit beside the ZIP. Deployment verifies the archive and every packaged file, installs side by side under `%LOCALAPPDATA%/Programs/StarRuptureWaveOverlay`, and preserves the prior installation. Exit the installed app before deployment. Deployment runs with standard user privileges. [Deployment details](docs/DEPLOYMENT.md).

## Settings and data

The app starts with default settings. Cache files are stored under `%LOCALAPPDATA%/StarRuptureWaveOverlay/cache`. Use `--config PATH` to select another cache directory; see [config/example.json](config/example.json).

Countdowns follow simulation time. The countdown shows `--:--` when readings are unavailable or more than five seconds old. See [architecture](docs/ARCHITECTURE.md) for timing details.

## Licence and affiliation

First-party project code is licensed under **PolyForm Strict 1.0.0**, copyright (c) 2026 Alex Jephson. See [LICENSE](LICENSE) for the permitted uses and restrictions. Dependencies and embedded Oxanium fonts retain their own licences; redistribution notices are included in release archives. See [third-party notices](THIRD-PARTY-NOTICES.md).

An unofficial community utility for StarRupture.
