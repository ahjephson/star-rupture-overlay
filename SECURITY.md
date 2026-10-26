# Security

The app opens the supported game process with query/read permissions and runs with standard user privileges. Game observations are read through an embedded worker with process/build/session identity checks, allocation guards, read budgets and freshness limits. Unknown builds, invalid identities and unavailable evidence produce unavailable data.

Version 0.1.0 is in development. Security maintenance targets the latest published version. The dependency policy is in deny.toml; reviewed exceptions and vendor changes are documented in docs/DEPENDENCIES.md.

For a suspected vulnerability, use GitHub private vulnerability reporting when enabled. If it is unavailable, request a private contact through an issue. Keep vulnerability details, saves, caches, memory dumps and credentials private.

Release archives include SHA256 checksums and file manifests for integrity. The candidate is unsigned. Publisher signing and provenance are release-readiness items.
