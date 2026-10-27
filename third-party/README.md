# Vendored dependencies

This directory contains the jmap, gospel, patternsleuth and ser-hex libraries used by the reader. [VENDOR.json](VENDOR.json) records their upstream revisions, licences, adaptations and verification hashes. [Source provenance](../docs/EXTRACTION.md) links the baseline inventories.

The vendored workspaces select the required libraries and resolve their shared dependencies through local paths. The root Cargo.lock pins registry versions and checksums. Available upstream licence notices, Cargo licence declarations and READMEs are retained; the standard Apache-2.0 text is supplied centrally. See [third-party notices](../THIRD-PARTY-NOTICES.md) for licence details.

Run `scripts/Verify-Vendor.ps1` from the repository root before building. Preserve vendored files during routine formatting and dependency updates. Deliberate patches require a reason and before/after hashes in VENDOR.json. Use `scripts/Format.ps1` to format the four application crates.

The reader accesses game memory through its query/read-only Windows backend. The upstream READMEs describe the libraries and their own tools; use the repository's [README](../README.md) and [contributing guide](../CONTRIBUTING.md) for overlay build and operation instructions.
