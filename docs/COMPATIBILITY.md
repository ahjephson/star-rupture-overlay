# Supported environment

The 0.2.7 development candidate supports the environment below.

| Requirement | Supported scope |
| --- | --- |
| Operating system | Windows 10/11 x64 |
| Display mode | Borderless/windowed; Fullscreen verified on the supported game build |
| Game distribution | Steam build 25646882, one exact executable hash |
| Installation | Location detected from the running game process |
| Game/map state | Supported main-map running session |

Both overlays support the game's Fullscreen setting on the supported system, including Alt-Tab and switching to Borderless. Topmost fullscreen windows require the game to have focus; the overlays hide while another application is foreground. Support does not extend to unverified exclusive-fullscreen configurations.

Executable filename: `StarRuptureGameSteam-Win64-Shipping.exe`. Windows supplies its full path; the reader checks the executable hash and process creation identity before reading game memory. Multiple matching processes are rejected as ambiguous.

Supported executable SHA256:

```text
95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1
```

A game update can invalidate support. Unsupported builds and runtime states produce unavailable data. Compatibility updates require reflection, allocation and timing verification plus live tests.

The overlay executable statically links the Visual C++ runtime. Recipe setup uses built-in Rust code and works offline, with no additional runtime or extraction tool to install. Native type preparation requires a supported loaded world; later launches reuse a validated asset cache. Game files remain read-only.
