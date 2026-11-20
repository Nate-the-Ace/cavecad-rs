"""Survey laser target card: front, a big arrow whose tip is the card's corner (put that corner on the station
mark); back, a bullseye to shoot the laser at. Square page, card square shown by corner crop marks.

    python3 -I make_target.py OUT_DIR       (needs pillow)

Print double-sided at 100% (actual size), cut on the crop marks, laminate (matte, so the laser dot shows).
"""
import math
import sys
from PIL import Image, ImageDraw, ImageFont

DPI = 600
PAGE = 127.0   # 5 in page
CARD = 101.6   # 4 in card


def mm(v):
    return round(v / 25.4 * DPI)


def font(px, bold=True):
    name = "Arial Bold.ttf" if bold else "Arial.ttf"
    for f in [f"/System/Library/Fonts/Supplemental/{name}", "/System/Library/Fonts/Helvetica.ttc"]:
        try:
            return ImageFont.truetype(f, px)
        except OSError:
            pass
    return ImageFont.load_default()


def crop_marks(dr, o, c):
    L, g = mm(6), mm(1.5)
    for x in (o, o + c):
        for y in (o, o + c):
            sx = -1 if x == o else 1
            sy = -1 if y == o else 1
            dr.line([(x + sx * g, y), (x + sx * (g + L), y)], fill=0, width=mm(0.2))
            dr.line([(x, y + sy * g), (x, y + sy * (g + L))], fill=0, width=mm(0.2))


def front():
    P, C = mm(PAGE), mm(CARD)
    o = (P - C) // 2
    img = Image.new("L", (P, P), 255)
    dr = ImageDraw.Draw(img)
    # arrow from the top-right area down to the bottom-left corner of the card, tip exactly on the corner
    tip = (o, o + C)
    ang = math.radians(-45)                                  # pointing down-left
    ux, uy = -math.cos(math.radians(45)), math.sin(math.radians(45))   # unit vector towards the tip
    head_len, head_w, shaft_w = mm(38), mm(40), mm(16)
    tail = (tip[0] - ux * mm(160), tip[1] - uy * mm(160))     # runs off the far corner; clipped to the card below
    base = (tip[0] - ux * head_len, tip[1] - uy * head_len)
    nx, ny = -uy, ux                                          # normal
    def pt(p, s):
        return (p[0] + nx * s, p[1] + ny * s)
    shaft = [pt(tail, shaft_w / 2), pt(base, shaft_w / 2), pt(base, -shaft_w / 2), pt(tail, -shaft_w / 2)]
    head = [tip, pt(base, head_w / 2), pt(base, -head_w / 2)]
    dr.polygon(shaft, fill=0)
    dr.polygon(head, fill=0)
    # clip anything outside the card to white (the tail end), so the card edge stays clean
    mask = Image.new("L", (P, P), 0)
    ImageDraw.Draw(mask).rectangle([o, o, o + C, o + C], fill=255)
    img = Image.composite(img, Image.new("L", (P, P), 255), mask)
    dr = ImageDraw.Draw(img)
    f = font(mm(6.5))
    f2 = font(mm(3.6), False)
    t = "corner on the mark"
    x = o + C - mm(5) - max(dr.textlength("STATION", font=f), dr.textlength(t, font=f2))
    dr.text((x, o + C - mm(22)), "STATION", font=f, fill=0)       # bottom right: clear of the arrow
    dr.text((x, o + C - mm(12)), t, font=f2, fill=0)
    crop_marks(dr, o, C)
    return img


def back():
    P, C = mm(PAGE), mm(CARD)
    o = (P - C) // 2
    cx = cy = P // 2
    img = Image.new("L", (P, P), 255)
    dr = ImageDraw.Draw(img)
    R = C // 2 - mm(6)
    # thin rings every 5 mm, heavier every 10 mm, so a laser dot shows on white anywhere and its offset reads off
    r, k = mm(5), 1
    while r <= R:
        w = mm(0.9) if k % 2 == 0 else mm(0.35)
        dr.ellipse([cx - r, cy - r, cx + r, cy + r], outline=0, width=w)
        r, k = mm(5) * (k + 1), k + 1
    dr.line([(cx - R, cy), (cx + R, cy)], fill=0, width=mm(0.35))
    dr.line([(cx, cy - R), (cx, cy + R)], fill=0, width=mm(0.35))
    c = mm(2.5)                                               # centre: a small ring, white inside
    dr.ellipse([cx - c, cy - c, cx + c, cy + c], fill=255, outline=0, width=mm(0.8))
    dr.ellipse([cx - mm(0.6), cy - mm(0.6), cx + mm(0.6), cy + mm(0.6)], fill=0)
    f = font(mm(2.6), False)
    for i in (1, 2, 3, 4):                                    # just outside each bold ring, above the line
        dr.text((cx + mm(10) * i + mm(0.9), cy - mm(3.6)), str(i), font=f, fill=0)
    note = "rings every 5 mm, bold every 1 cm"
    dr.text((cx - dr.textlength(note, font=f) / 2, o + C - mm(4.5)), note, font=f, fill=0)
    crop_marks(dr, o, C)
    return img


if __name__ == "__main__":
    pages = [front().convert("RGB"), back().convert("RGB")]
    pages[0].save(f"{sys.argv[1]}/LASER-TARGET.pdf", save_all=True, append_images=pages[1:], resolution=DPI)
    print("ok")
