# StarRupture Wave Overlay

A standalone Windows overlay showing the current wave stage and the time until the next stage. One executable contains both the HUD and its persistent read-only sampling worker. No MCP server, Codex, Node.js, save data or extracted game datasets are required at runtime.

The angular HUD uses bundled Oxanium fonts and stage colors. It stays visible while paused and when the game loses focus, uses a slightly transparent background, and moves below the pause border while the pause menu is open. Right-click its tray icon to hide, move, lock, reset or exit.

## Build and run

Requirements: Windows x64, Rust stable with edition2024 support (1.88+), and Visual Studio C++ Build Tools/Windows SDK. The supported game executable is deliberately SHA256-pinned; unknown builds fail closed. See docs/HANDOVER.md.

```powershell
./scripts/Build.ps1
./scripts/Start-Overlay.ps1
```

Double-click **`dist/starrupture-wave-overlay.exe`** to run. No command-line arguments, configuration file or second executable are needed. The release app creates no console window, prevents duplicate instances and reports startup failures in a Windows dialog. Exit through its tray menu. Fonts are embedded and registered privately.

`Build.ps1` tests and builds the workspace, packages that one executable and the font license in ignored `dist/`, and removes the obsolete reader executable there. `Start-Overlay.ps1` is an optional convenience launcher. The app stores private discovery caches under `%LOCALAPPDATA%/StarRuptureWaveOverlay/cache`; they are checked against the current process/build/session before use. It waits for a supported game and hides the HUD while the game is absent or minimized.

Optional `--config PATH` accepts the example in `config/example.json`; relative cache paths resolve against that file. Existing helper configuration fields are accepted for compatibility, but no external helper is launched or required. Explicit `live.enabled=false` still disables live startup. No game Save menu or MCP registration is needed.

For an external Rust installation, pass `-ToolchainDirectory PATH` to Build.ps1. An optional `-Offline` build uses an already populated local Cargo cache. Normal builds do not require either original repository.

Optional read-only live validation, with the supported game running:

```powershell
python ./scripts/test_reader.py
./dist/starrupture-wave-overlay.exe --self-test-window
```

## Components and boundaries

- `crates/wave-overlay`: native layered-window renderer/tray and joined background worker lifecycle.
- `crates/reader`: embedded read-only library; retained process/context, fast pause checks and complete timer observations.
- `crates/wave-core`: extracted pure observation validation, native timing and overlay-only configuration.
- `crates/observation-model`: pure presentation and bounded observation envelopes.
- `third-party`: retained source dependencies and upstream notices, not game data.

Pause checks target50ms and world/timer samples250ms; full read latency can defer pause checks. Timers use observed simulation time, freeze with verified pause and become unavailable when stale. There is no invented wall-clock countdown, game hook or process write. Source-build pins and allocation/session guards deliberately limit compatibility rather than guessing. Worker cancellation is checked before each bounded memory read and during discovery/hashing, with monotonic read budgets; shutdown joins the worker and closes its read-only handles. Sampling runs in the app process, so it shares the app lifetime.

This project was extracted from the private Rust MCP development workspace on2026-10-24. Historical implementations and private rollback backups are retained. Future overlay work belongs here. See docs/HANDOVER.md, docs/EXTRACTION.md and THIRD-PARTY-NOTICES.md.
