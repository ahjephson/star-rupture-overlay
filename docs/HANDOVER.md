# Maintainer handover

Status: public-release preparation verified locally and by fresh Windows CI; repository remains private. No release or visibility change is authorized by this preparation.

Read README.md, docs/ARCHITECTURE.md, docs/COMPATIBILITY.md and docs/RELEASING.md before changes. docs/validation.json records the latest completed checks. Historic session-specific development notes and the pre-preparation Git bundle are retained only in ignored local backups. Existing Git history is preserved; a targeted historical secret/account-path review found no matching formats, with limitations recorded in docs/PUBLIC-READINESS.md.

## Preserve

One Windows GUI app owns the native HUD and embedded read-only worker. Never add memory writes, injection, game hooks, save access, debug privileges or an arbitrary helper executable. Preserve exact game-build/path pins, object/world/allocation guards, bounded observations and cooperative cancellation. Fast menu evidence controls layout only; timer evidence and simulation pause remain independent.

Keep the approved Oxanium frame, stage colors and approved phase display labels, slight background transparency and play/pause positioning. Do not deploy a candidate over a running app. Build into a fresh ignored output folder; Deploy.ps1 is a separate explicit operation.

## Current decisions

Rust1.99.1, locked registry dependencies and immutable action pins are used for reviewed Windows builds. Source is formatted separately from vendor code. Vendor manifest-only safety/publish patches are documented and hash-verified. The source-include shim was replaced with a normal reader library; process handles stay on its worker thread. Local caches are bounded before parsing. The release executable statically links the VC runtime and uses thin LTO/stripped symbols.

No signing identity, public visibility change, version tag or release publication has been configured or performed. The release workflow creates drafts only after manual dispatch and confirmed live verification. Signing/provenance and repository protection settings must be reviewed before the first public launch.

This project is independent. Do not modify any planning workspace, MCP server, registrations, controller journals, datasets or old server copies while maintaining it. Local caches, builds, captures and private notes stay outside Git.

The owner reiterated preparation only: do not deploy, switch or replace the installed app. Deployment validation records refer to disposable test fixtures, never the actual installation.

