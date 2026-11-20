"""CaveCAD Capture station cards: CURRENT, FORWARD and REAR as AprilTag 16h5 glyphs #2, #12, #26.

Those three were picked from the 30 tags of the family as the most different: any two differ in at least 8 of
their 16 squares in any rotation, and none looks like itself turned (at least 6 squares differ), so a card
is never read as another card or upside down. The detector must accept only these three ids.

    python3 -I make_cards.py OUT_DIR        (needs opencv-python-headless, numpy, pillow)

Writes one two-page PDF per card and size on a square page: page 1 the glyph alone, centred; page 2 the
words, centred. Print double-sided at 100% (actual size) and trim smaller before laminating, but never
inside the dotted square: that is one cell of white round the glyph, which the reader needs.
"""
import sys
import cv2
from PIL import Image, ImageDraw, ImageFont

DPI = 600
CARDS = [(2, "CURRENT", "Put on the station you capture from."), (12, "FORWARD", "Put on the next station."), (26, "REAR", "Put on the previous station.")]
SIZES = [("large", 127.0, 76.2), ("small", 76.2, 40.8)]  # name, square page side mm, black glyph square mm (6 cells)
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


def front(tid, side, glyph):
    """The glyph alone, centred on a square page, with a dotted square one cell outside it: the smallest trim."""
    S, T = mm(side), mm(glyph)
    cell = T // 6
    T = cell * 6
    img = Image.new("L", (S, S), 255)
    tag = Image.fromarray(cv2.aruco.generateImageMarker(DICT, tid, 600, borderBits=1)).resize((T, T), Image.NEAREST)
    o = (S - T) // 2
    img.paste(tag, (o, o))
    dr = ImageDraw.Draw(img)
    a, b, dash = o - cell, o + T + cell, max(1, cell // 4)
    for p in range(a, b, dash * 2):  # light grey dots: outside the white cell the reader needs
        q = min(p + dash, b)
        for line in ([(p, a), (q, a)], [(p, b), (q, b)], [(a, p), (a, q)], [(b, p), (b, q)]):
            dr.line(line, fill=190, width=mm(0.2))
    return img


def back(tid, word, note, side, glyph):
    """The words, centred inside the same smallest-trim square as the front."""
    S = mm(side)
    box = mm(glyph) * 8 // 6        # the dotted square's side
    o = (S - box) // 2
    img = Image.new("L", (S, S), 255)
    dr = ImageDraw.Draw(img)
    size = round(box * 0.2)
    while dr.textlength(word, font=font(size)) > box * 0.92:
        size -= 2
    big = font(size)
    dr.text(((S - dr.textlength(word, font=big)) / 2, o + box * 0.18), word, font=big, fill=0)
    small = font(round(box * 0.065), False)
    lines = []
    for wd in note.split():
        if lines and dr.textlength(lines[-1] + " " + wd, font=small) <= box * 0.9:
            lines[-1] += " " + wd
        else:
            lines.append(wd)
    for i, line in enumerate(lines):
        dr.text(((S - dr.textlength(line, font=small)) / 2, o + box * (0.52 + i * 0.09)), line, font=small, fill=0)
    tiny = font(round(box * 0.045), False)
    tag = f"CaveCAD Capture  #{tid}"
    dr.text(((S - dr.textlength(tag, font=tiny)) / 2, o + box * 0.84), tag, font=tiny, fill=0)
    return img


if __name__ == "__main__":
    out = sys.argv[1]
    for size, side, glyph in SIZES:
        for tid, word, note in CARDS:
            pages = [front(tid, side, glyph).convert("RGB"), back(tid, word, note, side, glyph).convert("RGB")]
            pages[0].save(f"{out}/{word}-{size}.pdf", save_all=True, append_images=pages[1:], resolution=DPI)
    print("ok")
