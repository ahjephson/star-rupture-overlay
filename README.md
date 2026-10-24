# StarRupture Wave Overlay

A standalone Windows overlay showing the current wave stage and the time until the next stage. It owns one persistent read-only helper. No MCP server, Codex, Node.js, save data or extracted game datasets are required at runtime.

The angular HUD uses bundled Oxanium fonts and stage colors. It stays visible while paused and when the game loses focus, uses a slightly transparent background, and moves below the pause border while the pause menu is open. Right-click its tray icon to hide, move, lock, reset or exit.

## Build and run

Requirements: Windows x64, Rust stable with edition2024 support (1.88+), and Visual Studio C++ Build Tools/Windows SDK. The supported game executable is deliberately SHA256-pinned; unknown builds fail closed. See docs/HANDOVER.md.

```powershell
./scripts/Build.ps1
./scripts/Start-Overlay.ps1
```

`Build.ps1` tests and builds the Rust workspace, copies the two executables and font license to ignored `dist/`, and creates ignored `local/overlay.json` with the correct reader SHA256 and project-local cache directory. `Start-Overlay.ps1` launches a hidden console and refuses duplicate overlay instances. No game Save menu or MCP registration is needed. Exit through the tray menu. The font files are embedded and registered privately by the overlay.

For an external Rust installation, pass `-ToolchainDirectory PATH` to Build.ps1. An optional `-Offline` build uses an already populated local Cargo cache. Normal builds do not require either original repository.

Optional read-only live validation, with the supported game running:

```powershell
python ./scripts/test_reader.py
./dist/starrupture-wave-overlay.exe --self-test-window
```

## Components and boundaries

- `crates/wave-overlay`: native layered-window renderer/tray and owned reader lifecycle.
- `crates/reader`: dedicated external read-only helper; retained process/context, fast pause checks and complete timer observations.
- `crates/wave-core`: extracted pure observation validation, native timing and overlay-only configuration.
- `crates/observation-model`: pure presentation and bounded stream protocol.
- `third-party`: retained source dependencies and upstream notices, not game data.

Pause checks target50ms and world/timer samples250ms; full read latency can defer pause checks. Timers use observed simulation time, freeze with verified pause and become unavailable when stale. There is no invented wall-clock countdown, game hook or process write. Source-build pins and allocation/session guards deliberately limit compatibility rather than guessing.

This project was extracted from the private Rust MCP development workspace on2026-10-24. The original implementation, working runtime and backups remain intact. Future overlay work belongs here. See docs/HANDOVER.md, docs/EXTRACTION.md and THIRD-PARTY-NOTICES.md.
