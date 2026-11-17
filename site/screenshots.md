# Screenshot catalogue

All captures were supplied by the owner. Preserve the originals; do not recreate application UI.

| Capture | Location | Status |
| --- | --- | --- |
| Factory overview, STABLE countdown 13:58, Personal Marker 41 m, three recipe panels | `website/assets/in-game-overview.png` | Used in the hero and feature close-ups |
| Personal Marker arrival cue, STABLE countdown 13:33 | `website/assets/personal-marker-arrival.png` | Used for the separate arrival close-up and full in-game view |
| PRE-WAVE countdown 01:41, Personal Marker 91 m | `../local/website-screenshot-originals/product-crops/pre-wave-pending.png` | Catalogued only. Owner explicitly requested that this capture not be used yet. Kept outside the served website. |
| Clear compass against a dark wall, Personal Marker side arrow 92 m, PRE-WAVE countdown 01:15 | `../local/website-screenshot-originals/product-crops/compass-side-arrow-pending.png` | Used with owner approval as an alternative compass close-up for comparison. Served copy: `website/assets/compass-clear.png`. Current factory-background close-up retained. |
| PRE-WAVE final seconds, Rupture in 00:05, viewed from the airlock | `../local/website-screenshot-originals/product-crops/pre-wave-final-seconds-pending.png` | Used with owner approval in the six-stage timer close-up. Served copy: `website/assets/timer-pre-wave.png`. Original retained. |
| APPROACHING, Next stage in 00:03, viewed from the airlock | `../local/website-screenshot-originals/product-crops/approaching-pending.png` | Used with owner approval in the six-stage timer close-up. Served copy: `website/assets/timer-approaching.png`. Original retained. |
| INCINERATING, Next stage in 00:04, viewed from the airlock | `../local/website-screenshot-originals/product-crops/incinerating-pending.png` | Used with owner approval in the six-stage timer close-up. Served copy: `website/assets/timer-incinerating.png`. Original retained. |
| COOLING, Next stage in 01:09, viewed from the airlock | `../local/website-screenshot-originals/product-crops/cooling-pending.png` | Used with owner approval in the six-stage timer close-up. Served copy: `website/assets/timer-cooling.png`. Original retained. |
| GROWBACK, Next stage in 12:12, viewed from the airlock | `../local/website-screenshot-originals/product-crops/growback-pending.png` | Used with owner approval in the six-stage timer close-up. Served copy: `website/assets/timer-growback.png`. Original retained. |

## Privacy-cleaned app controls

The owner supplied nine tray/menu/settings captures and explicitly approved exact crops and masking. Eight unique cleaned assets are served from `website/assets/ui/`: `tray-waiting.png`, `menu-waiting.png`, `menu-connecting.png`, `menu-connected.png`, `settings-general.png`, `settings-wave-timer.png`, `settings-compass.png` and `settings-recipe-tracker.png`. The two Compass settings originals are duplicates. All Settings captures show Update 3.

Private originals and the crop provenance manifest are kept under ignored `local/website-screenshot-originals/`, outside the site source and served directory. Crops exclude desktop/game backgrounds and other tray icons. Transparent areas have their RGB cleared as well as their alpha; output PNGs contain no source metadata. `site/prepare-settings-images.py` reproduces the approved pixel crops and checks that visible UI pixels match the originals.

Still useful: an Update 2 single-recipe panel to illustrate the version difference.

Feature close-ups now use exact pixel crops in `website/assets/`, rather than hiding full captures with CSS. Full in-game overview and arrival images remain because the page explicitly displays and links them. Unused captures and duplicate originals are archived under ignored `local/`; they are excluded from source publication and the Pages package.
