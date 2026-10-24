"""Convert the owner-supplied raster source to a multi-resolution Windows icon.

Optional authoring command: Python with Pillow (not needed to build/run the app).
Preserves the supplied composition and alpha; no background replacement or crop.
"""
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "crates/wave-overlay/assets/icon"
SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)

if __name__ == "__main__":
    with Image.open(ASSETS / "source.png") as source:
        assert source.mode == "RGBA" and source.width == source.height
        source.save(ASSETS / "app.ico", format="ICO", sizes=[(s, s) for s in SIZES])
