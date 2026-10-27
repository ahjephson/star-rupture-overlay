# Dependencies

Cargo.lock pins registry versions. The policy in deny.toml checks the application's Windows x64 dependency graph for advisories, permitted licences and approved registries. Release archives contain a dependency-inventory.json and the reachable dependencies' licence notices.

## Components

| Component | Role |
| --- | --- |
| jmap and jmap_dumper | Native reflection types and schema generation through the reader's process boundary |
| usmap | Native property-schema serialization |
| gospel and patternsleuth | Runtime layout and reference discovery |
| retoc 0.1.5 | IoStore/Zen archive access and cooked-package conversion |
| repak 0.2.3 | PAK archive access for the recipe font |
| bcdec_rs 0.2.1 | BC3 texture decoding |
| Windows bindings | Native windows, rendering, tray controls and process access |

The five first-party crates define the UI, live reader, asset extraction, timing/configuration and shared observation model. Their responsibilities are described in [architecture](ARCHITECTURE.md).

## Recipe setup components

The reader generates a native type schema when the derived asset cache needs preparation. The assets crate discovers research recipes from the installed archives, resolves item/name/icon references and reads the supported unversioned properties and cooked texture/string-table data. BC3 and raw BGRA graphics become the renderer's local pixel files. Normal menu and shortcut observations use direct runtime references; native schema enumeration is a setup operation.

Extraction runs in a hidden process launched from the app's own Rust executable. Exit, disable and timeout terminate the owned setup process tree. Only complete validated generations replace the current cache receipt. Setup works offline, and game-derived assets stay in the local cache.

The retained retoc and repak libraries contain no standalone tools, game fixtures or native loaders. Retoc supports the selected uncompressed archives; repak uses no default compression/encryption features. Supported container identities are checked before extraction. The resolved Cargo graph contains no oodle_loader or its HTTP/TLS clients. Other registry compression libraries used by schema tooling are covered by the Cargo policy.

## Process access

The overlay's query/read-only Windows backend enforces build, session, allocation and read-budget checks. The reflection libraries also depend on read-process-memory 0.3.0; its Windows handle requests PROCESS_VM_READ and uses reference-counted cleanup. A first-party test reads a known buffer in its own process and rejects an invalid address. Game writes are outside the reader's interface.

## Maintenance exception

[RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) marks paste 1.0.15 unmaintained. Vendored dependencies use it as a compile-time macro. This is the advisory exception in deny.toml and requires review before each release. Transitive duplicate versions appear as audit warnings.

## Source and licence provenance

[third-party/VENDOR.json](../third-party/VENDOR.json) records upstream revisions, verification hashes and deliberate local adaptations. The [vendor guide](../third-party/README.md) describes source verification. Vendor verification covers retained file bytes; registry versions and checksums are pinned in Cargo.lock.

Export-Notices.ps1 copies reachable licence and attribution files into each release. Crate archives without separate notice files use the reviewed declarations and texts recorded in licenses/registry/index.json. Oxanium fonts retain SIL OFL 1.1 and their original hashes. Dependencies retain their upstream licences; see [third-party notices](../THIRD-PARTY-NOTICES.md).

First-party code uses PolyForm Strict 1.0.0, copyright (c) 2026 Alex Jephson. Cargo metadata uses LicenseRef-PolyForm-Strict-1.0.0, permitted for the five project crates in deny.toml. The terms and attribution are in LICENSE.
