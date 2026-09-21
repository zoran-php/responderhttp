# tools/build-installer-art.py
#
# Regenerates the NSIS installer graphics from src-tauri/icons/icon.png.
# Run from the repo root:  python tools/build-installer-art.py
#
# Outputs, all into src-tauri/icons/ and all referenced from
# tauri.conf.json's bundle.windows.nsis block:
#   installer.ico / uninstaller.ico   multi-size, 16 -> 256
#   header.bmp / uninstaller-header.bmp   24-bit BMP, 150 x 57
#   sidebar.bmp                       24-bit BMP, 164 x 314
#   tray-16.png ... tray-48.png       the tray icon, one PNG per DPI step,
#                                     embedded by src-tauri/src/desktop/tray.rs
#
# NSIS wants uncompressed 24-bit BMPs at exactly those dimensions; Pillow
# writes BI_RGB for RGB-mode images, which is what that means in practice.
# Requires Pillow and the Poppins font (or edit the two font paths below).
import io
import struct
from PIL import Image, ImageDraw, ImageFont

# The R is lifted as a mask from icon.png rather than redrawn in a font, so
# the installer art carries the same letterform as the app icon rather than a
# lookalike that drifts the moment either one is retouched.
SRC = "src-tauri/icons/icon.png"
OUT = "src-tauri/icons"
POPPINS_BOLD = "/usr/share/fonts/truetype/google-fonts/Poppins-Bold.ttf"
POPPINS_MED = "/usr/share/fonts/truetype/google-fonts/Poppins-Medium.ttf"

# Sampled from icon.png, not guessed.
BG_TOP = (72, 79, 93)
BG_BOTTOM = (37, 42, 53)
FROST = (136, 192, 208)
MUTED = (154, 165, 185)
# nord4. The uninstaller badge keeps the dark plate and only neutralises the
# glyph — muting both washed the mark out at 16 px, where it has least to give.
UNINSTALL_GLYPH = (216, 222, 233)
INK = (46, 52, 64)
SS = 4  # supersample factor


def r_mask():
    """Alpha mask of the R glyph, cropped to its bounding box."""
    im = Image.open(SRC).convert("RGBA")
    w, h = im.size
    mask = Image.new("L", (w, h), 0)
    px, mp = im.load(), mask.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a > 40 and abs(r - FROST[0]) < 60 and abs(g - FROST[1]) < 60 and abs(b - FROST[2]) < 60:
                mp[x, y] = 255
    return mask.crop(mask.getbbox())


GLYPH = r_mask()


def vertical_gradient(size, top, bottom):
    w, h = size
    grad = Image.new("RGB", (1, h))
    d = ImageDraw.Draw(grad)
    for y in range(h):
        t = y / max(1, h - 1)
        d.point((0, y), tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3)))
    return grad.resize((w, h), Image.BICUBIC)


