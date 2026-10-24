# Maintainer handover

Status: public-release preparation verified locally and by fresh Windows CI; repository remains private. No release or visibility change is authorized by this preparation.

Read README.md, docs/ARCHITECTURE.md, docs/COMPATIBILITY.md and docs/RELEASING.md before changes. docs/validation.json records the latest completed checks. Historic session-specific development notes and the pre-preparation Git bundle are retained only in ignored local backups. Existing Git history is preserved; a targeted historical secret/account-path review found no matching formats, with limitations recorded in docs/PUBLIC-READINESS.md.

## Preserve

One Windows GUI app owns the native HUD and embedded read-only worker. Never add memory writes, injection, game hooks, save access, debug privileges or an arbitrary helper executable. Preserve exact game-build/path pins, object/world/allocation guards, bounded observations and cooperative cancellation. Fast menu evidence controls layout only; timer evidence and simulation pause remain independent.

Keep the approved Oxanium frame, stage colors, INCINERATING label, slight background transparency and play/pause positioning. Do not deploy a candidate over a running app. Build into a fresh ignored output folder; Deploy.ps1 is a separate explicit operation.

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
