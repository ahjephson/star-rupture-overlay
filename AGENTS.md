# StarRupture Overlay

A Windows desktop HUD written in Rust that displays StarRupture's rupture countdown and pinned recipe research progress.

Read README.md, docs/ARCHITECTURE.md and docs/BUILDING.md before changes.

## Implementation

- Use query/read process permissions exclusively and run with standard user privileges. Keep game memory and save files unchanged.
- Keep memory reads on the embedded worker thread and rendering in the application's own native window. Preserve bounded observation envelopes, sequence/process/build/generation/freshness guards, per-read cancellation/time budgets and joined shutdown.
- Fast menu observations control position only. Timer observations independently establish simulation time and pause evidence. Advance countdowns only from validated simulation observations.
- Preserve unavailable states, native timing limits, the Oxanium design, stage colours and labels, background transparency, digit proportions and responsive play/menu layout.
- Preserve configuration acceptance, serialized fields, numeric behaviour, read ordering and resource lifetimes during maintenance. Scope functional changes explicitly.
- Confine project changes and build/runtime output to this repository or an explicitly selected external toolchain.
- Use synthetic test data in source control. Keep caches, captures, settings, saves, generated configuration and binaries in ignored local storage.
- Preserve vendored source, licence notices and asset hashes. Format first-party Rust with scripts/Format.ps1.

## Checks and documentation

- Run warnings-denied locked workspace tests and a release build, dependency policy, vendor/font/icon checks and package/deployment tests.
- Verify worker cancellation, cleanup and live observations before replacing an installed overlay. Run one normal overlay instance at a time.
- Write for the document's audience: user documentation covers operation and requirements; architecture documentation covers implementation details needed for maintenance. Include information that supports a practical task or decision.
- Use the [writing clearly and concisely skill](https://github.com/softaworks/agent-toolkit/blob/main/skills/writing-clearly-and-concisely/SKILL.md) for documentation and help text. Describe current facts in direct, concrete language.
- Describe current behaviour without app-release labels or dated development status. Keep release history in CHANGELOG.md and dependency/toolchain versions in their authoritative manifests; retain licence identifiers and supported-game compatibility details where needed.
- Write changelog entries for players: describe features, improvements and fixes they receive. Keep implementation tasks, internal refactors and verification notes out of the changelog.
- Keep each prose paragraph on one source line and let editors/renderers wrap it. Preserve structural line breaks for headings, lists, tables and code blocks.
- Summarize checks and limitations in change summaries, link CI results and record release-specific live checks in release notes. Keep raw logs and diagnostic output local.

## Release preparation

- Read docs/RELEASING.md for release and public-repository checks.
- Build into a fresh ignored output directory and preserve the installed application and rollback copies during preparation.
- Treat deployment, version tags, release creation, publication and repository visibility changes as separate operations requiring explicit authorization. Keep the repository private until a visibility change is requested.
- The release workflow is manually dispatched and creates unpublished drafts. Report hosted and live checks only when actually performed.
- Review tracked content and history before publication. Preserve Git history.

## Public documentation and feedback

- Write README and support pages for players installing and using the app. Keep build, architecture and release procedures in the maintainer guides.
- The owner does not accept pull requests or code contributions. Use issue reports for bugs and feature requests; do not open a pull request for repository changes.
- Game discovery and recipe asset preparation are app responsibilities. Keep game assets local and outside the repository and release archives; do not ask consumers to run extraction tools or copy a development asset pack.
