# Release process

Releases use reviewed version tags and a manually dispatched draft workflow. Repository visibility changes and final publication are separate, explicitly authorized operations.

Routine Windows CI checks development builds. Run **Windows package verification** on the candidate revision before preparing a release. This workflow builds and tests a release package and retains its ZIP and checksum for two days. It also runs automatically for changes to dependencies, build and packaging scripts, workflows, assets and licence files. The tagged draft workflow below independently builds and verifies the release source.

## Candidate checks

1. Update CHANGELOG.md and the workspace version shared by all application crates. Preserve the observation schema and contract identifiers unless the observation format changes deliberately.
2. Use a clean checkout with the pinned Rust toolchain. Run Install-QualityTools.ps1, Test-Dependencies.ps1 and Test-PublicContent.ps1.
3. Run Build.ps1 into a new output folder. This checks formatting, Clippy with warnings denied, all first-party tests and vendor hashes before the release build.
4. Run Package-Release.ps1 and Test-Release.ps1. Verify the executable, eight native stage renders (including both FADEOUT display substages and unknown timing), native icon extraction/loading/window assignment, licence inventory, extraction guards and install/upgrade/rollback regressions.
5. Run the candidate's bounded `--self-test-reader ABSOLUTE-PRIVATE-DIRECTORY` diagnostic against the supported running game. Verify reader, timing and compatibility changes through the relevant live checks. Summarize the supported game build, live checks and untested conditions in the release notes, with links to the candidate's CI results. Keep captures and caches private.
6. Review public content and all Git history again, including any newly added binary assets. Review the documented maintenance exception in deny.toml. Decide whether unsigned distribution is acceptable or establish publisher signing/provenance before publication.

The build uses locked dependencies, static Visual C++ runtime linkage, thin LTO, one code-generation unit and stripped symbols. It remaps source and cache paths. ZIP entry ordering and timestamps are stable; compiler output can vary across SDK and toolchain environments.

## Draft preparation

After reviewing and committing the candidate, create an annotated vMAJOR.MINOR.PATCH tag on that reviewed commit and push it. Tags are created explicitly before dispatch. Configure repository branch protection and private vulnerability reporting before a public launch.

Manually dispatch **Prepare draft release** from main with the existing tag and the live-verification checkbox. It refuses a missing/non-main tag or mismatched app version, builds and tests the tagged source, uploads the candidate and creates an **unpublished draft**. Repeated creation fails when release assets already exist. The ordinary CI workflow has read-only repository permissions; only the final draft job gets contents:write.

Review the downloaded archive, checksums, release notes and supported game hash. Publishing the draft and changing repository visibility each require explicit owner approval.

## Workflow security

GitHub recommends full commit pins for third-party actions; the checkout/upload/download steps use reviewed official action SHAs. See [GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use). Inputs are passed through environment variables and version-validated before shell use. PR checks use read-only repository permissions.

GitHub's draft flag is an explicit unpublished state; see the [release API](https://docs.github.com/en/rest/releases/releases#create-a-release). Optional future artifact attestations require suitable repository/plan support; see [GitHub provenance documentation](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations). Candidate executables are unsigned.
