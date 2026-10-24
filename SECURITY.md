# Security

The app opens only the supported game process with query/read permissions. It does not inject code, request debug privileges, modify memory or saves, or contact a server. Unknown builds, invalid identities and unavailable evidence fail closed.

Supported security fixes will target the latest published version. No public version has been released yet. The maintained dependency policy is in deny.toml; reviewed exceptions and vendor changes are documented in docs/DEPENDENCIES.md.

For a suspected vulnerability, use GitHub private vulnerability reporting when enabled. If it is unavailable, request a private contact through an issue without posting vulnerability details or personal data. Do not upload saves, caches, memory dumps or credentials.

Release archives include SHA256 checksums and file manifests for integrity. The initial candidate is unsigned; these checks do not establish publisher authenticity. Public signing/provenance configuration is a separate release decision.
