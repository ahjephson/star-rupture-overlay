# Third-party notices

PolyForm Strict 1.0.0 applies to first-party project code only, copyright (c) 2026 Alex Jephson. Dependencies retain their upstream licences and copyright notices. Release archives include their notice texts under licenses/ and a dependency-inventory.json with package versions/licence expressions.

| Component | Upstream | Licence | Source-checkout notice location |
| --- | --- | --- | --- |
| jmap | https://github.com/trumank/jmap | MIT | third-party/jmap/LICENSE |
| retoc | https://github.com/trumank/retoc | MIT | third-party/retoc/LICENSE |
| repak | https://github.com/trumank/repak | MIT OR Apache-2.0 | third-party/repak/LICENSE-MIT and LICENSE-APACHE |
| bcdec_rs | https://github.com/ScanMountGoat/image_dds | MIT | Exported registry licence in release licenses/ |
| gospel | https://github.com/trumank/gospel | MIT OR Apache-2.0 | third-party/gospel/LICENSE (MIT) |
| patternsleuth | https://github.com/trumank/patternsleuth | MIT OR Apache-2.0 | Upstream manifest declaration; standard Apache-2.0 text in third-party/LICENSE-APACHE |
| ser-hex | https://github.com/trumank/ser-hex | MIT OR Apache-2.0 | Both upstream licence texts retained |
| Oxanium | https://github.com/sevmeyer/oxanium | SIL OFL1.1 | crates/wave-overlay/assets/fonts/OFL.txt |

Selected source revisions, file hashes and local adaptations are recorded in [third-party/VENDOR.json](third-party/VENDOR.json) and [dependency review](docs/DEPENDENCIES.md). The retained patternsleuth source declares its dual licence in its exact-revision Cargo manifest. Its notice location is listed above.

Registry dependencies are pinned in Cargo.lock. For crate archives omitting licence files, exact upstream source copies, URLs and hashes are retained under licenses/registry. These notices are included when exporting the reachable dependency graph. The standard Apache-2.0 text is not a replacement for an upstream copyright notice where one exists.

Oxanium font binaries are embedded under their original names and retain their original licence.

## Recipe setup components

Recipe assets are prepared offline by built-in Rust code from the user's installed game. No third-party executable or native decompressor is downloaded or bundled. Selected retoc/repak source and the BC3 decoder retain their licences listed above. The local retoc/repak adaptations exclude unused native loaders and reject unsupported compressed archives. Locally extracted StarRupture icons, fonts and definitions remain in the user cache and retain their owners' rights.