def badge(size, flat=False, glyph_colour=FROST, bg=(BG_TOP, BG_BOTTOM)):
    """The app badge at any size. `flat` drops the gradient and squares the
    corners slightly — at 16 and 20 px a gradient turns to mud and a soft
    corner eats the silhouette.

    Colour and alpha are downsampled as separate full-bleed images rather than
    as one RGBA. Resizing an RGBA whose transparent region carries undefined
    colour drags that colour into the edge pixels, which showed up as a pale
    halo around every rounded corner.
    """
    s = size * SS
    if flat:
        mid = tuple(round((bg[0][i] + bg[1][i]) / 2) for i in range(3))
        plate = Image.new("RGB", (s, s), mid)
    else:
        plate = vertical_gradient((s, s), *bg).convert("RGB")

    # Glyph painted onto the full-bleed plate, so every pixel has a real colour
    # including the ones the corner mask is about to cut away.
    gw, gh = GLYPH.size
    target_h = int(s * (0.70 if flat else 0.66))
    target_w = max(1, round(gw * target_h / gh))
    g = GLYPH.resize((target_w, target_h), Image.LANCZOS)
    ink = Image.new("RGB", (target_w, target_h), glyph_colour)
    plate.paste(ink, ((s - target_w) // 2, (s - target_h) // 2), g)

    radius = int(s * (0.20 if flat else 0.235))
    corner = Image.new("L", (s, s), 0)
    ImageDraw.Draw(corner).rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=255)

    out = Image.new("RGBA", (size, size))
    out.paste(plate.resize((size, size), Image.LANCZOS))
    out.putalpha(corner.resize((size, size), Image.LANCZOS))
    return out


def _unused_write_ico(path, muted=False):
    glyph = UNINSTALL_GLYPH if muted else FROST
    bg = ((88, 94, 108), (58, 63, 74)) if muted else (BG_TOP, BG_BOTTOM)
    frames = []
    for n in (16, 20, 24, 32, 48, 64, 128, 256):
        frames.append(badge(n, flat=n <= 32, glyph_colour=glyph, bg=bg))
    frames[0].save(path, format="ICO",
                   sizes=[(f.width, f.height) for f in frames], append_images=frames[1:])
    return path


def fit_text(text, font_path, target_px):
    """Largest size whose cap height fits target_px."""
    size = target_px
    while size > 4:
        f = ImageFont.truetype(font_path, size)
        box = f.getbbox(text)
        if (box[3] - box[1]) <= target_px:
            return f
        size -= 1
    return ImageFont.truetype(font_path, 6)


def header(path, muted=False, with_wordmark=True):
    W, H = 150, 57
    # White, because MUI draws this strip on ${MUI_BGCOLOR} — a dark bitmap
    # here reads as a sticker pasted onto the wizard.
    im = Image.new("RGB", (W * SS, H * SS), (255, 255, 255))
    b = badge(40 * SS, glyph_colour=UNINSTALL_GLYPH if muted else FROST,
              bg=((88, 94, 108), (58, 63, 74)) if muted else (BG_TOP, BG_BOTTOM))
    right = W * SS - 9 * SS
    if with_wordmark:
        font = fit_text("ResponderHTTP", POPPINS_BOLD, 13 * SS)
        tw = font.getbbox("ResponderHTTP")[2] - font.getbbox("ResponderHTTP")[0]
        bx = right - tw - 8 * SS - b.width
        im.paste(b, (bx, (H * SS - b.height) // 2), b)
        d = ImageDraw.Draw(im)
        d.text((bx + b.width + 8 * SS, H * SS // 2), "ResponderHTTP",
               font=font, fill=MUTED if muted else INK, anchor="lm")
    else:
        im.paste(b, (right - b.width, (H * SS - b.height) // 2), b)
    im.resize((W, H), Image.LANCZOS).save(path)
    return path


def sidebar(path):
    W, H = 164, 314
    im = vertical_gradient((W * SS, H * SS), (62, 69, 84), (32, 37, 47)).convert("RGB")
    b = badge(72 * SS)
    im.paste(b, ((W * SS - b.width) // 2, 74 * SS), b)
    d = ImageDraw.Draw(im)
    font = fit_text("ResponderHTTP", POPPINS_BOLD, 17 * SS)
    d.text((W * SS // 2, 172 * SS), "ResponderHTTP", font=font, fill=(236, 239, 244), anchor="mt")
    d.line([(W * SS // 2 - 22 * SS, 200 * SS), (W * SS // 2 + 22 * SS, 200 * SS)],
           fill=FROST, width=max(1, SS))
    small = fit_text("API client", POPPINS_MED, 9 * SS)
    d.text((W * SS // 2, 212 * SS), "API client", font=small, fill=MUTED, anchor="mt")
    d.text((W * SS // 2, 226 * SS), "libcurl inside", font=small, fill=MUTED, anchor="mt")
    im.resize((W, H), Image.LANCZOS).save(path)
    return path





def _dib(image):
    """BITMAPINFOHEADER + bottom-up BGRA + 1bpp AND mask."""
    w, h = image.size
    px = image.load()
    header = struct.pack(
        "<IiiHHIIiiII",
        40,        # biSize
        w, h * 2,  # biWidth, biHeight (doubled: XOR image then AND mask)
        1, 32,     # biPlanes, biBitCount
        0,         # BI_RGB
        0, 0, 0, 0, 0,
    )
    xor = bytearray()
    for y in range(h - 1, -1, -1):
        for x in range(w):
            r, g, b, a = px[x, y]
            xor += bytes((b, g, r, a))
    # Fully described by the alpha channel above, but the mask still has to be
    # present and 4-byte aligned per row.
    row = ((w + 31) // 32) * 4
    return header + bytes(xor) + bytes(row * h)


def write_ico_file(path, frames):
    """frames: list of RGBA images, any sizes, largest last is not required."""
    frames = sorted(frames, key=lambda f: f.width)
    payloads = []
    for frame in frames:
        if frame.width <= 48:
            payloads.append(_dib(frame))
        else:
            import io
            buf = io.BytesIO()
            frame.save(buf, format="PNG")
            payloads.append(buf.getvalue())

    offset = 6 + 16 * len(frames)
    out = bytearray(struct.pack("<HHH", 0, 1, len(frames)))  # reserved, type=icon, count
    for frame, payload in zip(frames, payloads):
        out += struct.pack(
            "<BBBBHHII",
            frame.width if frame.width < 256 else 0,
            frame.height if frame.height < 256 else 0,
            0, 0, 1, 32, len(payload), offset,
        )
        offset += len(payload)
    for payload in payloads:
        out += payload
    with open(path, "wb") as handle:
        handle.write(bytes(out))


# Windows draws the notification-area icon at 16 px scaled by the display
# factor: 16 / 20 / 24 / 28 / 32 / 40 / 48 at 100-300 %. 28 is left out; the
# app picks the next size up and lets the shell shrink it by a pixel or two.
TRAY_SIZES = (16, 20, 24, 32, 40, 48)


def tray(path, size):
    """The badge as drawn for the smallest sizes: flat plate, larger glyph."""
    badge(size, flat=size <= 32).save(path, format="PNG")


def frames(muted):
    glyph = UNINSTALL_GLYPH if muted else FROST
    bg = ((88, 94, 108), (58, 63, 74)) if muted else (BG_TOP, BG_BOTTOM)
    return [badge(n, flat=n <= 32, glyph_colour=glyph, bg=bg)
            for n in (16, 20, 24, 32, 48, 64, 128, 256)]


if __name__ == "__main__":
    write_ico_file(f"{OUT}/installer.ico", frames(False))
    write_ico_file(f"{OUT}/uninstaller.ico", frames(True))
    header(f"{OUT}/header.bmp")
    header(f"{OUT}/uninstaller-header.bmp", muted=True)
    sidebar(f"{OUT}/sidebar.bmp")
    for size in TRAY_SIZES:
        tray(f"{OUT}/tray-{size}.png", size)
    print("wrote installer and tray art into", OUT)
