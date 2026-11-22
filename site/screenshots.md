# Screenshot catalogue

In-game captures were supplied by the owner. Preserve the originals and use captures of the actual application UI.

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

The tray and menu images are exact crops from owner-supplied screenshots. The five Settings images show the General, Wave timer, Compass, Recipe tracker and Codex tabs in version 1.2.0. They were captured directly from the overlay’s owned Windows dialogs with isolated defaults.

Run the candidate executable with --self-test-dialogs and a fresh private output directory, then pass that directory to site/prepare-settings-images.py with --dialog-dir. For the tray and menu images, use --source-dir with the original clipboard captures. Keep originals outside the served website under ignored local/ storage.

Still useful: an Update 2 single-recipe panel to illustrate the version difference.

Feature close-ups now use exact pixel crops in `website/assets/`, rather than hiding full captures with CSS. Full in-game overview and arrival images remain because the page explicitly displays and links them. Unused captures and duplicate originals are archived under ignored `local/`; they are excluded from source publication and the Pages package.
