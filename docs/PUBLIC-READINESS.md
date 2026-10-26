# Release readiness

Version 0.1.0 is a development version. This checklist describes the remaining release work. Automated check results are available in CI; release-specific live checks belong in the release notes.

## Existing release infrastructure

- Pinned Rust toolchain, locked registry dependencies, strict Clippy and first-party formatting checks.
- Bounded read-only observations, session/allocation guards and stale-data handling.
- Hash-verified vendor source, fonts and icons, with dependency licence inventories.
- Portable Windows x64 packaging with a static VC runtime, file manifest and archive checksum.
- Native window, rendering, stacking, notification and icon diagnostics.
- Disposable installation, upgrade and rollback tests, including malformed-archive rejection.
- Read-only pull-request CI and a manually dispatched workflow for unpublished release drafts.

## Distribution work

- [ ] Resolve the default C-drive Steam installation restriction before general distribution. Installation discovery is a separate functional change and must preserve executable-hash and process-identity validation.
- [ ] Verify the release against the supported game build and document the supported map/display modes. A game update requires a compatibility review.
- [ ] Complete live reader and lifecycle checks for the release candidate. Check phase boundaries, pause layout, focus return and relevant DPI and display configurations in the running game.
- [ ] Review signing and publisher provenance. Current executables are unsigned; hashes establish byte integrity only.
- [ ] Configure and verify repository protection, review requirements and private vulnerability reporting.
- [ ] Review the dependency policy and its named maintenance exception using current advisory data.
- [ ] Review the source-available licence and all redistribution notices for the release archive.
- [ ] Review tracked content and Git history before publication, alongside the automated secret-pattern checks.
- [ ] Verify the final reviewed revision in Windows CI and run the complete build/package checks in [RELEASING.md](RELEASING.md).
- [ ] Review the draft archive, release notes, compatibility statement and checksums before explicitly authorizing publication and any repository visibility change.

## Check results

PR descriptions summarize checks and limitations and link relevant CI runs. Release notes identify the supported game build and summarize live reader and appearance checks for the release candidate. Distinguish automated results from manual checks and state any conditions that remain untested.

Private logs, captures, installation records and backups remain outside source control and release archives. Preserve repository history when editing current documentation.
