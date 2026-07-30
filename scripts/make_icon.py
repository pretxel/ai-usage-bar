#!/usr/bin/env python3
"""Render the AI Usage Bar app icon (1024x1024) with Pillow.

A dark macOS-style rounded tile with a soft violet glow and three ascending
bar-chart bars (violet / teal / amber) — the app's logo motif.
Run: python3 scripts/make_icon.py  ->  scripts/app-icon-1024.png
"""
from PIL import Image, ImageDraw, ImageFilter

S = 4          # supersample factor
BASE = 1024
W = BASE * S


def sc(v):
    return int(round(v * S))


def vgrad(w, h, c1, c2):
    """Vertical gradient RGBA image."""
    h = max(h, 1)
    col = Image.new("RGB", (1, h))
    px = col.load()
    for y in range(h):
        t = y / (h - 1) if h > 1 else 0.0
        px[0, y] = tuple(int(c1[i] + (c2[i] - c1[i]) * t) for i in range(3))
    return col.resize((max(w, 1), h)).convert("RGBA")


def rrect_mask(size, box, radius):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=radius, fill=255)
    return m


def hx(s):
    s = s.lstrip("#")
    return tuple(int(s[i:i + 2], 16) for i in (0, 2, 4))


def draw_rocket():
    """Upright starship sprite (nose up), RGBA, in supersampled units."""
    LW, LH = 320, 760
    spr = Image.new("RGBA", (sc(LW), sc(LH)), (0, 0, 0, 0))
    cx = 160

    # exhaust trail — fading puffs straight below the tail (becomes the
    # motion trail once the whole sprite is rotated)
    trail = Image.new("RGBA", (sc(LW), sc(LH)), (0, 0, 0, 0))
    td = ImageDraw.Draw(trail)
    ty, r, a = 602.0, 46.0, 165.0
    while ty < 748 and r > 4:
        td.ellipse([sc(cx - r), sc(ty - r), sc(cx + r), sc(ty + r)],
                   fill=(255, 236, 200, int(a)))
        ty += 26
        r *= 0.76
        a *= 0.68
    trail = trail.filter(ImageFilter.GaussianBlur(sc(11)))
    spr.alpha_composite(trail)

    d = ImageDraw.Draw(spr)

    # flame (under the body)
    d.polygon([(sc(118), sc(468)), (sc(202), sc(468)), (sc(180), sc(548)),
               (sc(160), sc(600)), (sc(140), sc(548))], fill=(245, 158, 11, 255))
    d.polygon([(sc(138), sc(472)), (sc(182), sc(472)), (sc(168), sc(536)),
               (sc(160), sc(570)), (sc(152), sc(536))], fill=(254, 230, 138, 255))

    # fins (violet) — drawn before body so the body overlaps their roots
    d.polygon([(sc(112), sc(366)), (sc(60), sc(478)), (sc(112), sc(450))],
              fill=hx("#7c3aed") + (255,))
    d.polygon([(sc(208), sc(366)), (sc(260), sc(478)), (sc(208), sc(450))],
              fill=hx("#7c3aed") + (255,))

    # body — white→silver gradient capsule
    bt, bb = 150, 474
    body = vgrad(sc(LW), sc(bb - bt), hx("#ffffff"), hx("#c4ccda"))
    bmask = Image.new("L", (sc(LW), sc(bb - bt)), 0)
    ImageDraw.Draw(bmask).rounded_rectangle(
        [sc(106), 0, sc(214), sc(bb - bt) - 1], radius=sc(54), fill=255)
    spr.paste(body, (0, sc(bt)), bmask)

    d = ImageDraw.Draw(spr)
    # nose cone + accent tip
    d.polygon([(sc(160), sc(38)), (sc(106), sc(164)), (sc(214), sc(164))],
              fill=hx("#eef1f6") + (255,))
    d.polygon([(sc(160), sc(38)), (sc(138), sc(98)), (sc(182), sc(98))],
              fill=(245, 158, 11, 255))

    # window — teal porthole with rim + glint
    d.ellipse([sc(cx - 44), sc(232 - 44), sc(cx + 44), sc(232 + 44)],
              fill=hx("#e2e8f2") + (255,))
    d.ellipse([sc(cx - 33), sc(232 - 33), sc(cx + 33), sc(232 + 33)],
              fill=hx("#22d3ee") + (255,))
    d.ellipse([sc(cx - 30), sc(232 - 30), sc(cx + 6), sc(232 - 2)],
              fill=(255, 255, 255, 95))

    # soft left-edge highlight on the body
    hl = Image.new("RGBA", (sc(LW), sc(LH)), (0, 0, 0, 0))
    ImageDraw.Draw(hl).rounded_rectangle(
        [sc(122), sc(168), sc(140), sc(452)], radius=sc(10),
        fill=(255, 255, 255, 130))
    hl = hl.filter(ImageFilter.GaussianBlur(sc(7)))
    spr.alpha_composite(hl)

    return spr


