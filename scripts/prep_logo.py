#!/usr/bin/env python3
"""Turn the supplied Token Tracker logo into clean icon sources.

- trims the white border around the rounded tile
- squares + rounds the corners to transparency (so the macOS icon has no
  white corners)
Outputs:
  scripts/app-icon-source.png   (1024, for `tauri icon`)
  public/logo.png               (256, in-app brand mark / favicon)
"""
import sys
from PIL import Image, ImageChops, ImageDraw

SRC = sys.argv[1] if len(sys.argv) > 1 else \
    "/Users/edselserrano/Downloads/Gemini_Generated_Image_u6i02du6i02du6i0.png"


def rounded(im, radius_frac):
    s = im.size[0]
    mask = Image.new("L", im.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, s - 1, s - 1], radius=int(s * radius_frac), fill=255)
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    out.paste(im, (0, 0), mask)
    return out


def main():
    im = Image.open(SRC).convert("RGBA")

    # trim near-white border via bbox of the non-white content
    rgb = im.convert("RGB")
    bg = Image.new("RGB", im.size, (255, 255, 255))
    diff = ImageChops.difference(rgb, bg).convert("L").point(
        lambda p: 255 if p > 18 else 0)
    bbox = diff.getbbox()
    if bbox:
        im = im.crop(bbox)

    # square it on transparent canvas
    w, h = im.size
    s = max(w, h)
    sq = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    sq.paste(im, ((s - w) // 2, (s - h) // 2))

    master = sq.resize((1024, 1024), Image.LANCZOS)
    master = rounded(master, 0.205)
    master.save("scripts/app-icon-source.png")

    small = master.resize((256, 256), Image.LANCZOS)
    small.save("public/logo.png")
    print("wrote scripts/app-icon-source.png (1024) + public/logo.png (256)")


if __name__ == "__main__":
    main()
