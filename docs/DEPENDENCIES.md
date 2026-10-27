# Dependency review

Reviewed 2026-10-27. Cargo.lock pins registry versions. The policy in deny.toml checks the application's Windows x64 dependency graph for advisories, permitted licences and approved registries. The release inventory lists these dependencies.

## Registry dependencies

- zstd 0.14.3, zstd-safe 8.2.0 and zstd-sys 2.1.2 address [RUSTSEC-2026-0391](https://rustsec.org/advisories/RUSTSEC-2026-0391.html), [RUSTSEC-2026-0398](https://rustsec.org/advisories/RUSTSEC-2026-0398.html), [RUSTSEC-2026-0403](https://rustsec.org/advisories/RUSTSEC-2026-0403.html) and [RUSTSEC-2026-0410](https://rustsec.org/advisories/RUSTSEC-2026-0410.html). The usmap manifest selects the patched compression dependency.
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

## Recipe setup components

The Rust dependency graph now directly uses the already vendored jmap and usmap crates to produce a local native type schema on the existing bounded reader worker. This does not invoke the standalone dumper or add a game writer. Native schema enumeration runs only when the derived asset cache needs rebuilding; normal menu and shortcut observations keep their direct-reference route.

First-run setup downloads the pinned self-contained UnrealAssetScout 0.5.0 executable, Oodle Data 2.9.16 and zlib-ng binaries from the upstream URLs used by CUE4Parse ec6595e46448. SHA256 checks cover the downloaded archives and executable/native components. Detex comes from the verified helper's embedded resources. Exact identities and licence-notice provenance are in [licenses/asset-setup/index.json](../licenses/asset-setup/index.json). The helper uses its own pinned dependency set; Cargo's advisory check does not audit that managed/native graph. Upgrading it requires a separate review and an empty-cache extraction check.

The app ZIP contains the helper's licence and notices, but none of those executable components or game assets. Oodle remains proprietary and is downloaded at run time rather than redistributed. Setup requires internet access if its cached components are absent or fail validation. Native helper/library execution is confined to a hidden, cancellable Windows job; first-run requests and outputs stay in the user's recipe cache.

## Process-memory dependency update

Dependabot PR #6 updates read-process-memory from 0.1.6 to 0.3.0 in the vendored jmap workspace manifest. VENDOR.json records the reviewed manifest hash change; upstream Rust source is unchanged. The new Windows PID-to-handle conversion requests PROCESS_VM_READ and retains reference-counted handle cleanup. A first-party test reads a known buffer from its own process and checks that an invalid address is rejected. The overlay's separate query/read-only game backend and its build/session guards are unchanged; no game write capability is enabled or exercised.
