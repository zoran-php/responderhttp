#!/usr/bin/env python3
"""Store listing art, generated from app-icon.png.

Every promotional asset the Store listing offers, drawn from the one 1024 px
icon the package already uses, so the art cannot drift from the app. Run from
the repository root:

    python tools/build-store-art.py

Palette: the gradient runs from the manifest's BackgroundColor (#3B4252) to the
web pages' background (#181D24), and the accent is the icon's own blue, so the
tile, the listing and the site agree.

Text assets use Lato — the site sets Inter, which is not installed here; Lato
is the nearest humanist sans available and reads the same at these sizes.
"""
import os

from PIL import Image, ImageDraw, ImageFont

ICON_FILE = "app-icon.png"
OUT_DIR = os.path.join("store", "art")

TOP = (0x3B, 0x42, 0x52)
BOTTOM = (0x18, 0x1D, 0x24)
ACCENT = (0x88, 0xC0, 0xD0)
TEXT = (0xF1, 0xF5, 0xF9)
MUTED = (0x94, 0xA3, 0xB8)

FONT_BOLD = "/usr/share/fonts/truetype/lato/Lato-Black.ttf"
FONT_REGULAR = "/usr/share/fonts/truetype/lato/Lato-Regular.ttf"

WORDMARK = "ResponderHTTP"
TAGLINE = "Desktop API client"


def gradient(size, top=TOP, bottom=BOTTOM):
    """A vertical ramp, drawn one pixel wide and stretched: at 4K a per-row
    loop over the full width is slow for no visible gain."""
    width, height = size
    column = Image.new("RGB", (1, height))
    for y in range(height):
        t = y / max(height - 1, 1)
        column.putpixel((0, y), tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3)))
    return column.resize(size, Image.Resampling.BICUBIC)


def glow(canvas, centre, radius, colour=ACCENT, strength=0.16):
    """A soft accent halo behind the icon. Drawn small and scaled up, which is
    a cheap blur and smooth enough at these sizes."""
    small = 128
    layer = Image.new("L", (small, small), 0)
    draw = ImageDraw.Draw(layer)
    steps = 24
    for step in range(steps, 0, -1):
        r = small / 2 * step / steps
        value = round(255 * strength * (1 - step / steps) ** 2)
        draw.ellipse(
            [small / 2 - r, small / 2 - r, small / 2 + r, small / 2 + r], fill=value
        )
    layer = layer.resize((radius * 2, radius * 2), Image.Resampling.BICUBIC)
    tint = Image.new("RGB", layer.size, colour)
    canvas.paste(tint, (centre[0] - radius, centre[1] - radius), layer)


def paste_icon(canvas, icon, centre, side):
    resized = icon.resize((side, side), Image.Resampling.LANCZOS)
    canvas.paste(resized, (centre[0] - side // 2, centre[1] - side // 2), resized)


def text_width(draw, text, font):
    box = draw.textbbox((0, 0), text, font=font)
    return box[2] - box[0]


def save(canvas, name):
    path = os.path.join(OUT_DIR, name)
    canvas.save(path, "PNG", optimize=True)
    print(f"{path:52} {canvas.size[0]}x{canvas.size[1]}  {os.path.getsize(path) // 1024} KB")


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    icon = Image.open(ICON_FILE).convert("RGBA")

    # --- Store display logo, 71x71. Icon only: at this size a wordmark is mud.
    logo = gradient((71, 71))
    paste_icon(logo, icon, (35, 35), 60)
    save(logo, "logo-71x71.png")

    # --- Super hero art, 3840x2160. No wordmark and nothing in the middle:
    # the Store lays its own title and buttons over hero placements, so this
    # keeps the subject left and the right two thirds quiet.
    hero = gradient((3840, 2160))
    glow(hero, (1150, 1080), 900)
    paste_icon(hero, icon, (1150, 1080), 900)
    save(hero, "super-hero-3840x2160.png")

    # --- Titled hero art, 1920x1080. The titled variant is the one that
    # carries the name, so here the wordmark is the point.
    titled = gradient((1920, 1080))
    glow(titled, (560, 540), 380)
    paste_icon(titled, icon, (560, 540), 380)
    draw = ImageDraw.Draw(titled)
    name_font = ImageFont.truetype(FONT_BOLD, 128)
    tag_font = ImageFont.truetype(FONT_REGULAR, 56)
    draw.text((900, 448), WORDMARK, font=name_font, fill=TEXT)
    draw.text((906, 600), TAGLINE, font=tag_font, fill=MUTED)
    save(titled, "titled-hero-1920x1080.png")

    # --- Branded key art, 584x800. Portrait: icon above, name below.
    key = gradient((584, 800))
    glow(key, (292, 330), 230)
    paste_icon(key, icon, (292, 330), 300)
    draw = ImageDraw.Draw(key)
    name_font = ImageFont.truetype(FONT_BOLD, 58)
    tag_font = ImageFont.truetype(FONT_REGULAR, 27)
    draw.text(((584 - text_width(draw, WORDMARK, name_font)) / 2, 560), WORDMARK, font=name_font, fill=TEXT)
    draw.text(((584 - text_width(draw, TAGLINE, tag_font)) / 2, 636), TAGLINE, font=tag_font, fill=MUTED)
    save(key, "branded-key-art-584x800.png")

    # --- Featured promotional square art, 1080x1080. No wordmark and no
    # tagline: Microsoft's rule for this slot is that it must not include the
    # product's title. Icon centred, and larger than it would be under text.
    square = gradient((1080, 1080))
    glow(square, (540, 540), 400)
    paste_icon(square, icon, (540, 540), 620)
    save(square, "promotional-square-1080x1080.png")


if __name__ == "__main__":
    main()
