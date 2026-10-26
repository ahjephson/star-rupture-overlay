# Maintenance guide

## Behaviour contract

- Game access is limited to query/read permissions for the executable and main map in [compatibility](COMPATIBILITY.md). Unknown builds, ambiguous processes and changed object identities produce unavailable data.
- The UI thread owns the native HUD, tray icon, on-demand dialogs, fonts and GDI resources. Preferences and process-presence notifications remain separate from observation validation. The reader runs on one worker thread. Shutdown cancels and joins that worker before shared state is released.
- Menu observations affect layout only. Timer observations independently establish simulation time and pause evidence. Countdowns advance with validated simulation observations, and evidence older than five seconds becomes unavailable.
- Native Waiting displays STABLE; Moving displays APPROACHING. Fadeout displays INCINERATING for its first 8.5 simulation seconds, then COOLING. Both labels share the native Fadeout phase duration.
- Preserve the Oxanium typography, stage colours, frame geometry, background transparency, countdown formatting and play/menu positions. Window ordering places the HUD immediately above the game in the ordinary window stack.
- Preserve configuration keys, cache and diagnostic file formats, typed observation validation and the reader's observation compatibility version.

See [architecture](ARCHITECTURE.md) for timing limits, observation guards and native resource ownership.

## Development and validation

Use the pinned Rust toolchain and the first-party formatter described in [contributing](../CONTRIBUTING.md). Preserve vendored source, licence texts and asset hashes. Keep caches, captures, account-specific paths and generated output out of Git.

Build candidates into fresh ignored directories. Native diagnostics use the application's own hidden or off-screen windows; package/deployment tests use disposable fixtures. Live reader checks require the supported running game. Summarize check results and limitations in PR descriptions, use CI for automated results, and include release-specific live checks in release notes. Keep raw diagnostic output local.

Application deployment is a separate operation from building and testing. Exit the installed app before an explicitly requested upgrade and retain a verified rollback. Confine maintenance changes to this repository.

## Release status

Version 0.1.0 is in development. Public distribution remains subject to the work in [release readiness](PUBLIC-READINESS.md) and the checks in [releasing](RELEASING.md). Publication, tags, repository visibility and installation changes require explicit authorization.

Source provenance is documented in [EXTRACTION.md](EXTRACTION.md). Keep runtime records and backups in ignored local storage.
