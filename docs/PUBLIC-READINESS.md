# Public-release readiness

Status: prepared locally, awaiting hosted CI verification and explicit owner approval for publication. Last updated2026-10-24. The repository remains private; no release, tag, deployment or visibility change was performed.

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

## Owner decisions before public launch

- Confirm final publication and repository visibility explicitly.
- Review the tested supported game hash and default C-drive installation restriction; other installations and game builds are unsupported.
- Decide whether the initial unsigned release is acceptable or establish a code-signing/provenance identity. Checksums establish integrity, not publisher authenticity.
- Configure branch protection, appropriate review requirements and private vulnerability reporting. No repository-security settings were changed during preparation.
- Run release-specific live checks for the final tagged build and review the documented paste maintenance exception.
- Review the PolyForm Strict terms as the selected source-available licence. Historical commits retain their earlier licence declarations; history has not been rewritten.

The draft-release workflow itself is intentionally not dispatched in this preparation: doing so would create a release record. Its syntax is checked with the official actionlint tool; candidate building/packaging/deployment are tested independently.
