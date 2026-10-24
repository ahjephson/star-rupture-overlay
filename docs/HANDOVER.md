# StarRupture Wave Overlay handover

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
