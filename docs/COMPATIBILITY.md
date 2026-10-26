# Supported environment

Version 0.1.0 supports the environment below. Supporting other installation locations requires a compatibility change.

| Requirement | Supported scope |
| --- | --- |
| Operating system | Windows 10/11 x64 |
| Display mode | Borderless/windowed; exclusive fullscreen is not validated |
| Game distribution | Steam, one exact executable hash |
| Installation | Default C-drive Steam path below |
| Game/map state | Supported main-map running session |

Executable path:

```text
C:\Program Files (x86)\Steam\steamapps\common\StarRupture\StarRupture\Binaries\Win64\StarRuptureGameSteam-Win64-Shipping.exe
```

Supported executable SHA256:

```text
95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1
```

A game update can invalidate support. Unsupported builds, installation paths and runtime states produce unavailable data. Compatibility updates require reflection, allocation and timing verification plus live tests.

The executable statically links the Visual C++ runtime and uses Windows system DLLs. The app starts with default settings and accepts an optional configuration file.