def main():
    canvas = Image.new("RGBA", (W, W), (0, 0, 0, 0))

    # --- rounded tile -----------------------------------------------------
    inset = 88
    tile_box = [sc(inset), sc(inset), sc(BASE - inset), sc(BASE - inset)]
    radius = sc(196)

    tile = vgrad(W, W, hx("#1a1230"), hx("#0a0711"))

    # soft violet glow, upper-left
    glow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gx, gy, gr = sc(360), sc(300), sc(560)
    gd.ellipse([gx - gr, gy - gr, gx + gr, gy + gr], fill=(139, 92, 246, 150))
    # faint teal counter-glow, lower-right
    tx, ty, tr = sc(760), sc(820), sc(460)
    gd.ellipse([tx - tr, ty - tr, tx + tr, ty + tr], fill=(56, 189, 248, 70))
    glow = glow.filter(ImageFilter.GaussianBlur(sc(150)))
    tile = Image.alpha_composite(tile, glow)

    mask = rrect_mask((W, W), tile_box, radius)
    canvas.paste(tile, (0, 0), mask)

    # subtle top highlight on the tile edge for depth
    hl = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    hd = ImageDraw.Draw(hl)
    hd.rounded_rectangle(
        [tile_box[0], tile_box[1], tile_box[2], tile_box[1] + sc(6)],
        radius=sc(6), fill=(255, 255, 255, 38),
    )
    hl = hl.filter(ImageFilter.GaussianBlur(sc(3)))
    canvas = Image.alpha_composite(canvas, Image.composite(hl, Image.new("RGBA", (W, W)), mask))

    # --- bars -------------------------------------------------------------
    bars = [
        # (x, top_y, ['#topcolor', '#bottomcolor'])
        (214, 556, ["#a78bfa", "#7c3aed"]),
        (420, 496, ["#22d3ee", "#0ea5e9"]),
        (626, 436, ["#fbbf24", "#d97706"]),
    ]
    bw = 150
    baseline = 796
    bar_radius = 40

    # drop shadow under the bars
    shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    for x, top, _ in bars:
        sd.rounded_rectangle(
            [sc(x), sc(top + 18), sc(x + bw), sc(baseline + 18)],
            radius=sc(bar_radius), fill=(0, 0, 0, 130),
        )
    shadow = shadow.filter(ImageFilter.GaussianBlur(sc(22)))
    canvas = Image.alpha_composite(canvas, Image.composite(shadow, Image.new("RGBA", (W, W)), mask))

    # the bars themselves
    for x, top, cols in bars:
        bh = baseline - top
        grad = vgrad(sc(bw), sc(bh), hx(cols[0]), hx(cols[1]))
        bmask = Image.new("L", (sc(bw), sc(bh)), 0)
        ImageDraw.Draw(bmask).rounded_rectangle(
            [0, 0, sc(bw) - 1, sc(bh) - 1], radius=sc(bar_radius), fill=255
        )
        # glossy top highlight
        gloss = Image.new("RGBA", (sc(bw), sc(bh)), (0, 0, 0, 0))
        ImageDraw.Draw(gloss).rounded_rectangle(
            [sc(14), sc(12), sc(bw - 14), sc(46)], radius=sc(18), fill=(255, 255, 255, 60)
        )
        grad = Image.alpha_composite(grad, gloss)
        canvas.paste(grad, (sc(x), sc(top)), bmask)

    # --- starship ---------------------------------------------------------
    rocket = draw_rocket().rotate(-37, expand=True, resample=Image.BICUBIC)
    # nudge a hair smaller so it sits cleanly inside the tile
    rw, rh = int(rocket.width * 0.92), int(rocket.height * 0.92)
    rocket = rocket.resize((rw, rh), Image.LANCZOS)
    cxp, cyp = sc(602), sc(404)            # target centre of the rocket
    px, py = cxp - rw // 2, cyp - rh // 2

    # drop shadow cast onto the tile
    shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    sh = Image.new("RGBA", rocket.size, (0, 0, 0, 0))
    sh.paste((0, 0, 0, 150), (0, 0), rocket.split()[3])
    sh = sh.filter(ImageFilter.GaussianBlur(sc(15)))
    shadow.paste(sh, (px + sc(16), py + sc(22)), sh)
    canvas.alpha_composite(Image.composite(shadow, Image.new("RGBA", (W, W)), mask))

    canvas.paste(rocket, (px, py), rocket)

    # --- downscale & save -------------------------------------------------
    out = canvas.resize((BASE, BASE), Image.LANCZOS)
    out.save("scripts/app-icon-1024.png")
    print("wrote scripts/app-icon-1024.png", out.size)


if __name__ == "__main__":
    main()
