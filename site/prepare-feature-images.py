"""Reproduce exact public feature crops from ignored originals and approved hero images."""
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "site/website/assets"
ORIGINALS = ROOT / "local/website-screenshot-originals/product-crops"

jobs = [
    (ASSETS / "in-game-overview.png", "timer-stable.png", (8, 8, 230, 94)),
    (ASSETS / "in-game-overview.png", "compass-factory.png", (1060, 0, 1500, 93)),
    (ASSETS / "personal-marker-arrival.png", "compass-arrived.png", (1206, 40, 1354, 93)),
    (ASSETS / "in-game-overview.png", "recipe-tracking.png", (2248, 446, 2558, 996)),
    (ORIGINALS / "compass-clear.png", "compass-clear.png", (1060, 0, 1500, 93)),
]
jobs += [(ORIGINALS / name, name, (8, 8, 230, 94)) for name in ["timer-pre-wave.png", "timer-approaching.png", "timer-incinerating.png", "timer-cooling.png", "timer-growback.png"]]
for source, name, bounds in jobs:
    with Image.open(source) as image:
        crop = image.crop(bounds)
        clean = Image.frombytes(crop.mode, crop.size, crop.tobytes())
        clean.save(ASSETS / name)
        with Image.open(ASSETS / name) as result:
            assert result.tobytes() == crop.tobytes()
            assert not result.info
print(f"Prepared {len(jobs)} exact feature crops without source metadata.")
