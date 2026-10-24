# StarRupture Wave Overlay handover

## Small stage headroom adjustment — 2026-10-24

Alex accepted the new game-style frame and requested a slightly taller box with a little more gap above the stage. Added4 pixels to the panel height and shifted the existing content down4 pixels; fonts, text allocation and internal line spacing keep their previous dimensions. At the current display the footprint is223x89 instead of223x85. Upper-left anchor and pause-border gap remain unchanged. Native six-stage renders preserve every title/countdown pixel count; PRE-WAVE title bounds were verified unchanged except for a4px vertical offset.

All25 tests/5 targets, warnings-denied locked/offline release build and hidden native window/layer/focus/lifecycle checks pass. Read-only Stop/EOF validation repeated successfully. Previous source/runtime is backed up under local/backups/before-stage-top-padding-20261024. Old PID34712 closed gracefully; current overlay PID44576/session67278, private run local/run-stage-padding-20261024, one reader and first validated live data confirmed. No MCP or planning files changed. This supersedes the prior223x85 height only.

## Game-style frame applied — 2026-10-24

Alex requested a box resembling the supplied in-game Multiplayer settings panel after separation. Replaced the all-corners chamfered, stage-colored contour with a raised upper-left tab, diagonal top step, thin pale neutral outline, square left corners and one clipped bottom-right corner. Added a thin header divider. The renderer, per-pixel alpha mask and native window region use the same panel geometry. Stage colors remain on the title; Oxanium text, natural countdown proportions, alpha230 background, footprint, automatic play/pause position and read-only timer/reader behavior remain unchanged.

Backed up source/docs and both runtime executables under ignored local/backups/before-game-panel-frame-20261024. Updated the existing silhouette/alpha test; all25 workspace tests across5 targets pass, warnings denied/locked/offline. Release build, all six native stage renders and hidden window/layering/hit-test/focus/cleanup tests pass. Inspected the actual native WAITING render; no generated replacement fonts/assets. Read-only protocol Stop/EOF checks repeated successfully (93 menu reads/24 world reads over6sec, shutdown within15ms).

Previous overlay PID35272 closed gracefully, then new dist binary launched as PID34712, session56733. New private run local/run-game-panel-20261024. One owned reader and first validated live observation verified; no MCP/planning source or active-server changes. User appearance acceptance pending. Header/contour change is intentionally confined to this independent project. GitHub main receives the reviewed change; do not copy it back into the MCP workspace.

Status: independently extracted project,2026-10-24. The user accepted the persistent-reader responsiveness and requested a separate folder, GitHub repository and Codex project.

## Independent-project validation

25 tests across5 targets passed with warnings denied; release build passed. Live reader:93 menu checks/24 world observations over6sec, Stop/EOF clean within15ms. Hidden native window/layering/styles/hit-tests/focus/destruction passed. Reader files, pure world validation and timing code match the accepted source. Runtime now starts from this project's dist directory (overlay PID35272, launch session36475); the original runtime was closed gracefully and remains available for rollback. Validation details in docs/validation.json. Build dependencies used the existing development Cargo cache for this one offline verification; that cache is not a source/runtime dependency.

Private GitHub repository:https://github.com/ahjephson/star-rupture-overlay. Initial source commit 540638b31cf3d68a84b2e5844583d5e2437d3891 pushed and verified on main. Codex app addition is the only manual setup step: Add project and choose this folder; current tools cannot create a local project, and the Windows automation skill forbids Codex UI automation. No task was created or project falsely marked added.

## Preserve

Runtime consists of starrupture-wave-overlay.exe and one dedicated SHA-pinned starrupture-overlay-reader.exe plus private configuration/cache. No MCP dependency or registration. Read-only query/read process handle only; no injection, hooks or game writes. Unknown build/session data fails closed. Keep process creation/build, registry serial/world/player/subsystem guards, stream sequence/generation/freshness limits, clean child lifetime and no fallback to saves.

Current supported game executable SHA256:95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1. The reader currently verifies the standard Steam installation path; alternative installation paths remain unsupported until explicitly designed and tested. Application rejects elevated execution.

Pause checks target50ms, full world/timer observations250ms. Schedules are serialized; world read latency can defer pause checks. Last source-project live test:93 menu reads/24 world observations over about6sec; Stop/EOF exited0 within16ms. Parent eight-second test: same single child throughout, validated data, clean child reap. Source workspace161 tests/25 targets and native window tests passed. These are historical source validation, not a substitute for this extracted project's validation.

Approved display: Oxanium, natural digit proportions, stage-specific colors (Moving displayed INCINERATING), background alpha230, text/border255, clear chamfers. At2560x1440 play position left8/top8; paused left8/top88. Manual tray movement overrides until Reset. Visible without game focus; hides for minimized/exited game or explicit Hide. Fast menu state controls position only. Full timer observation timestamps remain independent and stale after5sec. Floor fractional seconds, retain isolated upward-second debounce, no wall-clock extrapolation.

## Separation

All required code and vendored source are local. A stale inherited gospel fingerprint was resolved in the new manifest after proving all19 changes exactly reproduce rustfmt output of the recorded upstream revision; dependency source bytes remain unchanged and original hashes are retained. The full MCP core was reduced to pure wave validation/timing and overlay configuration; unrelated server, save/rename controls, building tools, datasets and planning records are absent. The legacy module name saves in wave-core contains time/number formatting only, no file access. Reader-private inherited finite support remains internal and has no separate executable/CLI exposed. Dependencies are independent copies; future edits do not synchronize back automatically.

Original workspace and working runtime/backups are retained for rollback. Do not alter active MCP files or registrations. Personal captures/configuration/binaries/cache live only in ignored local/dist/.tools paths. Default GitHub visibility is private. Available upstream notices and central Apache-2.0 text are retained; see THIRD-PARTY-NOTICES.md.

## Validation and next work

Run scripts/Build.ps1 then the native --self-test-window check. Run bounded persistent protocol and parent lifecycle tests against the game before replacing a visible runtime. Never launch duplicate visible overlays. Record exact results in docs/validation.json. The Codex app project must point to this folder and must not inherit the gameplay workspace's MCP registrations.
