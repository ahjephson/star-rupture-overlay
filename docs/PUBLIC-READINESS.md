# Public release readiness

Status: version 0.2.7 in development. The repository is private and no download has been published. Changing visibility, creating tags and publishing a release remain separate actions.

## Repository presentation

- [x] README focuses on requirements, starting the app, recipe controls, settings, updates and help.
- [x] Build and maintenance instructions live in a separate guide.
- [x] Feedback policy and PR template state that pull requests and code contributions are not accepted.
- [x] Bug and feature issue forms request useful player-facing information and keep private game data out of reports.
- [x] Unsigned downloads and SmartScreen are explained without presenting checksums as publisher signatures.
- [x] Recipe assets are prepared by the app from the installed game; consumers do not need extraction tools or a manually copied pack.

## Local verification

The current build passes strict Clippy, locked workspace tests, vendor/font/icon checks, dependency policy and portable package/deployment regressions. Recipe checks cover cold-cache preparation, cache reuse, changed/corrupt cache rejection, disabled tracking, setup-process cancellation and native rendering. The owner accepts first-run asset preparation, recipe tracking and automatic panel closing on completion in the running build. Memory checks verify reduced cold-start peaks and regular image-cache use; completed recipes close the panel in time for the game unlock notification. The release checklist below covers the remaining manual scenarios.

The public-content check scans tracked and new source for private/generated paths and common secret patterns, and verifies bundled font/icon hashes. Release review also covers reachable Git history. Ignored caches, captures, settings, game assets and binaries stay outside publication. A pattern scan cannot guarantee the absence of every secret or establish asset rights; keep the final content/provenance review in the release procedure.

## Before making the repository public

- [ ] Review the final documentation and source diff, licence and retained artwork/font provenance.
- [ ] Decide and verify the owner's branch-protection settings and enable a usable private security-reporting route. This project accepts issue reports, not external code contributions.
- [ ] Recheck tracked content and history on the exact revision selected for publication.
- [ ] Confirm the README's download status matches the release actually available to users.

## Before publishing an app download

- [x] Manual acceptance of first-run asset preparation and recipe tracking in the current running build.
- [ ] Verify Settings progress, retry, disable/re-enable and exit during setup through the normal UI.
- [ ] Verify the exact release on the supported game build, expected display/DPI configurations, phase transitions, pause and focus changes. Existing tests cover their documented scope only.
- [ ] Verify startup, desktop shortcut, game-exit choices, upgrades and alternate installation locations on the selected release.
- [ ] Review the Rust extraction dependency graph, retained-source licences and documented maintenance exception.
- [ ] Run Windows CI and package verification on the selected committed revision. Record relevant live results and limitations in release notes.
- [ ] Review the ZIP, checksums, offline setup, dependency notices and compatibility statement, then explicitly authorize the tag/release publication.

The app is distributed as an unsigned portable ZIP, with SmartScreen guidance and integrity checks. Game discovery and offline asset preparation are built into the app.

See [releasing](RELEASING.md), [compatibility](COMPATIBILITY.md), [security reporting](../SECURITY.md) and [third-party notices](../THIRD-PARTY-NOTICES.md). Keep raw validation output and activation records local.
