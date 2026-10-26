# Dependency review

Reviewed 2026-10-26. Cargo.lock pins registry versions. The policy in deny.toml checks the application's Windows x64 dependency graph for advisories, permitted licences and approved registries. The release inventory lists these dependencies.

## Registry dependencies

- crossbeam-epoch 0.9.20 addresses [RUSTSEC-2026-0204](https://rustsec.org/advisories/RUSTSEC-2026-0204.html).
- anyhow 1.0.103 addresses RUSTSEC-2026-0190.
- The patternsleuth workspace manifest disables unused compressed-debug decoding, excluding the ruzstd 0.5 dependency affected by [RUSTSEC-2026-0344](https://rustsec.org/advisories/RUSTSEC-2026-0344.html).

## Maintenance exception

[RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) marks paste 1.0.15 unmaintained. Vendored dependencies use it as a compile-time macro. This is the single advisory exception in deny.toml and requires review before each release. Transitive duplicate versions appear as audit warnings.

## Source and licence provenance

[third-party/VENDOR.json](../third-party/VENDOR.json) records upstream revisions, adaptations and verification hashes. Manifest patches disable unused decompression and mark retained forks non-publishable. The vendor guide describes the reader's dependencies and maintenance commands. Each local patch records its prior hash and reason; formatting changes to gospel have a separate provenance record. Run vendor verification after each deliberate change.

Registry packages retain their own licence declarations. Export-Notices.ps1 copies reachable upstream licence/copyright files into the release. Four crate archives omit licence files; exact-revision upstream copies and their URLs/hashes are retained in licenses/registry/index.json. Packaging requires the corresponding licence notices. Oxanium fonts retain SIL OFL 1.1 and their original hashes.

## First-party licence

First-party code is licensed under PolyForm Strict 1.0.0, copyright (c) 2026 Alex Jephson. The terms and attribution are in LICENSE. Cargo metadata uses the custom SPDX reference LicenseRef-PolyForm-Strict-1.0.0; deny.toml permits it for the four first-party crates. Dependencies retain their upstream licences.
