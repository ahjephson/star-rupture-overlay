# Supported environment

The 0.2.7 development candidate supports the environment below.

| Requirement | Supported scope |
| --- | --- |
| Operating system | Windows 10/11 x64 |
| Display mode | Borderless/windowed; Fullscreen verified on the supported game build |
| Game distribution | Steam build 25646882, one exact executable hash |
| Installation | Location detected from the running game process |
| Game/map state | Supported main-map running session |

Manual display-mode checks, reported by the owner on 2026-10-27: the earlier build hid both overlays in Fullscreen because it rejected topmost game windows. With foreground-only topmost following installed, both overlays work in Fullscreen; Alt-Tab away and back and switching back to Borderless also work. This verifies the supported game’s Fullscreen setting on the tested system. The underlying exclusive/fullscreen-optimization presentation mode was not measured; true exclusive fullscreen on other configurations is not certified.

Executable filename: `StarRuptureGameSteam-Win64-Shipping.exe`. Windows supplies its full path; the reader checks the executable hash and process creation identity before reading game memory. Multiple matching processes are rejected as ambiguous.

Supported executable SHA256:

```text
95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1
```

A game update can invalidate support. Unsupported builds and runtime states produce unavailable data. Compatibility updates require reflection, allocation and timing verification plus live tests.

The overlay executable statically links the Visual C++ runtime. Recipe setup uses the Windows PowerShell included with Windows and downloads a self-contained extraction helper, so users do not need a separate SDK or runtime installation. The first setup needs internet access; a validated asset cache is reused on later launches. Preparing native type information requires a supported loaded world. Game files remain read-only.
