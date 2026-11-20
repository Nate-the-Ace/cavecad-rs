"""CaveCAD Capture station cards: CURRENT, FORWARD and REAR as AprilTag 16h5 glyphs #2, #12, #26.

Those three were picked from the 30 tags of the family as the most different: any two differ in at least 8 of
their 16 squares in any rotation, and none looks like itself turned (at least 6 squares differ), so a card
is never read as another card or upside down. The detector must accept only these three ids.

    python3 -I make_cards.py OUT_DIR        (needs opencv-python-headless, numpy, pillow)

Writes one two-page PDF per size, US Letter: page 1 the fronts, page 2 the backs (the same card, placed so a
duplex print flipped on the long edge lines each back up behind its front). Print double-sided at 100%
(actual size), laminate, cut on the grey lines. Every card then reads from either side.
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


PAGE_W, PAGE_H = 215.9, 279.4


def layout(w, h):
    """Card slots on a Letter page as (x mm, y mm, turned): three 4x6 cards fit as two turned upright side by
    side over one lying flat; business cards fill a 2 x 3 grid (two full sets)."""
    if w > 120:  # 4x6: [w][w] upright on top, one flat below, centred
        top = (PAGE_W - 2 * h) / 2
        y0 = (PAGE_H - (w + h)) / 2
        return [(top, y0, True), (top + h, y0, True), ((PAGE_W - w) / 2, y0 + w, False)]
    gx, gy = (PAGE_W - 2 * w) / 3, (PAGE_H - 3 * h) / 4
    return [(gx + c * (w + gx), gy + r * (h + gy), False) for r in range(3) for c in range(2)]


def sheet(out, name, w, h, t, _grid):
    imgs = [card(i, wd, n, w, h, t) for i, wd, n in CARDS]
    slots = layout(w, h)
    pages = []
    for back in (False, True):
        page = Image.new("L", (mm(PAGE_W), mm(PAGE_H)), 255)
        for k, (x, y, turned) in enumerate(slots):
            im = imgs[k % len(imgs)]
            cw, ch = (h, w) if turned else (w, h)
            if turned:
                im = im.rotate(90, expand=True)
            if back:  # flipped on the long edge: the back sits mirrored left to right
                x = PAGE_W - x - cw
            page.paste(im, (mm(x), mm(y)))
        title = "BACKS" if back else "FRONTS"
        ImageDraw.Draw(page).text((mm(8), mm(3)), f"CaveCAD Capture station cards {w:g} x {h:g} mm - {title}. Print double-sided, flip on long edge, 100% (actual size).", font=font(mm(2.6), False), fill=0)
        pages.append(page.convert("RGB"))
    pages[0].save(f"{out}/CaveCAD-station-cards-{name}.pdf", save_all=True, append_images=pages[1:], resolution=DPI)
    for (i, wd, _), im in zip(CARDS, imgs):
        im.save(f"{out}/{name}-{wd.lower()}.png", dpi=(DPI, DPI))


if __name__ == "__main__":
    for s in SIZES:
        sheet(sys.argv[1], *s)
    print("ok")
