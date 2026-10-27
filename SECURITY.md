# Security

The overlay reads the supported game process with standard user permissions. It also reads installed game archives to prepare recipe icons, fonts and definitions. It does not write to game memory, game files or saves. Unsupported builds and unavailable readings remain unavailable.

## Reporting a vulnerability

Use **Security > Report a vulnerability** on this repository when private vulnerability reporting is available. If that option is missing, open an issue asking for a private security contact without including the vulnerability details. Keep reproduction details, credentials, saves, memory dumps and local data out of public issues.

Security maintenance targets the latest published version. The current 0.2.7 candidate is in development; no public release has been published yet. The project makes no guaranteed response-time commitment.

## Downloads and first-run setup

The app is unsigned. Published release ZIPs include SHA256 checksums and file manifests for checking integrity; these are not publisher signatures.

The recipe tracker's initial setup downloads pinned extraction components from their upstream GitHub releases. Their hashes are verified before use. The setup runs with standard user permissions in a local cache, and its temporary process tree is stopped on cancellation or timeout. Derived assets and game data stay on the user's computer. See [setup dependencies](docs/DEPENDENCIES.md#recipe-setup-components) and [support](docs/SUPPORT.md).
