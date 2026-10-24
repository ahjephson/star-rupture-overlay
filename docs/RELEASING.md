# Release process

Nothing in this preparation makes the repository public or publishes a release. Visibility changes and final publication require the owner's explicit approval.

## Candidate checks

1. Update CHANGELOG.md and the app workspace version. Preserve the reader's internal schema version unless its validator changes deliberately.
2. Use a clean checkout with the pinned Rust toolchain. Run Install-QualityTools.ps1, Test-Dependencies.ps1 and Test-PublicContent.ps1.
3. Run Build.ps1 into a new output folder. This checks formatting, Clippy with warnings denied, all first-party tests and vendor hashes before the release build.
4. Run Package-Release.ps1 and Test-Release.ps1. Verify the executable, eight native stage renders (including both FADEOUT display substages and unknown timing), native icon extraction/loading/window assignment, licence inventory, extraction guards and install/upgrade/rollback regressions.
5. Check a candidate against the supported game using its bounded --self-test-reader diagnostic. Reader/timing/compatibility changes need appropriate live verification. Keep all captures and caches private. CI cannot verify a running proprietary game.
6. Review public content and all Git history again, including any newly added binary assets. Review the documented maintenance exception in deny.toml. Decide whether unsigned distribution is acceptable or establish publisher signing/provenance before publication.

Build flags retain exact locked dependencies, static VC runtime linkage, thin LTO, one code-generation unit and stripped symbols. Source/cache paths are remapped. ZIP entry ordering and timestamps are stable; bit-for-bit compiler reproducibility across SDK/toolchain environments is not claimed.

## Draft preparation

After reviewing and committing the candidate, create an annotated vMAJOR.MINOR.PATCH tag on that reviewed commit and push it. No tag is created automatically by CI. Configure repository branch protection and private vulnerability reporting before a public launch; these repository settings are not changed by local preparation.

Manually dispatch **Prepare draft release** from main with the existing tag and the live-verification checkbox. It refuses a missing/non-main tag or mismatched app version, builds and tests the tagged source, uploads the candidate and creates an **unpublished draft**. Repeated creation fails instead of overwriting assets. It cannot publish or change visibility. The ordinary CI workflow has read-only repository permissions; only the final draft job gets contents:write.

Review the downloaded archive, checksums, release notes and supported game hash. Publishing that draft and changing repository visibility are separate owner actions. Do neither until approved. No automatic tag-triggered publishing, background updater or production deployment is configured.

## Workflow security

GitHub recommends full commit pins for third-party actions; the checkout/upload/download steps use reviewed official action SHAs. See [GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use). Inputs are passed through environment variables and version-validated before shell use. PR checks receive no write token or release secret.

GitHub's draft flag is an explicit unpublished state; see the [release API](https://docs.github.com/en/rest/releases/releases#create-a-release). Optional future artifact attestations require suitable repository/plan support; see [GitHub provenance documentation](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations). No code-signing certificate or signing identity is assumed.
