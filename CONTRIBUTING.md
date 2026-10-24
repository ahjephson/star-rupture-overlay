# Contributing

Issue reports and feature requests are welcome. First-party code uses [PolyForm Strict 1.0.0](LICENSE), which restricts modification and redistribution; discuss source-code contributions with the owner before preparing patches. The checks below describe the authorized maintainer workflow. Read [architecture](docs/ARCHITECTURE.md) and [compatibility](docs/COMPATIBILITY.md) before changes.

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

Reader/timing changes also need bounded live verification against the supported build before a release. Never fabricate successful live checks. With the game running, the candidate's `--self-test-reader ABSOLUTE-PRIVATE-DIRECTORY` performs 24 validated observations and stops. The optional `python scripts/test_reader.py` additionally tests GUI startup/duplicates/shutdown; exit other overlay instances before that lifecycle test. Keep raw results local.

## Safety and privacy

Preserve read-only access, exact build/path pins, object allocation/session checks, observation bounds and stale-data behavior. Fast menu data changes layout only. Do not add writes, injection, privilege escalation, implicit saves or network telemetry.

Do not commit local settings, account paths, process addresses, memory dumps, save files, game assets or extracted datasets. Report bugs using version numbers, reproduction steps and redacted diagnostics. Secret-pattern checks are a useful gate, not proof that arbitrary content is safe to publish.
