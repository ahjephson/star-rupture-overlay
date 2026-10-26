# Contributing

Issue reports and feature requests are welcome. First-party code uses [PolyForm Strict 1.0.0](LICENSE), which restricts modification and redistribution; discuss source-code contributions with the owner before preparing patches. The checks below describe the maintainer workflow. Read [architecture](docs/ARCHITECTURE.md) and [compatibility](docs/COMPATIBILITY.md) before changes.

## Rust conventions

Follow the [Rust Style Guide](https://doc.rust-lang.org/style-guide/) and use the pinned rustfmt and Clippy versions. Keep macro bodies readable; expand large observation objects across lines. Use workspace inheritance for shared Cargo package settings.

Keep modules focused on discovery, observation validation, timing, rendering or application lifetime. Prefer explicit imports for project dependencies and the narrowest useful visibility. Win32 API imports may be grouped by platform namespace. Use ownership and RAII for resources and document the lifetime, thread and aliasing requirements of unsafe interfaces. Error handling must preserve unavailable states and existing cleanup paths.

For maintenance refactors, preserve configuration acceptance, serialized fields, numeric operations, read ordering, scheduling and native resource lifetimes. Make formatting changes separately from structural changes. Changes to compatibility, timing, UI behaviour or error handling need explicit scope and regression coverage. Preserve validation guards and observed native timing during cleanup.

## Documentation

Describe current behaviour and supported conditions for the document's audience. Explain technical constraints where they help readers operate or maintain the app. Keep each prose paragraph on one source line and preserve structural breaks for headings, lists, tables and code. Preserve source and licence provenance. Summarize checks and limitations in PR descriptions, link relevant CI runs, and include release-specific live checks in release notes. Keep personal records and raw diagnostics in ignored local storage.

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

Build and package in a fresh ignored output folder, then run Test-Release.ps1 against the ZIP. CI performs the same checks on Windows. It tests the app's own hidden native window, renderer, archive rejection, installation, upgrade and rollback without a game or elevated privileges.

Reader/timing changes also need bounded live verification against the supported build before a release. Report the live checks actually performed and their limitations. With the game running, the candidate's `--self-test-reader ABSOLUTE-PRIVATE-DIRECTORY` performs 24 validated observations and stops. The optional `python scripts/test_reader.py` additionally tests GUI startup/duplicates/shutdown; exit other overlay instances before that lifecycle test. Keep raw results local.

## Artwork

Application and tray artwork, the raster source, generated ICO and hash inventory are in `crates/wave-overlay/assets/icon/`. Builds use the checked-in ICO. The Windows SDK resource compiler embeds the icon during the build. Artwork changes must update the ICO and its hash inventory together.

## Safety and privacy

Preserve read-only access, exact build/path pins, object allocation/session checks, observation bounds and stale-data behavior. Fast menu data changes layout only. Keep game access within the query/read-only boundary and run with standard user privileges.

Report bugs using version numbers, reproduction steps and redacted diagnostics. Keep local settings, account paths, process addresses, memory dumps, save files, game assets and extracted datasets outside source control. Review tracked content and history alongside the automated secret-pattern checks before publication.
