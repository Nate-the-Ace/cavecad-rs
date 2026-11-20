"""CaveCAD Capture station cards: CURRENT, FORWARD and REAR as AprilTag 16h5 glyphs #2, #12, #26.

Those three were picked from the 30 tags of the family as the most different: any two differ in at least 8 of
their 16 squares in any rotation, and none looks like itself turned (at least 6 squares differ), so a card
is never read as another card or upside down. The detector must accept only these three ids.

    python3 -I make_cards.py OUT_DIR        (needs opencv-python-headless, numpy, pillow)

Writes one PDF per size, US Letter, print at 100% (actual size), laminate, cut on the grey lines.
"""
import sys
import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont

DPI = 600
CARDS = [(2, "CURRENT", "on the station you capture from"), (12, "FORWARD", "on the next station"), (26, "REAR", "on the previous station")]
# name, card w x h mm, black tag square mm (6 cells), per page (cols, rows)
SIZES = [("index-4x6", 152.4, 101.6, 72.0, (1, 2)), ("business-card", 85.0, 55.0, 40.0, (2, 3))]
DICT = cv2.aruco.getPredefinedDictionary(cv2.aruco.DICT_APRILTAG_16h5)


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


def card(tid, word, note, w_mm, h_mm, tag_mm):
    W, H, T = mm(w_mm), mm(h_mm), mm(tag_mm)
    quiet = (H - T) // 2                      # white all round the tag: at least one cell
    assert quiet >= T // 6, "quiet zone smaller than one cell"
    img = Image.new("L", (W, H), 255)
    tag = Image.fromarray(cv2.aruco.generateImageMarker(DICT, tid, 600, borderBits=1)).resize((T, T), Image.NEAREST)
    img.paste(tag, (quiet, quiet))
    dr = ImageDraw.Draw(img)
    x0, room = quiet + T + quiet, W - (quiet + T + quiet) - mm(3)
    size = round(H * 0.22)
    while dr.textlength(word, font=font(size)) > room:
        size -= 4
    dr.text((x0, round(H * 0.18)), word, font=font(size), fill=0)
    small = font(round(H * 0.06), False)
    lines = []
    for wd in note.split():
        if lines and dr.textlength(lines[-1] + " " + wd, font=small) <= room:
            lines[-1] += " " + wd
        else:
            lines.append(wd)
    for i, line in enumerate(lines):
        dr.text((x0, round(H * 0.50) + i * round(H * 0.075)), line, font=small, fill=0)
    dr.text((x0, H - round(H * 0.15)), f"CaveCAD  #{tid}", font=font(round(H * 0.05), False), fill=0)
    dr.rectangle([0, 0, W - 1, H - 1], outline=170, width=mm(0.3))  # cut line, outside the quiet zone
    return img


def sheet(out, name, w, h, t, grid):
    cols, rows = grid
    imgs = [card(i, wd, n, w, h, t) for i, wd, n in CARDS]
    per = cols * rows
    order = imgs + imgs[: (-len(imgs)) % per]  # fill the last page with spares
    pages = []
    for p in range(0, len(order), per):
        page = Image.new("L", (mm(215.9), mm(279.4)), 255)
        gx = (mm(215.9) - cols * mm(w)) // (cols + 1)
        gy = (mm(279.4) - mm(12) - rows * mm(h)) // (rows + 1)
        for k, im in enumerate(order[p : p + per]):
            c, r = k % cols, k // cols
            page.paste(im, (gx + c * (mm(w) + gx), mm(12) + gy + r * (mm(h) + gy)))
        ImageDraw.Draw(page).text((mm(12), mm(6)), f"CaveCAD Capture station cards, {w:g} x {h:g} mm. Print at 100% (actual size, no fit-to-page), laminate, cut on the grey lines.", font=font(mm(3), False), fill=0)
        pages.append(page.convert("RGB"))
    pages[0].save(f"{out}/CaveCAD-station-cards-{name}.pdf", save_all=True, append_images=pages[1:], resolution=DPI)
    for (i, wd, _), im in zip(CARDS, imgs):
        im.save(f"{out}/{name}-{wd.lower()}.png", dpi=(DPI, DPI))


if __name__ == "__main__":
    for s in SIZES:
        sheet(sys.argv[1], *s)
    print("ok")
