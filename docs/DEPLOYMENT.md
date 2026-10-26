# Deployment and rollback

## Portable installation

Compare the ZIP's SHA256 with its matching .sha256 file, extract to a fresh user-writable folder and run starrupture-wave-overlay.exe. Keep LICENSE, THIRD-PARTY-NOTICES.md and licenses/ with it. Deployment uses a user-writable directory and standard user privileges.

Before upgrading, exit through the overlay tray menu. Extract the new version beside the previous version, test it and retain the old folder for rollback. Single-instance protection keeps one normal HUD running. Moving back to the old executable restores the earlier app; caches are independently validated at startup.

## Scripted per-user deployment

From a trusted source checkout, use scripts/Deploy.ps1 with -ArchivePath. The matching archive.sha256 must be adjacent. The default destination is %LOCALAPPDATA%/Programs/StarRuptureWaveOverlay; -Destination supports a user-selected folder. -Launch is optional and starts only the verified installed app.

The deployer requires the installed app to be closed. It checks archive checksum, entry names, extraction bounds, duplicate/case-colliding paths, manifest shape and every file's size/hash. New files go into a private staging directory, then a version/hash directory. An atomic installation.json update records current and previous folders. Previous installation directories are preserved. Rejected packages leave the previous selection intact; failed staging folders can be reviewed and removed manually.

Use -Rollback to verify and select the previous version. It also retains the version being left, so another rollback can toggle back. Uninstall by exiting the app and deleting its selected installation folder; optionally remove the private cache separately. Installation folders and caches are removed manually.

Checksums and manifests verify file integrity. Candidate executables are unsigned. Upgrades use the explicit deployment procedure above.
