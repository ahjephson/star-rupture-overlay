# Building and maintaining the app

This guide documents the owner's build and maintenance workflow. Pull requests and code contributions are not accepted. First-party code uses [PolyForm Strict 1.0.0](../LICENSE). Read [architecture](ARCHITECTURE.md) and [compatibility](COMPATIBILITY.md) before maintenance.

## Build a local candidate

Use Windows with the Rust toolchain pinned in [rust-toolchain.toml](../rust-toolchain.toml), Visual Studio C++ Build Tools and the Windows SDK, PowerShell 7 and Git. Run commands from the repository root:

```powershell
./scripts/Install-QualityTools.ps1
./scripts/Test-Dependencies.ps1
./scripts/Test-PublicContent.ps1
./scripts/Build.ps1 -OutputDirectory local/candidate
./scripts/Package-Release.ps1 -BuildDirectory local/candidate
```

Build.ps1 checks formatting, strict Clippy, workspace tests and vendor hashes. It requires a fresh output directory and leaves the running app unchanged. Use `-Offline` with cached dependencies, or `-ToolchainDirectory PATH` for the pinned local toolchain. Packaging requires a clean checkout by default; `-AllowUncommitted` marks development archives. [Release preparation](RELEASING.md) and [deployment](DEPLOYMENT.md) describe the separate packaging and activation steps.

Recipe setup uses built-in Rust code to prepare assets offline from the supported installed game. A loaded world supplies the native type schema; the app discovers the archives and maintains its local asset cache. The recipe diagnostics below cover finite setup, cancellation, rendering and live reader checks. All generated data belongs in ignored local storage.

## Rust conventions

Follow the [Rust Style Guide](https://doc.rust-lang.org/style-guide/) and use the pinned rustfmt and Clippy versions. Keep macro bodies readable; expand large observation objects across lines. Use workspace inheritance for shared Cargo package settings.

Keep modules focused on discovery, observation validation, timing, rendering or application lifetime. Prefer explicit imports for project dependencies and the narrowest useful visibility. Win32 API imports may be grouped by platform namespace. Use ownership and RAII for resources and document the lifetime, thread and aliasing requirements of unsafe interfaces. Error handling must preserve unavailable states and existing cleanup paths.

For maintenance refactors, preserve configuration acceptance, serialized fields, numeric operations, read ordering, scheduling and native resource lifetimes. Make formatting changes separately from structural changes. Changes to compatibility, timing, UI behaviour or error handling need explicit scope and regression coverage. Preserve validation guards and observed native timing during cleanup.

## Documentation

Describe current behaviour and supported conditions for the document's audience. Explain technical constraints where they help readers operate or maintain the app. Keep each prose paragraph on one source line and preserve structural breaks for headings, lists, tables and code. Preserve source and licence provenance. Summarize checks and limitations in change summaries, link relevant CI runs, and include release-specific live checks in release notes. Keep personal records and raw diagnostics in ignored local storage.

## Local checks

Use the toolchain in rust-toolchain.toml and PowerShell 7:

```powershell
./scripts/Format.ps1
./scripts/Test.ps1
./scripts/Install-QualityTools.ps1
./scripts/Test-Dependencies.ps1
./scripts/Test-PublicContent.ps1
```

The formatter visits first-party Rust files only. Do not run a formatter over vendored dependencies. Vendor bytes and font hashes are checked separately; intentional patches require a reason and before/after hashes in third-party/VENDOR.json. Dependency changes must update Cargo.lock and pass the dependency policy in deny.toml.

Windows CI checks formatting, Clippy, workspace tests, a development build, dependency policy and vendor/font/icon hashes on pushes to main. It caches Cargo downloads and development build output. Successful main builds save caches for later runs.

For full verification, build and package in a fresh ignored output folder, then run Test-Release.ps1 against the ZIP. The **Windows package verification** workflow runs these checks manually and when dependency, build, packaging, workflow, asset or licence files change. It tests the app's own hidden native window, renderer, archive rejection, installation, upgrade and rollback without a game or elevated privileges. Its candidate ZIP and checksum are retained for two days. Run this workflow before release preparation and for source changes that need native package diagnostics.

Reader/timing changes also need bounded live verification against the supported build before a release. Report the live checks actually performed and their limitations. With the game running, the candidate's `--self-test-reader ABSOLUTE-PRIVATE-DIRECTORY` performs 24 validated observations and stops. The optional `scripts/Test-ReaderLifecycle.ps1` additionally tests GUI startup/duplicates/shutdown; exit other overlay instances before that lifecycle test. Keep raw results local.

## Artwork

Application and tray artwork, the raster source, generated ICO and hash inventory are in `crates/wave-overlay/assets/icon/`. Builds use the checked-in ICO. The Windows SDK resource compiler embeds the icon during the build. Artwork changes must update the ICO and its hash inventory together.

## Safety and privacy

Preserve read-only access, exact build/path pins, object allocation/session checks, observation bounds and stale-data behavior. Fast menu data changes layout only. Keep game access within the query/read-only boundary and run with standard user privileges.

Report bugs using version numbers, reproduction steps and redacted diagnostics. Keep local settings, account paths, process addresses, memory dumps, save files, game assets and extracted datasets outside source control. Review tracked content and history alongside the automated secret-pattern checks before publication.

## Recipe tracker diagnostics

- `--self-test-asset-setup ABSOLUTE-DIRECTORY` exercises the production first-run path in an isolated cache, then checks recipe renders, font selection and 24 world observations. Repeating it checks cache reuse.
- `--self-test-asset-disabled ABSOLUTE-DIRECTORY` checks 24 world observations with setup disabled and verifies that no recipe cache is created.
- `--self-test-asset-cancel ABSOLUTE-DIRECTORY` starts only owned fixture processes and verifies that cancellation stops the entire process tree.
- Run the usual formatting, strict Clippy, workspace tests, dependency/vendor checks and release/package regressions.
- `--self-test-recipes ABSOLUTE-DIRECTORY` checks native shortcut conflict/release handling, actual Nippo font selection, partial/completed/stale renders and premultiplied alpha, using synthetic counters and owned offscreen windows. It does not read the game or start a normal overlay.
- `--self-test-recipe-warmup ABSOLUTE-DIRECTORY` runs the production worker for 24 world observations with connection preparation enabled and no shortcut requests. It verifies unchanged selection and joined shutdown; local logs record connection setup timing.
- `--self-test-dialogs ABSOLUTE-DIRECTORY` checks native settings defaults, Save, Cancel, invalid/file-failure paths and recipe control persistence using isolated preferences.
- `--self-test-recipe-reader ABSOLUTE-DIRECTORY [NATIVE-RECIPE-ID]` measures a bounded hover scan and makes three finite, guarded research observations plus bag inventory against a supported running game. It defaults to Biofilament; an explicit catalogue recipe ID also permits checking validated absence after completion. It records any visible hovered candidate without pinning it, submitting items, opening windows or changing the running overlay. An unavailable candidate outside the station is expected and does not verify hover capture.
- Manually verify Alt+X over the recipe icon, material tiles and row space; scrolling and closing/reopening the station; tracking after walking away; partial submissions, completion and clearing; shortcut changes/conflicts; focus changes; scale and placement. Automated counter reads do not establish those manual results.

The optional `scripts/Import-RecipeAssets.ps1` prepares local exported test fixtures. Keep them outside Git and release archives.
