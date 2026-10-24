# Maintainer handover

Status: public-release preparation in progress; repository remains private. No release or visibility change is authorized by this preparation.

Read README.md, docs/ARCHITECTURE.md, docs/COMPATIBILITY.md and docs/RELEASING.md before changes. docs/validation.json records the latest completed checks. Historic session-specific development notes and the pre-preparation Git bundle are retained only in ignored local backups. Existing Git history is preserved; a targeted historical secret/account-path review found no matching formats, with limitations recorded in docs/PUBLIC-READINESS.md.

## Preserve

One Windows GUI app owns the native HUD and embedded read-only worker. Never add memory writes, injection, game hooks, save access, debug privileges or an arbitrary helper executable. Preserve exact game-build/path pins, object/world/allocation guards, bounded observations and cooperative cancellation. Fast menu evidence controls layout only; timer evidence and simulation pause remain independent.

Keep the approved Oxanium frame, stage colors, INCINERATING label, slight background transparency and play/pause positioning. Do not deploy a candidate over a running app. Build into a fresh ignored output folder; Deploy.ps1 is a separate explicit operation.

## Current decisions

Rust1.99.1, locked registry dependencies and immutable action pins are used for reviewed Windows builds. Source is formatted separately from vendor code. Vendor manifest-only safety/publish patches are documented and hash-verified. The source-include shim was replaced with a normal reader library; process handles stay on its worker thread. Local caches are bounded before parsing. The release executable statically links the VC runtime and uses thin LTO/stripped symbols.

No signing identity, public visibility change, version tag or release publication has been configured or performed. The release workflow creates drafts only after manual dispatch and confirmed live verification. Signing/provenance and repository protection settings must be reviewed before the first public launch.

This project is independent. Do not modify any planning workspace, MCP server, registrations, controller journals, datasets or old server copies while maintaining it. Local caches, builds, captures and private notes stay outside Git.
