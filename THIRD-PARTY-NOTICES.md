# Third-party notices

PolyForm Strict 1.0.0 applies to first-party project code only, copyright (c) 2026 Alex Jephson. Dependencies retain their upstream licences and copyright notices. Release archives include their notice texts under licenses/ and a dependency-inventory.json with package versions/licence expressions.

| Component | Upstream | Licence | Retained notice |
| --- | --- | --- | --- |
| jmap | https://github.com/trumank/jmap | MIT | third-party/jmap/LICENSE |
| gospel | https://github.com/trumank/gospel | MIT OR Apache-2.0 | third-party/gospel/LICENSE (MIT) |
| patternsleuth | https://github.com/trumank/patternsleuth | MIT OR Apache-2.0 | Upstream manifest declaration; standard Apache-2.0 text in third-party/LICENSE-APACHE |
| ser-hex | https://github.com/trumank/ser-hex | MIT OR Apache-2.0 | Both upstream licence texts retained |
| Oxanium | https://github.com/sevmeyer/oxanium | SIL OFL1.1 | crates/wave-overlay/assets/fonts/OFL.txt |

Selected source revisions, file hashes and local manifest-only changes are recorded in third-party/VENDOR.json and docs/DEPENDENCIES.md. Selected patternsleuth upstream source did not include a project-specific licence text or NOTICE; its exact-revision Cargo manifest declares the dual licence. No upstream Rust source was changed during public-release preparation.

Registry dependencies are pinned in Cargo.lock. For crate archives omitting licence files, exact upstream source copies, URLs and hashes are retained under licenses/registry. These notices are included when exporting the reachable dependency graph. The standard Apache-2.0 text is not a replacement for an upstream copyright notice where one exists.

Oxanium font binaries are embedded without modification or renaming and retain their original licence. No game binaries, extracted assets, personal saves, memory captures or runtime configuration are distributed.
