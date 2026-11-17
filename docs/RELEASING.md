# Release process

The maintainer prepares releases; pull requests are not accepted. Releases use reviewed version tags and a manually dispatched draft workflow. Repository visibility changes and final publication are separate, explicitly authorized operations.

Routine Windows CI checks development builds. Run **Windows package verification** on the candidate revision before preparing a release. This workflow builds and tests a release package and retains its ZIP and checksum for two days. It also runs automatically for changes to dependencies, build and packaging scripts, workflows, assets and licence files. The tagged draft workflow below independently builds and verifies the release source.

## Candidate checks

1. Update CHANGELOG.md and the workspace version shared by all application crates. Preserve the observation schema and contract identifiers unless the observation format changes deliberately.
2. Use a clean checkout with the pinned Rust toolchain. Run Install-QualityTools.ps1, Test-Dependencies.ps1 and Test-PublicContent.ps1.
3. Run Build.ps1 into a new output folder. This checks formatting, Clippy with warnings denied, all first-party tests and vendor hashes before the release build.
4. Run Package-Release.ps1 and Test-Release.ps1. Verify the executable, eight native stage renders (including both FADEOUT display substages and unknown timing), native icon extraction/loading/window assignment, dialog defaults/Save/Cancel/error handling, licence inventory, extraction guards and install/upgrade/rollback regressions.
5. Run the candidate's bounded `--self-test-reader ABSOLUTE-PRIVATE-DIRECTORY` diagnostic against the supported running game. Verify reader, timing and compatibility changes through the relevant live checks. Summarize the supported game build, live checks and untested conditions in the release notes, with links to the candidate's CI results. Keep captures and caches private.
6. Review public content and all Git history again, including any newly added binary assets. Review the documented maintenance exception in deny.toml. Keep unsigned distribution and its SmartScreen guidance explicit. Verify offline first-run extraction, Rust licence notices, an empty asset cache, cache reuse and setup cancellation before publication.

The build uses locked dependencies, static Visual C++ runtime linkage, thin LTO, one code-generation unit and stripped symbols. It remaps source and cache paths. ZIP entry ordering and timestamps are stable; compiler output can vary across SDK and toolchain environments.

## Public-repository checks

Review the current tracked files and every reachable branch and tag before making the repository public. Keep account paths, credentials, settings, captures, saves, extracted game assets and raw verification output in ignored local storage. Check Git author/committer email addresses as well as file contents. Preserve history; any necessary history rewrite needs separate review and authorization.

Check the retained-source inventory, licence declarations, notice export, bundled font hashes and artwork permission. Keep source and asset provenance in [PROVENANCE.md](PROVENANCE.md). The app ZIP contains the executable, its player README, licence notices and dependency/file inventories; maintenance guides and local audit reports do not belong in it.

The public README must describe the download that actually exists. Check the final committed source through both Windows CI and Windows package verification, and include their results with the release-specific live checks. Keep raw test logs and acceptance notes out of the public documentation.

Check main-branch protection and private vulnerability reporting through the authenticated repository settings. Protect against unintended force pushes and deletion; select any required status checks to fit the owner's maintenance workflow. Do not require external pull requests as a release prerequisite. Protection availability depends on the repository plan and visibility; see [GitHub protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches).

GitHub private vulnerability reporting is available for public repositories; see [GitHub's configuration guide](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository). Where the current plan prevents protection of a private repository, apply and verify protection and private reporting immediately after an explicitly authorized visibility change, before announcing the repository or publishing an app download. Until private reporting is enabled, SECURITY.md describes the fallback contact process. Repository visibility and settings changes remain separate authorized actions.

## Draft preparation

After reviewing and committing the candidate, create an annotated version tag on that reviewed commit and push it. Tags must exist before dispatch.

| Release channel | Dispatch branch | Tag format |
| --- | --- | --- |
| Stable | main | vMAJOR.MINOR.PATCH |
| Update 3 preview | update3 | vMAJOR.MINOR.PATCH-update3.N |

Manually dispatch **Prepare draft release** from the appropriate branch with the existing tag and the live-verification checkbox. The workflow rejects missing tags, tags outside that branch's history and mismatched app versions. It builds and tests the tagged source and creates an **unpublished draft**. Update 3 drafts are marked **Pre-release** and cannot replace the latest stable release. The branch and tag rules are covered by the offline Test-ReleasePolicy.ps1 check.

Repeated creation fails when a release already exists. The ordinary CI workflow has read-only repository permissions; only the final draft job gets contents:write.

Review the downloaded archive, checksums, release notes and supported public game version. Include the live checks actually performed and any remaining limitations. Publishing the draft requires explicit owner approval; draft preparation never publishes it.

## Workflow security

GitHub recommends full commit pins for third-party actions; the checkout/upload/download steps use reviewed official action SHAs. See [GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use). Inputs are passed through environment variables and version-validated before shell use. PR checks use read-only repository permissions.

GitHub's draft flag is an explicit unpublished state; see the [release API](https://docs.github.com/en/rest/releases/releases#create-a-release). Candidate executables are unsigned.
