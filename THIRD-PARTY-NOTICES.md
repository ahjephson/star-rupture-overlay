# Third-party notices

PolyForm Strict 1.0.0 applies to first-party project code only, copyright (c) 2026 Alex Jephson. Dependencies retain their upstream licences and copyright notices. Release archives include their notice texts under licenses/ and a dependency-inventory.json with package versions/licence expressions.

| Component | Upstream | Licence | Source-checkout notice location |
| --- | --- | --- | --- |
| jmap | https://github.com/trumank/jmap | MIT | third-party/jmap/LICENSE |
| gospel | https://github.com/trumank/gospel | MIT OR Apache-2.0 | third-party/gospel/LICENSE (MIT) |
| patternsleuth | https://github.com/trumank/patternsleuth | MIT OR Apache-2.0 | Upstream manifest declaration; standard Apache-2.0 text in third-party/LICENSE-APACHE |
| ser-hex | https://github.com/trumank/ser-hex | MIT OR Apache-2.0 | Both upstream licence texts retained |
| Oxanium | https://github.com/sevmeyer/oxanium | SIL OFL1.1 | crates/wave-overlay/assets/fonts/OFL.txt |

Selected source revisions, file hashes and local adaptations are recorded in [third-party/VENDOR.json](third-party/VENDOR.json) and [dependency review](docs/DEPENDENCIES.md). The retained patternsleuth source declares its dual licence in its exact-revision Cargo manifest. Its notice location is listed above.

Registry dependencies are pinned in Cargo.lock. For crate archives omitting licence files, exact upstream source copies, URLs and hashes are retained under licenses/registry. These notices are included when exporting the reachable dependency graph. The standard Apache-2.0 text is not a replacement for an upstream copyright notice where one exists.

Oxanium font binaries are embedded under their original names and retain their original licence.

## Recipe setup components

First-run recipe setup downloads a pinned UnrealAssetScout executable and its required decompressors from the upstream sources used by that release. They are stored in the local user cache rather than included in this repository or the app ZIP. UnrealAssetScout uses MIT, CUE4Parse uses Apache-2.0, and their other components retain the licences in the upstream notices. Oodle Data is proprietary and is not redistributed with the app.

Exact versions, sources and hashes are recorded in [the setup component inventory](licenses/asset-setup/index.json). The app archive includes the [UnrealAssetScout licence](licenses/asset-setup/UnrealAssetScout-LICENSE.txt) and [upstream third-party notices](licenses/asset-setup/UnrealAssetScout-NOTICES.txt). Locally extracted StarRupture icons, fonts and definitions stay in the user cache and retain their owners' rights.
