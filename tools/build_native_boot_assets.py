from __future__ import annotations

import math
import random
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont


ROOT = Path(__file__).resolve().parents[1]
BIOS = ROOT / "bios"


def fit_font(draw: ImageDraw.ImageDraw, text: str, max_width: int, start: int) -> ImageFont.FreeTypeFont:
    size = start
    while size > 12:
        font = ImageFont.truetype(BIOS / "orbitron.ttf", size)
        if draw.textbbox((0, 0), text, font=font, stroke_width=1)[2] <= max_width:
            return font
        size -= 2
    return ImageFont.truetype(BIOS / "orbitron.ttf", 12)


def build_neon_rift_label(width: int, height: int) -> Image.Image:
    label = Image.new("RGBA", (width, height), (5, 4, 24, 255))
    pixels = label.load()
    for y in range(height):
        for x in range(width):
            radial = max(0.0, 1.0 - math.hypot(x - width * 0.68, y - height * 0.38) / (width * 0.72))
            horizon = max(0.0, 1.0 - abs(y - height * 0.63) / (height * 0.45))
            pixels[x, y] = (
                round(5 + 20 * radial),
                round(4 + 10 * radial + 8 * horizon),
                round(24 + 52 * radial + 30 * horizon),
                255,
            )

    glow = Image.new("RGBA", label.size, (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow)
    horizon_y = round(height * 0.66)
    glow_draw.line((0, horizon_y, width, horizon_y), fill=(18, 220, 255, 220), width=max(2, height // 55))
    glow_draw.ellipse(
        (
            round(width * 0.60),
            round(height * 0.06),
            round(width * 0.91),
            round(height * 0.57),
        ),
        fill=(255, 35, 185, 150),
    )
    label.alpha_composite(glow.filter(ImageFilter.GaussianBlur(max(5, height // 16))))

    draw = ImageDraw.Draw(label)
    # Neon planet and crescent.
    planet_box = (
        round(width * 0.64),
        round(height * 0.08),
        round(width * 0.88),
        round(height * 0.53),
    )
    draw.ellipse(planet_box, fill=(43, 18, 118, 255), outline=(255, 55, 195, 255), width=max(2, height // 60))
    draw.arc(planet_box, 205, 24, fill=(31, 225, 255, 255), width=max(3, height // 42))

    # Perspective laser grid.
    vanishing = (width // 2, horizon_y)
    for index in range(-10, 11):
        x = round(width * 0.5 + index * width * 0.095)
        color = (26, 204, 255, 150) if index % 2 == 0 else (229, 33, 255, 130)
        draw.line((vanishing[0], vanishing[1], x, height), fill=color, width=max(1, height // 130))
    for row in range(1, 9):
        progress = (row / 8.0) ** 1.8
        y = round(horizon_y + (height - horizon_y) * progress)
        draw.line((0, y, width, y), fill=(111, 52, 255, 150), width=max(1, height // 150))

    rng = random.Random(0x504C4159)
    for _ in range(55):
        x = rng.randrange(width)
        y = rng.randrange(max(1, horizon_y))
        radius = 1 if rng.random() < 0.8 else 2
        color = (35, 220, 255, 210) if rng.random() < 0.55 else (255, 54, 195, 210)
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=color)

    title = "NEON RIFT"
    title_font = fit_font(draw, title, round(width * 0.82), round(height * 0.30))
    title_box = draw.textbbox((0, 0), title, font=title_font, stroke_width=max(1, height // 90))
    title_width = title_box[2] - title_box[0]
    title_x = (width - title_width) // 2
    title_y = round(height * 0.28)
    stroke = max(2, height // 42)
    draw.text(
        (title_x + stroke, title_y + stroke),
        title,
        font=title_font,
        fill=(255, 38, 186, 80),
        stroke_width=stroke * 2,
        stroke_fill=(255, 38, 186, 45),
    )
    draw.text(
        (title_x, title_y),
        title,
        font=title_font,
        fill=(226, 244, 255, 255),
        stroke_width=max(1, height // 75),
        stroke_fill=(16, 184, 255, 255),
    )

    subtitle = "A PLAYFUSION ORIGINAL"
    subtitle_font = fit_font(draw, subtitle, round(width * 0.70), round(height * 0.095))
    subtitle_box = draw.textbbox((0, 0), subtitle, font=subtitle_font)
    subtitle_x = (width - (subtitle_box[2] - subtitle_box[0])) // 2
    draw.text(
        (subtitle_x, round(height * 0.58)),
        subtitle,
        font=subtitle_font,
        fill=(255, 113, 57, 255),
    )

    corner = round(height * 0.055)
    mask = Image.new("L", label.size, 0)
    ImageDraw.Draw(mask).polygon(
        [
            (corner, 0),
            (width - corner, 0),
            (width, corner),
            (width, height - corner),
            (width - corner, height),
            (corner, height),
            (0, height - corner),
            (0, corner),
        ],
        fill=246,
    )
    label.putalpha(mask)
    return label


def main() -> None:
    cartridge = Image.open(BIOS / "boot-cartridge.png").convert("RGBA")
    bbox = cartridge.getbbox()
    if bbox is None:
        raise RuntimeError("Cartridge artwork is empty")
    cartridge = cartridge.crop(bbox)

    label_width = round(cartridge.width * 0.60)
    label_height = round(cartridge.height * 0.49)
    label = build_neon_rift_label(label_width, label_height)
    label_x = (cartridge.width - label_width) // 2
    label_y = round(cartridge.height * 0.18)

    glow = Image.new("RGBA", cartridge.size, (0, 0, 0, 0))
    ImageDraw.Draw(glow).rounded_rectangle(
        (
            label_x - 5,
            label_y - 5,
            label_x + label_width + 5,
            label_y + label_height + 5,
        ),
        radius=round(label_height * 0.055),
        fill=(42, 198, 255, 90),
        outline=(255, 35, 188, 125),
        width=5,
    )
    cartridge.alpha_composite(glow.filter(ImageFilter.GaussianBlur(14)))
    cartridge.alpha_composite(label, (label_x, label_y))

    glass = Image.new("RGBA", label.size, (0, 0, 0, 0))
    ImageDraw.Draw(glass).polygon(
        [
            (0, 0),
            (round(label_width * 0.62), 0),
            (round(label_width * 0.34), label_height),
            (0, label_height),
        ],
        fill=(255, 255, 255, 22),
    )
    glass.putalpha(ImageChops.multiply(glass.getchannel("A"), label.getchannel("A")))
    cartridge.alpha_composite(glass, (label_x, label_y))

    output = BIOS / "boot-cartridge-neon-rift.png"
    cartridge.save(output)
    print(output)


if __name__ == "__main__":
    main()
