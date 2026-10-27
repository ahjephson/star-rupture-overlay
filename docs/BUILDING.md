# Building and maintaining the app

This guide documents the owner's build and maintenance workflow. Pull requests and code contributions are not accepted. First-party code uses [PolyForm Strict 1.0.0](../LICENSE). Read [architecture](ARCHITECTURE.md) and [compatibility](COMPATIBILITY.md) before maintenance.

## Build a local candidate

Use Windows with Rust 1.99.1, Visual Studio C++ Build Tools and the Windows SDK, PowerShell 7 and Git. Run commands from the repository root:

```powershell
./scripts/Install-QualityTools.ps1
./scripts/Test-Dependencies.ps1
./scripts/Test-PublicContent.ps1
./scripts/Build.ps1 -OutputDirectory local/candidate
./scripts/Package-Release.ps1 -BuildDirectory local/candidate
```

Build.ps1 checks formatting, strict Clippy, workspace tests and vendor hashes. It requires a fresh output directory and leaves the running app unchanged. Use `-Offline` with cached dependencies, or `-ToolchainDirectory PATH` for the pinned local toolchain. Packaging requires a clean checkout by default; `-AllowUncommitted` marks development archives. [Release preparation](RELEASING.md) and [deployment](DEPLOYMENT.md) describe the separate packaging and activation steps.

Recipe setup uses built-in Rust code to prepare assets offline from the supported installed game. A loaded world supplies the native type schema; the app discovers the archives and maintains its local asset cache. The [recipe tracker guide](RECIPE-TRACKER.md) documents finite setup, cancellation, rendering and live reader diagnostics. All generated data belongs in ignored local storage.

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
