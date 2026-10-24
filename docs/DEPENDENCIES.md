# Dependency review

Reviewed 2026-10-24. Cargo.lock pins exact registry versions. deny.toml audits the reachable Windows x64 graph for advisories, acceptable licences and approved source registries. The release inventory contains only reachable packages; raw cargo metadata/account paths are not distributed.

## Advisory fixes

- crossbeam-epoch updated from 0.9.18 to 0.9.20 for [RUSTSEC-2026-0204](https://rustsec.org/advisories/RUSTSEC-2026-0204.html).
- anyhow updated from 1.0.102 to 1.0.103 for RUSTSEC-2026-0190.
- Disabled unused object compressed-debug decoding in the retained patternsleuth workspace manifest, removing ruzstd0.5 from the graph for [RUSTSEC-2026-0344](https://rustsec.org/advisories/RUSTSEC-2026-0344.html). The external PE reader does not require compressed debug sections. Upstream Rust source and game-read guards are unchanged.

## Maintenance exception

[RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) marks paste1.0.15 unmaintained, with no patched version. The vendored dependencies use it as a compile-time macro. The single named exception is documented in deny.toml; no runtime vulnerability or unsoundness advisory is ignored. Review an upstream replacement before each release. Transitive duplicate versions are visible audit warnings, not silently rewritten vendor dependency ranges.

## Source and licence provenance

third-party/VENDOR.json retains upstream sources, revisions and reviewed file hashes. Local manifest-only patches disable unused decompression and mark retained forks non-publishable; each records the prior hash and reason. The prior formatting-only gospel changes retain their separate provenance record. Vendor verification must pass after any deliberate review.

Registry packages retain their own licence declarations. Export-Notices.ps1 copies reachable upstream licence/copyright files into the release. Four crate archives omit licence files; exact-revision upstream copies and their URLs/hashes are retained in licenses/registry/index.json. Missing future notices fail packaging instead of fabricating attribution. Oxanium fonts retain SIL OFL1.1 and their original hashes. No game assets or derived data are part of these dependencies.

## First-party licence

The owner selected PolyForm Strict1.0.0, copyright (c)2026 Alex Jephson. The unmodified terms plus attribution are in LICENSE. Cargo metadata uses the custom SPDX reference LicenseRef-PolyForm-Strict-1.0.0; deny.toml permits it only for the four first-party crates. No third-party licence is changed or waived. Public availability is not a general permission to modify or redistribute the first-party code; see the actual licence terms.
