"""Prepare exact native Settings captures and privacy-safe tray crops.

The owner explicitly approved deterministic cropping and masking. No UI pixels
are generated or retouched. Originals stay in ignored local storage.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "site" / "website" / "assets" / "ui"
PRIVATE = ROOT / "local" / "website-screenshot-originals"
# Coordinates isolate the app UI and exclude other tray icons and desktop/game backgrounds.
CAPTURES = [
    ("tray-waiting", "f1f3a296-57b2-4c2c-ad0d-2cf459dfd5b2", (13, 22, 159, 123), "tray"),
    ("menu-waiting", "365c30fc-c764-4757-9ce8-bc86a9bb5a21", (51, 27, 269, 167), "rectangle"),
    ("menu-connecting", "37e9fd09-2e6b-4ee0-b7ce-e221d0284281", (48, 27, 329, 207), "rectangle"),
    ("menu-connected", "bc542ccc-98cb-4376-a9b6-e794c51deb2a", (55, 27, 286, 207), "rectangle"),
]


def main():
    parser = argparse.ArgumentParser()
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument("--source-dir", type=Path)
    inputs.add_argument("--dialog-dir", type=Path)
    args = parser.parse_args()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    PRIVATE.mkdir(parents=True, exist_ok=True)
    if args.dialog_dir:
        save_settings_captures(args.dialog_dir)
        return
    manifest = []
    for name, identifier, bounds, mode in CAPTURES:
        source = args.source_dir / f"codex-clipboard-{identifier}.png"
        shutil.copyfile(source, PRIVATE / source.name)
        with Image.open(source) as original:
            cropped = original.convert("RGBA").crop(bounds)
        mask = Image.new("L", cropped.size, 255)
        draw = ImageDraw.Draw(mask)
        if mode == "tray":
            mask.paste(0, (0, 0, *cropped.size))
            draw.rounded_rectangle((0, 0, 145, 46), radius=4, fill=255)
            # Keep only the Overlay's selected tray tile; all neighbouring icons are masked out.
            draw.rounded_rectangle((38, 59, 77, 99), radius=5, fill=255)
        cropped.putalpha(mask)
        # Erase hidden RGB as well as alpha, so transparent areas retain no private pixels.
        cleaned = Image.new("RGBA", cropped.size)
        cleaned.paste(cropped, mask=mask)
        path = OUTPUT / f"{name}.png"
        cleaned.save(path)
        with Image.open(path) as saved, Image.open(source) as original:
            expected = original.convert("RGBA").crop(bounds)
            visible_diff = ImageChops.difference(saved.convert("RGB"), expected.convert("RGB"))
            assert not ImageChops.multiply(visible_diff, mask.convert("RGB")).getbbox(), name
            assert saved.info == {}, f"{name}: output must not retain screenshot metadata"
            for pixel in saved.getdata():
                assert pixel[3] != 0 or pixel[:3] == (0, 0, 0), f"{name}: hidden private RGB"
        manifest.append({"asset": path.name, "source": source.name, "crop": bounds, "mask": mode,
                         "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    (PRIVATE / "crop-manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"Prepared and verified {len(manifest)} exact UI crops. Originals and provenance stay private.")


def save_settings_captures(directory):
    # The owned-window dialog diagnostic already isolates the window from the desktop.
    captures = {
        "settings.bmp": "general",
        "settings-wave.bmp": "wave",
        "settings-compass.bmp": "compass",
        "settings-recipe.bmp": "recipe",
        "settings-codex.bmp": "codex",
    }
    for source, tab in captures.items():
        path = OUTPUT / f"settings-{tab}-v1-2-0.png"
        with Image.open(directory / source) as bitmap:
            image = bitmap.convert("RGB")
        image.save(path, optimize=True)
        with Image.open(path) as saved:
            assert not ImageChops.difference(saved, image).getbbox(), path
            assert saved.info == {}, path
        print(f"{path.name}: {image.width} × {image.height}")


if __name__ == "__main__":
    main()
