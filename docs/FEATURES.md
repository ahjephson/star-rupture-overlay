# Features

StarRupture Overlay shows rupture timing and pinned recipe research progress while you play. It reads the supported game without changing game memory, files or saves.

## Rupture countdown

The countdown shows the current rupture stage: STABLE, PRE-WAVE, APPROACHING, INCINERATING, COOLING or GROWBACK. It follows the game's simulation and pauses with verified game pause. Unavailable or stale readings show `--:--`.

In **Settings > General > Countdown format**, choose hours when needed (`1:20:00`) or total minutes (`80:00`). Both choices keep the panel width fixed. Below an hour, the display uses `MM:SS`.

The wave panel hides while the game reports waves stopped, including the observed tutorial state. INCINERATING lasts seven seconds before the display changes to COOLING.

The display moves below the pause-menu border. To position it yourself, choose **Unlock to move** from the tray menu, drag it, then **Lock position**. **Reset position** restores automatic placement. You can also hide and show the overlays from the tray menu.

Borderless, windowed and the supported fullscreen mode are covered in [compatibility](COMPATIBILITY.md). The overlays hide at the main menu and when the game is minimized or closed. Other windows can cover the game and overlays; fullscreen overlays hide when the game loses focus.

## Recipe research sidebar

With the game focused, open the recipe station, hover over an unfinished recipe row and press **Alt+X**. The sidebar stays visible when you leave the station and closes automatically when the pinned recipe is completed.

| Shortcut target | Result |
| --- | --- |
| Different unfinished recipe | Pin or switch to it |
| Same pinned unfinished recipe | Clear the sidebar |
| Outside the recipe station | Clear an existing pin |
| Completed recipe or empty space inside the station | Keep the current selection |

Each tile shows the material icon and submitted/required research amounts. White counters mean you have some material in your bag; the orange number shows that quantity. Red counters mean you have none. A tick marks a completed requirement. Unavailable readings stay distinct from zero stock. Submit materials using the game's INSERT buttons.

Settings lets you enable the tracker, change its shortcut, choose an upper/middle/lower right position and select 75–150% scale. A replacement recipe appears after a successful reading.

The app prepares names, icons and the recipe font automatically from your installed game. Setup works offline, and later launches reuse the local cache. Progress appears in **Settings > Recipe tracker**. Disabling the tracker or exiting the app stops setup. See [support](SUPPORT.md) if preparation fails.

## App controls

Right-click the notification-area icon for the tray menu, or double-click it to open Settings.

- **Start StarRupture** launches the game through Steam.
- The optional **Play StarRupture with Overlay** desktop shortcut starts the app and game. Settings can create, update or remove it.
- **Start with Windows** waits in the tray for the game. It is optional and off by default.
- Choose whether the app keeps running, asks or exits when a connected game closes.
- **About** shows version and connection information. **Copy support details** helps prepare a bug report.
- **Exit** closes the overlay.

For installation, updates and removal, see the [README](../README.md). For troubleshooting, see [support](SUPPORT.md).
