# StarRupture Wave Overlay

This is an independent Windows Rust desktop overlay, not an MCP server or a gameplay planning workspace.
Read README.md and docs/HANDOVER.md before changes. Keep all build/runtime paths inside this project or an explicitly selected external toolchain. No Node, Codex or MCP dependency at runtime.

- Read-only process access only. Never write game memory, inject code, hook DirectX, alter saves or request debug privileges/elevation for the application.
- Keep the one dedicated SHA-pinned child reader; bounded IPC, sequence/process/build/generation/freshness guards and child cleanup are required.
- Fast menu readings change position only. They must never refresh timer evidence or establish simulation pause. Preserve unavailable states, native timing limits and no wall-time extrapolation.
- Preserve the approved Oxanium design, stage colors, INCINERATING label, slightly transparent background, natural digit proportions and responsive play/pause layout.
- The MCP/planning repositories are references only. Do not change their servers, registrations, helpers, configuration, datasets or journals.
- Keep local caches, captured memory, personal paths/settings, saves, generated configuration and binaries out of Git. Synthetic tests only in source control. Preserve third-party source and license notices.
- Run warnings-denied locked workspace tests and a release build. Test child Stop/EOF/cleanup and live streams before replacing the running overlay. Avoid duplicate visible overlays.
- Update docs/HANDOVER.md and validation records with actual evidence and limitations. Record intentional forks from the source project; no implicit shared paths or synchronized edits.
