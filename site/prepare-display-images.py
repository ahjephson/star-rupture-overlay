"""Encode smaller display images while retaining the linked full-size PNG captures."""
from pathlib import Path
from PIL import Image

ASSETS = Path(__file__).resolve().parent / "website/assets"
for name in ["in-game-overview", "personal-marker-arrival"]:
    with Image.open(ASSETS / f"{name}.png") as source:
        display = source.convert("RGB")
        display.thumbnail((1600, 1600), Image.Resampling.LANCZOS)
        display.save(ASSETS / f"{name}.webp", quality=88, method=6)
    print(f"{name}: {display.width} x {display.height}, {(ASSETS / f'{name}.webp').stat().st_size:,} bytes")
