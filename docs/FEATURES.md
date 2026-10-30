# Features

StarRupture Overlay shows rupture timing and pinned recipe research progress while you play. It reads the supported game without changing game memory, files or saves.

## Rupture countdown

The countdown shows the current rupture stage: STABLE, PRE-WAVE, APPROACHING, INCINERATING, COOLING or GROWBACK. It follows the game's simulation and pauses with verified game pause. Unavailable or stale readings show `--:--`.

In **Settings > General > Countdown format**, choose hours when needed (`1:20:00`) or total minutes (`80:00`). Both choices keep the panel width fixed. Below an hour, the display uses `MM:SS`.

The wave panel hides while the game reports waves stopped, including the observed tutorial state. INCINERATING lasts seven seconds before the display changes to COOLING.

The display moves below the pause-menu border. To position it yourself, choose **Unlock to move** from the tray menu, drag it, then **Lock position**. **Reset position** restores automatic placement. You can also hide and show the overlays from the tray menu.

Borderless, windowed and the supported fullscreen mode are covered in [compatibility](COMPATIBILITY.md). The overlays hide at the main menu and when the game is minimized or closed. Other windows can cover the game and overlays; fullscreen overlays hide when the game loses focus.

## Recipe research sidebar

With the game focused, open the recipe station and pin up to three unfinished recipes. Hover over a recipe row and press **Alt+X**, or double-click the recipe icon or name. Each recipe has its own compact panel and closes when its research is completed.

| Shortcut target | Result |
| --- | --- |
| Unpinned unfinished recipe, with fewer than three pins | Add a panel |
| Pinned unfinished recipe | Remove that panel |
| Fourth unfinished recipe | Keep the existing three pins |
| Outside the recipe station | Clear all pins |
| Completed recipe or empty space inside the station | Keep the current selection |

Double-click pinning works on recipe icons and names, away from ingredient tiles and INSERT buttons. It leaves the game's normal clicks intact. Double-clicks outside the station do not clear pins. In **Settings > Recipe tracker**, turn double-click pinning on or off.

When the game UI cursor is visible, hover over a material tile to highlight it. After a short pause, a small tooltip anchored to the icon's centre shows the item name. It stays in place while you hover over the same item; moving to another item starts a new tooltip. Moving away hides it.

Each panel shows material icons and submitted/required research amounts. White counters mean you have some material in your bag; the orange number shows that quantity. Red counters mean you have none. A tick marks a completed requirement. Unavailable readings stay distinct from zero stock. Submit materials using the game's INSERT buttons.

When the game's UI cursor is visible, click a panel's **×** to remove that recipe. The close cross turns blue when hovered, without a surrounding hover border, and keeps the game's cursor appearance when available. The rest of the panel lets clicks pass through to the game.

Panels sit side by side along the right edge with a top tab on the leftmost panel and a cut lower-right corner on the rightmost panel. A single panel has both details; the other panel edges are plain. They share the height of the tallest selected recipe, with space reserved for two-line names and close buttons. Settings lets you enable the tracker, change its shortcut, choose an upper/middle/lower right position and select 75–150% scale. Icons and spacing are compact; a new panel appears only after a successful reading. The rightmost panel keeps the same screen-edge gap during gameplay and pause. On the pause menu, the group shrinks only if it needs more room, leaving at least the same gap beside the menu. Smaller groups keep their normal size and right-edge position. If it cannot fit at a readable size, it hides until the menu closes. Close buttons keep their click area when panels shrink.

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