Latest verified code/package commit: `b227eabd1263c4f819e40e54b8506b5daa675a5e`; [Windows CI](https://github.com/ahjephson/star-rupture-overlay/actions/runs/40124395614) passed. The final follow-up changes only readiness records. The manually dispatched draft workflow was not invoked. No deployment, app replacement, public release, tag or visibility change was performed.

## New candidate: ordinary stacking and STABLE label — 2026-10-24

Implemented the owner's requested game-relative stacking: HUD remains above the visible game when focus moves elsewhere, but applications above the game can cover it. Removed global topmost style and all normal HWND_TOPMOST placement. Changed only the display label WAITING to STABLE, retaining native waiting identifiers and timing. Candidate-only development: do not deploy or replace the running overlay. The controlled-window diagnostic and existing native/render checks verify the change; actual two-monitor user appearance remains pending an explicitly authorized candidate switch. No MCP or memory-reader changes. Pre-change source is recoverable under ignored local/backups/before-window-stacking-20261024.

## Authorized stacking activation — 2026-10-24

The owner explicitly authorized switching the running standalone overlay for the two-monitor test, retaining rollback and leaving MCP servers unchanged. This supersedes the preparation-only restriction for this specific activation, not for future deployments or publication.

Active dist now contains the tested source6c540d3236104c5f1f50747013a69e4c8efa0674, executable SHA256b96804f92f5ce7353092be94262b4369ac2603e218155c4af8759fb8bf2a9428. A clean-source build matched the natively tested candidate byte for byte. The old app closed gracefully; one new app with its embedded read-only worker received fresh validated live data. Read-only actual-window queries confirmed a visible ordinary HUD immediately above the game, with no global topmost flag. STABLE is the waiting-phase display label. Real two-monitor visual acceptance remains pending player feedback; hosted CI for6c540d3 was still in progress at activation.

Rollback runtime and its checksum record are in ignored local/backups/before-stacking-activation-20261024T194604Z/rollback-dist; retired-dist retains the original directory as well. Its executable SHA256 is3320cbb7f2db8f9b6f75530fc6f0ad2b27c19da5f236a639df006fd4b3f3df60. Private startup/window-query logs are under local/run-stacking-20261024. Exit the current app before any explicitly requested rollback; preserve these copies. MCP servers, registrations and the planning workspace were not changed. No public release, version tag or visibility change occurred.

## Focus-return delay candidate — 2026-10-24

Player confirmed the two-monitor covering behavior works. They clarified the remaining issue is delayed reappearance (about half a second) when returning to the game, not repeated flickering. Added a queued Windows foreground/restore/reorder notification path, out of context and without injection, that updates only stacking and skips reader locking/full timer work. The100ms timer remains a fallback. No memory-reader, timer evidence or MCP change. The running source6c540d3 is preserved for this candidate's separate manual latency check; no second switch has been performed.

Native diagnostic:20 notification round trips, zero timer messages, reader mutex deliberately held, maximum observed recovery1028 microseconds initially,662 microseconds on the final verification, focus unchanged and teardown confirmed. This is synthetic off-screen scheduling evidence, not measured real-game presentation latency. Ordinary stacking/native-window/render regressions and strict workspace checks must remain passing. Backup source: local/backups/before-focus-notifications-20261024. Candidate activation and perceived-latency confirmation remain pending.

## Open work — updated 2026-10-24

- Finish the real-game focus-return latency and displayed wave-cycle checks; the updated candidate is now activated and awaiting player feedback.
- Requested app/tray icon: design an original icon consistent with the HUD, embed it in the Windows executable (Explorer/window/taskbar) and use a legible small-size version in the system tray. Retain editable source and a multi-resolution ICO; verify small sizes, high DPI and light/dark taskbars. No extracted game artwork. Status: requested/to do; no icon design or implementation started.


## Phase display update — 2026-10-24

Owner requested APPROACHING instead of the Moving display label, then INCINERATING during the first seven simulation seconds of native Fadeout and COOLING afterward. Implemented in presentation only: native phase identities, reader, timing calculation and evidence remain unchanged. Early Fadeout uses the existing red; cooling uses the existing orange. Its countdown targets the seven-second display boundary first, then the native phase end. Presenter debounce resets at the display boundary. Unknown/invalid substage timing retains FADEOUT with an unavailable countdown, never a wall-time guess.

All30 Rust tests, strict Clippy, formatting, locked release build, vendor/font checks and disposable package/install/rollback regressions pass. Eight off-screen native panels (including both Fadeout labels and unknown timing), hidden window, stacking and queued-event diagnostics pass; rendered labels fit the approved frame and unchanged stages/timer retain their glyph counts. This is synthetic boundary/render verification, not a live rupture-cycle observation. Source backup: local/backups/before-stage-labels-20261024. Candidate: local/stage-labels-final-20261024. No real installation or running app was replaced; active source6c540d3 and its rollback remain preserved. Candidate includes the previously pending focus-return notification change. MCP and planning workspaces remain unchanged. No release/tag/visibility change. Manual activation and real-game cycle/focus confirmation remain pending.


## Authorized phase-label and notification activation — 2026-10-24

Owner explicitly requested switching the running overlay for testing. Active dist now runs source a718afca53eb00c9d5a1526b5538836e25b5c965, executable SHA256 29105e06bd4b55d53eef300e04995b465c82f79bc61e28d63ca793d814ada317. A clean committed-source build matched the previously tested candidate byte for byte. Its finite live preflight received24 world readings and93 menu readings in one session, with zero child processes. The previous app closed gracefully and joined its reader; one new visible ordinary (not globally topmost) overlay received its first fresh validated live observation, with queued OS notifications active. This activates both the phase-label changes and the previously pending focus-return notification improvement.

Rollback: ignored local/backups/before-stage-activation-20261024T205507Z/rollback-dist, executable SHA256 b96804f92f5ce7353092be94262b4369ac2603e218155c4af8759fb8bf2a9428. The original directory is also retained as retired-dist with a private activation journal. New run logs and window validation are in local/run-stage-labels-20261024. Real-game visual acceptance of the APPROACHING / seven-second INCINERATING / COOLING sequence and perceived focus-return latency remain for the owner to confirm. MCP servers, registrations and standalone settings were unchanged. No public release, tag or visibility change.
