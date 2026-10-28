# Security

The overlay reads the supported game process with standard user permissions. It also reads installed game archives to prepare recipe icons, fonts and definitions. It does not write to game memory, game files or saves. Unsupported builds and unavailable readings remain unavailable.

## Reporting a vulnerability

Use **Security > Report a vulnerability** on this repository when private vulnerability reporting is available. If that option is missing, open an issue asking for a private security contact without including the vulnerability details. Keep reproduction details, credentials, saves, memory dumps and local data out of public issues.

Security maintenance targets the latest published version. The project makes no guaranteed response-time commitment.

## Downloads and first-run setup

The app is unsigned. Published release ZIPs include SHA256 checksums and file manifests for checking integrity; these are not publisher signatures.

Recipe setup works offline using Rust code built into the app. The read-only game connection supplies native type information, and the app reads the installed archives to prepare local assets. Extraction runs in a hidden process launched from the same executable, with standard user permissions and cancellation/timeout cleanup. Completed caches are validated against the installed archives and prepared-file hashes. Derived assets and game data stay on the user's computer. See [setup dependencies](docs/DEPENDENCIES.md#recipe-setup-components) and [support](docs/SUPPORT.md).
