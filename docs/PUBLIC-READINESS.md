# Public release readiness

Status: 0.2.7 development candidate, prepared for owner review on 2026-10-27. The repository is private and no download has been published. Changing visibility, creating tags and publishing a release remain separate actions.

## Repository presentation

- [x] README focuses on requirements, starting the app, recipe controls, settings, updates and help.
- [x] Build and maintenance instructions live in a separate guide.
- [x] Feedback policy and PR template state that pull requests and code contributions are not accepted.
- [x] Bug and feature issue forms request useful player-facing information and keep private game data out of reports.
- [x] Unsigned downloads and SmartScreen are explained without presenting checksums as publisher signatures.
- [x] Recipe assets are prepared by the app from the installed game; consumers do not need extraction tools or a manually copied pack.

## Local verification

The candidate is checked with strict Clippy, locked workspace tests, vendor/font/icon checks, dependency policy and portable package/deployment regressions. First-run setup additionally needs an empty cache, validated cache reuse, changed/corrupt cache rejection, disabled-tracker behaviour and complete setup-process cancellation checks. Finite native diagnostics do not replace manual first-launch, focus and shortcut acceptance.

Tracked content and reachable Git history have been scanned for private/generated paths, account paths and common secret patterns. The review excludes ignored caches, captures, settings, game assets and binaries from publication. A pattern scan cannot guarantee the absence of every secret or establish asset rights; keep the final content/provenance review in the release procedure.

## Before making the repository public

- [ ] Review the final documentation and source diff, licence and retained artwork/font provenance.
- [ ] Decide and verify the owner's branch-protection settings and enable a usable private security-reporting route. This project accepts issue reports, not external code contributions.
- [ ] Recheck tracked content and history on the exact revision selected for publication.
- [ ] Confirm the README's download status matches the release actually available to users.

## Before publishing an app download

- [ ] Complete manual acceptance of automatic first-run preparation in the actual app, including Settings progress, retry, disable/re-enable and exit during setup.
- [ ] Verify the exact release on the supported game build, expected display/DPI configurations, phase transitions, pause and focus changes. Existing tests cover their documented scope only.
- [ ] Verify startup, desktop shortcut, game-exit choices, upgrades and alternate installation locations on the selected release.
- [ ] Review the pinned helper's managed/native dependency set separately from Cargo's advisory policy, and recheck the documented Rust maintenance exception.
- [ ] Run Windows CI and package verification on the selected committed revision. Record relevant live results and limitations in release notes.
- [ ] Review the ZIP, checksums, first-run download sources/notices and compatibility statement, then explicitly authorize the tag/release publication.

Code signing is not part of the current distribution plan. The app remains unsigned, with SmartScreen guidance and integrity checks. An installer is optional; game discovery and asset preparation belong to the app so portable users receive the same setup.

See [releasing](RELEASING.md), [compatibility](COMPATIBILITY.md), [security reporting](../SECURITY.md) and [third-party notices](../THIRD-PARTY-NOTICES.md). Keep raw validation output and activation records local.
