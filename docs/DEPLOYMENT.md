# Deployment and rollback

## Portable installation

Compare the ZIP's SHA256 with its matching .sha256 file, extract to a fresh user-writable folder and run starrupture-overlay.exe. Keep LICENSE, THIRD-PARTY-NOTICES.md and licenses/ with it. Deployment uses a user-writable directory and standard user privileges.

Before upgrading, disable Start with Windows if enabled, then exit through the overlay tray menu. Enable startup from the new version after verification. In its Settings, choose Update shortcut to redirect an existing desktop shortcut to the new copy, or Create shortcut if it is missing. Extract the new version beside the previous version, test it and retain the old folder for rollback. Single-instance protection keeps one normal HUD running. To roll back, exit the new version and run the previous executable. The app validates caches at startup.

## Scripted per-user deployment

From a trusted source checkout, use scripts/Deploy.ps1 with -ArchivePath. The matching archive.sha256 must be adjacent. The default destination is %LOCALAPPDATA%/Programs/StarRuptureWaveOverlay; -Destination supports a user-selected folder. -Launch is optional and starts only the verified installed app.

The deployer requires the installed app to be closed. It checks archive checksum, entry names, extraction bounds, duplicate/case-colliding paths, manifest shape and every file's size/hash. New files go into a private staging directory, then a version/hash directory. An atomic installation.json update records current and previous folders. Previous installation directories are preserved. Rejected packages leave the previous selection intact; failed staging folders can be reviewed and removed manually.

Use `-Rollback` to verify and select the previous version. The deployer retains both versions, so another rollback switches back. To uninstall, disable Start with Windows and remove the desktop shortcut in Settings, then exit the app and delete the deployment directory, including its version folders and installation.json. Remove preferences and private caches separately if desired.

Checksums and manifests verify file integrity. Candidate executables are unsigned. Upgrades use the explicit deployment procedure above.

## Executable naming compatibility

New packages use `starrupture-overlay.exe`. Deployment also recognises older `starrupture-wave-overlay.exe` packages and resolves the executable from each validated manifest, including on rollback. Packages containing both names or a conflicting executable declaration are rejected. Both process names are checked before modifying an installation. Existing preferences, caches and the default installation folder retain their `StarRuptureWaveOverlay` path so the rename does not discard local data.
