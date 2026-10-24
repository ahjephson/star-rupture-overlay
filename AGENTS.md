# StarRupture Wave Overlay

This is an independent Windows Rust desktop overlay, not an MCP server or a gameplay planning workspace.
Read README.md and docs/HANDOVER.md before changes. Keep all build/runtime paths inside this project or an explicitly selected external toolchain. No Node, Codex or MCP dependency at runtime.

- Read-only process access only. Never write game memory, inject code, hook DirectX, alter saves or request debug privileges/elevation for the application.
- User approved single-process integration on2026-10-24: keep memory reads on the embedded worker thread. Preserve bounded observation envelopes, sequence/process/build/generation/freshness guards, per-read cancellation/time budgets and joined shutdown. No helper executable or arbitrary external process launch.
- Fast menu readings change position only. They must never refresh timer evidence or establish simulation pause. Preserve unavailable states, native timing limits and no wall-time extrapolation.
- Preserve the approved Oxanium design, stage colors and approved phase display labels, slightly transparent background, natural digit proportions and responsive play/pause layout.
- The MCP/planning repositories are references only. Do not change their servers, registrations, helpers, configuration, datasets or journals.
- Keep local caches, captured memory, personal paths/settings, saves, generated configuration and binaries out of Git. Synthetic tests only in source control. Preserve third-party source and license notices.
- Run warnings-denied locked workspace tests and a release build. Test worker cancellation/cleanup and live observations before replacing the running overlay. Avoid duplicate visible overlays.
- Update docs/HANDOVER.md and validation records with actual evidence and limitations. Record intentional forks from the source project; no implicit shared paths or synchronized edits.

## Public release preparation

- Keep the repository private until the owner explicitly requests a visibility change. Never create a tag, publish a release or deploy a candidate as a side effect of maintenance.
- Read docs/RELEASING.md and docs/PUBLIC-READINESS.md. Build into a fresh ignored output directory; never overwrite a running installation.
- Use the pinned toolchain, scripts/Format.ps1 for first-party formatting, strict Clippy/tests, dependency policy, vendor/font checks and package/deployment tests. Do not reformat vendor code.
- The release workflow is manually dispatched and creates unpublished drafts only. Live game validation is separate from hosted CI. Do not assert hosted or live checks passed without actual evidence.
- Keep personal development notes and capture details in ignored local backups; public validation records contain aggregate results only. Review history before publication and never silently rewrite it.
