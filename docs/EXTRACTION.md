# Source provenance

The source inventories in this repository record first-party and vendored origins:

- [extracted-source-sha256.json](extracted-source-sha256.json) records the first-party extraction baseline from 2026-10-24.
- [third-party/VENDOR.json](../third-party/VENDOR.json) records vendored source revisions, licences, reviewed adaptations and current verification hashes.
- [reference/helper-vendor-original-hashes.json](reference/helper-vendor-original-hashes.json) records the original selected vendor-source hashes.
- [vendor-format-verification.json](vendor-format-verification.json) records formatting provenance for 19 gospel source files.

The vendor inventory documents manifest adaptations and links retained files to their upstream revisions. Vendor verification compares those files against the inventory. Registry dependency versions and checksums are pinned in Cargo.lock.

First-party code is licensed under PolyForm Strict 1.0.0. Vendored libraries, registry dependencies and embedded fonts retain their own licences; see [third-party notices](../THIRD-PARTY-NOTICES.md) and [dependency review](DEPENDENCIES.md).
