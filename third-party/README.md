# Retained helper dependencies

These are selected source libraries from the read-only reference helper's retained jmap 0.2.0 source and locked gospel, patternsleuth and ser-hex revisions. `VENDOR.json` records origin, revisions, licenses, adaptations and file hashes. The pre-adaptation selected hashes are under `docs/reference/helper-vendor-original-hashes.json`; the original helper lock is retained separately. No extracted game data or upstream game-derived usmap test fixtures are included.

Workspace manifests select only the libraries needed by the helper and replace Git dependencies with local paths. One unused patternsleuth import was removed for warnings-denied builds. Available upstream license notices, Cargo license declarations and readmes are retained; the standard Apache-2.0 text is also supplied centrally. Registry dependency versions/checksums are pinned by the root Cargo.lock. Code licenses do not establish game/community data redistribution rights.

Run `scripts/Verify-Vendor.ps1` before building. Do not run a formatter or dependency updater over this tree. Format the three owned workspace packages explicitly. The production helper uses its own query/read-only Windows backend; it does not invoke the full dumper, upstream HTTP backends, internal process writes or remote game functions.
