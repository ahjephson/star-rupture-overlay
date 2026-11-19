# Game compatibility

This release targets **StarRupture Update 3: Custom Game** on Steam, game version **0.4.0 (build 130045)**. Earlier full-cycle timing and display checks used Update 3 PTB builds 128786 and 129402.

| Requirement | Supported |
| --- | --- |
| Game update | Update 3: Custom Game |
| Public game build | 130045 |
| Game distribution | Steam |
| Operating system | Windows 10 or 11, 64-bit |
| Display mode | Windowed, Borderless and the game's Fullscreen setting |

The wave and recipe panels have been checked with Alt-Tab and switching between Fullscreen and Borderless. The HUD hides while another application has focus; the compass also hides during pause and game menus. Other exclusive-fullscreen configurations have not been verified.

Start the game and load your world to use the overlays. Recipe setup completes automatically after a world is loaded; later launches reuse the prepared resources. No extra tools need to be installed.

Rupture stages and the return from Growback to Stable have been checked through a Heat wave cycle. Countdown starts have been checked with Rarely, Normal and Often frequency settings. Recipe setup, row selection and research counters have also been checked. Other wave types, modified stage durations and custom time dilation are not verified.

Per-world recipe-pin restoration has been checked on PTR2 through an overlay restart and switching between local worlds. Each world restores its own choices and refreshes research progress from the running game. A missing or ambiguous local save-folder identity keeps restoration unavailable.

Future game builds may require an overlay update. If readings become unavailable after an update, check for a compatible overlay build.
