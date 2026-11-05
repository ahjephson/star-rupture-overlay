# StarRupture Overlay

A Windows desktop HUD written in Rust that displays StarRupture's rupture countdown and pinned recipe research progress.

Read README.md, docs/ARCHITECTURE.md and docs/BUILDING.md before changes.

## Implementation

- Use the local [Rust skills](.agents/skills/rust-skills/SKILL.md) when writing, reviewing or refactoring first-party Rust. Read the relevant rule files as needed. Apply the guidance within this project's read-only game-access, resource-lifetime, toolchain and architecture requirements.

- Use query/read process permissions exclusively and run with standard user privileges. Keep game memory and save files unchanged.
- Keep memory reads on the embedded feature workers and rendering in the application's own native window. Preserve bounded observation envelopes, sequence/process/build/generation/freshness guards, per-read cancellation/time budgets and joined shutdown.
- Fast menu observations control position only. Timer observations independently establish simulation time and pause evidence. Advance countdowns only from validated simulation observations.
- Preserve unavailable states, native timing limits, the Oxanium design, stage colours and labels, background transparency, digit proportions and responsive play/menu layout.
- Preserve configuration acceptance, serialized fields, numeric behaviour, read ordering and resource lifetimes during maintenance. Scope functional changes explicitly.
- Confine project changes and build/runtime output to this repository or an explicitly selected external toolchain.
- Use synthetic test data in source control. Keep caches, captures, settings, saves, generated configuration and binaries in ignored local storage.
- Preserve vendored source, licence notices and asset hashes. Format first-party Rust with scripts/Format.ps1.

## Discovery and efficient runtime access

- Broad discovery scans are research and recovery tools, not the finished runtime access path. Once discovery establishes how to reach a value, replace the exploratory scan with an efficient, validated lookup before considering the feature complete.
- Use owned player/controller/world references, bounded class/property lookups and validated cached layouts. Known-build startup, menu reopening, shortcuts and regular polling must not repeatedly enumerate the full object registry, walk unrelated objects or rescan executable images to retrieve already understood data.
- Resolve common metadata once and share immutable results across feature workers. Do not duplicate discovery scans or large caches when adding a worker. Keep feature-specific buffers and mutable observations local to their worker.
- Scope cached layouts and lookup hints to the appropriate executable/build, process and world identities. Revalidate ownership, allocation/serial identities, field bounds and freshness on use; never reuse raw process addresses across launches or remove integrity checks to improve speed.
- Allow a broad scan only in explicit diagnostics or bounded recovery when a validated direct lookup or cache cannot be used. Give recovery a clear trigger, retry/time/memory limits and diagnostic visibility. Never silently repeat it on each startup of an already understood build, UI action or poll. Document any retained fallback and its justification.
- Verify discovery costs as part of acceptance: record scan counts and startup timings for cold and warm caches, confirm normal lookup/polling paths perform no broad scans, and exercise invalidation and recovery. A successful prototype or parallel initialization does not excuse leaving exploratory scanning in the normal path.

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
