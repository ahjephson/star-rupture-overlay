"""Create exact, privacy-safe UI crops from owner-supplied screenshots.

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
    ("settings-general", "4b8bb601-21da-4a6d-8fdd-26d68a0f0f32", (20, 23, 553, 522), "rounded"),
    ("settings-wave-timer", "0c946f53-335c-415e-bf97-4b16a393f585", (29, 23, 563, 357), "rounded"),
    ("settings-compass", "2109487f-f132-4173-bfe6-8c275478e90a", (20, 29, 554, 488), "rounded"),
    ("settings-recipe-tracker", "4fa32e41-667a-4fb5-851b-84737bf82035", (27, 23, 561, 602), "rounded"),
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=Path, required=True)
    args = parser.parse_args()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    PRIVATE.mkdir(parents=True, exist_ok=True)
    manifest = []
    for name, identifier, bounds, mode in CAPTURES:
        source = args.source_dir / f"codex-clipboard-{identifier}.png"
        shutil.copyfile(source, PRIVATE / source.name)
        with Image.open(source) as original:
            cropped = original.convert("RGBA").crop(bounds)
        mask = Image.new("L", cropped.size, 255)
        draw = ImageDraw.Draw(mask)
        if mode == "rounded":
            mask.paste(0, (0, 0, *cropped.size))
            draw.rounded_rectangle((0, 0, cropped.width - 1, cropped.height - 1), radius=8, fill=255)
        elif mode == "tray":
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
    # Retain the duplicate Compass capture privately too, without publishing another asset.
    duplicate = args.source_dir / "codex-clipboard-f77531d4-aed3-40b9-abc7-740d3695dd21.png"
    shutil.copyfile(duplicate, PRIVATE / duplicate.name)
    (PRIVATE / "crop-manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"Prepared and verified {len(manifest)} exact UI crops. Originals and provenance stay private.")


if __name__ == "__main__":
    main()
