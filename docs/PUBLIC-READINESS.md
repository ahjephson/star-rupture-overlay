# Public-release readiness

Status: prepared and verified locally and on a fresh Windows CI runner; publication requires explicit owner approval. Last updated2026-10-24. The repository remains private; no release, tag, deployment or visibility change was performed.

## Focus-return follow-up — 2026-10-24

The player accepted the covering behavior and reported delayed reappearance (about half a second) on focus return. A separate candidate now subscribes to queued Windows notifications outside the game process and repairs only stacking, without reader locking or a timer wait. All28 local Rust tests and native window/stacking/render/event diagnostics pass. The event diagnostic performed20 repairs with the reader lock held and zero timer messages; maximum observed native recovery was662 microseconds. This does not measure real-game presentation latency. The running6c540d3 build remains unchanged while the candidate awaits a manual activation/check.

## Earlier stacking candidate — 2026-10-24

Game-relative ordinary stacking and the STABLE display label have since been implemented and locally verified. All27 Rust tests and native stacking/window/six-stage render checks pass. The new code's hosted run is separate from the earlier successful preparation run below. The running installation remains unchanged; real two-monitor appearance awaits an explicitly authorized switch. Native tests use only owned off-screen fixtures, never the player's game/browser windows. See ARCHITECTURE.md and validation.json.

## Prepared

- First-party code consistently formatted and strict Clippy clean; observation validation/ownership documented and source-include shim removed.
- Corrupt/oversized local caches bounded before allocation/parsing, with a regression test.
- Rust toolchain, locked dependencies, vendor/font hashes and official Action SHAs pinned.
- Known dependency advisories fixed or removed; one explicit unmaintained compile-time macro exception retained for per-release review.
- Owner-selected PolyForm Strict1.0.0, copyright (c)2026 Alex Jephson, applied to first-party metadata and docs. Dependency/font licences remain unchanged; actual notice texts and exact-revision fallback attributions are packaged.
- Portable Windows x64 package, static VC runtime, file manifest, archive checksum and stable ZIP entry timestamps.
- Verified install/upgrade/rollback and rejection of path traversal, rooted entries, case duplicates, extra files, tampered executable and bad archive checksum.
- Read-only Windows CI plus manually dispatched draft-only release process. No workflow can automatically publish or change repository visibility.
- Current public docs contain aggregate validation, not personal process/session details. Earlier private notes remain recoverable locally.

## Evidence and limits

27 Rust tests across5 targets, warnings-denied Clippy, native hidden-window checks, all six native stage renders, read-only live sampling and disposable deployment regressions passed locally. Live sample:24 world observations,93 menu reads, one session, zero child processes. No installed app or MCP server was replaced. Native import inspection confirms GUI/x64 and no separate VC runtime dependency. See validation.json for aggregate records; full logs/captures remain ignored locally.

The targeted review inspected174 historical text blobs across all six pre-preparation commits. No Windows account path or tested credential/private-key format matched. Tracked-content/font checks cover the new tree. These are bounded checks, not an exhaustive guarantee against all sensitive information. History was preserved; old development prose contains historical names/session identifiers, without distributed saves or memory dumps. Re-review before the actual visibility change.

## Hosted verification

[Windows CI run40124395614](https://github.com/ahjephson/star-rupture-overlay/actions/runs/40124395614) passed for source `b227eabd1263c4f819e40e54b8506b5daa675a5e`. This includes the complete build, package, native rendering and disposable installer regressions, with private artifact retention. No game installation, release record, version tag or visibility change was made. Fresh Git checkout checks also preserved all100 vendor hashes, font provenance and first-party formatting.

One nonblocking hosted annotation remains: the pinned official v4 actions run through GitHub's Node24 compatibility path and emit a Node20 deprecation notice. Dependency-update proposals for newer action majors are open for review and have not been merged automatically. This concerns hosted tooling only; the app remains independent of Node. Review those proposals before a later workflow refresh.

## Owner decisions before public launch

- Confirm final publication and repository visibility explicitly.
- Review the tested supported game hash and default C-drive installation restriction; other installations and game builds are unsupported.
- Decide whether the initial unsigned release is acceptable or establish a code-signing/provenance identity. Checksums establish integrity, not publisher authenticity.
- Configure branch protection, appropriate review requirements and private vulnerability reporting. No repository-security settings were changed during preparation.
- Run release-specific live checks for the final tagged build and review the documented paste maintenance exception.
- Review the PolyForm Strict terms as the selected source-available licence. Historical commits retain their earlier licence declarations; history has not been rewritten.

The draft-release workflow itself is intentionally not dispatched in this preparation: doing so would create a release record. Its syntax is checked with the official actionlint tool; candidate building/packaging/deployment are tested independently.
