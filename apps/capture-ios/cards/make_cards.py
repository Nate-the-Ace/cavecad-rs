"""CaveCAD Capture station cards: CURRENT, FORWARD and REAR as AprilTag 16h5 glyphs #2, #12, #26.

Those three were picked from the 30 tags of the family as the most different: any two differ in at least 8 of
their 16 squares in any rotation, and none looks like itself turned (at least 6 squares differ), so a card
is never read as another card or upside down. The detector must accept only these three ids.

    python3 -I make_cards.py OUT_DIR        (needs opencv-python-headless, numpy, pillow)

Writes one two-page PDF per card and size, the page the size of the card itself: page 1 the glyph alone, as
big as the card allows (one cell of white all round, which the reader needs); page 2 the words. Print
double-sided at 100% (actual size) on card stock of that size, laminate.
"""
import sys
import cv2
from PIL import Image, ImageDraw, ImageFont

DPI = 600
CARDS = [(2, "CURRENT", "Put on the station you capture from."), (12, "FORWARD", "Put on the next station."), (26, "REAR", "Put on the previous station.")]
SIZES = [("4x6", 152.4, 101.6), ("business-card", 85.0, 55.0)]  # name, card w x h mm (landscape)
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


def front(tid, w, h):
    """The glyph alone: 6 cells of black square plus one white cell all round fill the card's height."""
    W, H = mm(w), mm(h)
    cell = H // 8
    T = cell * 6
    img = Image.new("L", (W, H), 255)
    tag = Image.fromarray(cv2.aruco.generateImageMarker(DICT, tid, 600, borderBits=1)).resize((T, T), Image.NEAREST)
    img.paste(tag, ((W - T) // 2, (H - T) // 2))
    return img


def back(tid, word, note, w, h):
    W, H = mm(w), mm(h)
    img = Image.new("L", (W, H), 255)
    dr = ImageDraw.Draw(img)
    size = round(H * 0.32)
    while dr.textlength(word, font=font(size)) > W * 0.86:
        size -= 4
    big = font(size)
    dr.text(((W - dr.textlength(word, font=big)) / 2, H * 0.14), word, font=big, fill=0)
    small = font(round(H * 0.085), False)
    dr.text(((W - dr.textlength(note, font=small)) / 2, H * 0.60), note, font=small, fill=0)
    tiny = font(round(H * 0.06), False)
    tag = f"CaveCAD Capture station card  #{tid}"
    dr.text(((W - dr.textlength(tag, font=tiny)) / 2, H * 0.82), tag, font=tiny, fill=0)
    return img


if __name__ == "__main__":
    out = sys.argv[1]
    for size, w, h in SIZES:
        for tid, word, note in CARDS:
            pages = [front(tid, w, h).convert("RGB"), back(tid, word, note, w, h).convert("RGB")]
            pages[0].save(f"{out}/{word}-{size}.pdf", save_all=True, append_images=pages[1:], resolution=DPI)
    print("ok")
