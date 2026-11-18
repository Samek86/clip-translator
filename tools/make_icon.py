#!/usr/bin/env python3
"""Render the tray/window icon: white 가 on a #005FB8 rounded square."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"
BLUE = (0, 95, 184, 255)
WHITE = (255, 255, 255, 255)
ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "assets"


def render(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    radius = max(2, round(size * 0.22))
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=BLUE)
    font = ImageFont.truetype(FONT, max(8, round(size * 0.58)))
    text = "가"
    bbox = draw.textbbox((0, 0), text, font=font)
    width, height = bbox[2] - bbox[0], bbox[3] - bbox[1]
    x = (size - width) / 2 - bbox[0]
    y = (size - height) / 2 - bbox[1] - size * 0.03
    draw.text((x, y), text, font=font, fill=WHITE)
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    sizes = [16, 20, 24, 32, 48, 64, 128, 256]
    images = [render(size) for size in sizes]
    images[-1].save(
        OUT / "icon.ico",
        format="ICO",
        append_images=images[:-1],
        sizes=[(image.width, image.height) for image in images],
    )
    images[sizes.index(32)].save(OUT / "tray-32.png")
    images[sizes.index(64)].save(OUT / "tray-64.png")
    images[-1].save(OUT / "icon-256.png")


if __name__ == "__main__":
    main()
