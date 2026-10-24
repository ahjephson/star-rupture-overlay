# Supported environment

Status: initial Windows x64 development candidate, 2026-10-24.

| Requirement | Supported scope |
| --- | --- |
| Operating system | Windows 10/11 x64; actual live development validation on the maintainer's Windows system |
| Display mode | Borderless/windowed; exclusive fullscreen is not validated |
| Game distribution | Steam, one exact executable hash |
| Installation | Default C-drive Steam path below; other library locations currently fail closed |
| Game/map state | Supported main-map running session; no saved-data fallback |

Executable path:

```text
C:\Program Files (x86)\Steam\steamapps\common\StarRupture\StarRupture\Binaries\Win64\StarRuptureGameSteam-Win64-Shipping.exe
```

Supported executable SHA256:

```text
95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1
```

A game update can invalidate support. Do not replace the hash or remove guards merely to make an unknown build attach. A compatibility update needs fresh reflection/allocation/timing verification and live tests. Extending installation-location support is a future task; the current release intentionally retains the validated path boundary.

Published candidate binaries statically link the VC runtime. Their import table is checked for accidental separate VC redistributable dependencies. Windows system DLLs remain required. No Node.js, .NET, Python, Rust compiler, MCP server or settings file is needed to run the packaged app.
