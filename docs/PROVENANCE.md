# Source and asset provenance

First-party code and the app/tray icon artwork use the PolyForm Strict 1.0.0 licence in [LICENSE](../LICENSE). Dependencies, fonts and other third-party material retain their own licences and notices; see [third-party notices](../THIRD-PARTY-NOTICES.md).

## Vendored and registry dependencies

[third-party/VENDOR.json](../third-party/VENDOR.json) records the retained upstream sources, versions or revisions, original and current file hashes, formatting and deliberate local patches. The original hash inventory is included directly in that manifest. `scripts/Verify-Vendor.ps1` checks the retained files against it. Licence texts and upstream declarations stay beside the source or in the central third-party notices.

Cargo.lock pins registry versions and source checksums. [licenses/registry/index.json](../licenses/registry/index.json) records reviewed notice sources and hashes for packages whose archives omit standalone notices. Release packages contain the reachable dependencies' notices and a dependency-inventory.json. The [dependency guide](DEPENDENCIES.md) describes their roles and the maintenance exception.

## Fonts and artwork

Oxanium font files retain their upstream names and SIL OFL 1.1 licence. The [font manifest](../crates/wave-overlay/assets/fonts/manifest.json) records the exact upstream revision, source URLs and hashes.

Alex Jephson created the app and tray icon artwork. The original raster and generated ICO are covered by the repository's PolyForm Strict 1.0.0 licence, copyright (c) 2026 Alex Jephson. The canonical raster source, generated ICO, sizes and hashes are recorded in the [icon manifest](../crates/wave-overlay/assets/icon/manifest.json). The README's overlay preview is an illustrative render of the app's own interface.

StarRupture icons, recipe definitions and the recipe font are read from each user's installed game into a local cache. Those extracted assets and native type schemas are excluded from the repository and app ZIP. Their owners retain their rights.
