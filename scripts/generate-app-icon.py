from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter


ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "src-tauri" / "icons"
ICONS.mkdir(parents=True, exist_ok=True)


def rounded_rectangle_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def lerp(a: int, b: int, t: float) -> int:
    return round(a + (b - a) * t)


def gradient(size: int, start: tuple[int, int, int], end: tuple[int, int, int]) -> Image.Image:
    img = Image.new("RGBA", (size, size))
    px = img.load()
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * (size - 1))
            px[x, y] = (
                lerp(start[0], end[0], t),
                lerp(start[1], end[1], t),
                lerp(start[2], end[2], t),
                255,
            )
    return img


def draw_icon(size: int) -> Image.Image:
    scale = size / 1024
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bg = (50, 43, 32, 255)
    bg_2 = (36, 34, 31, 255)
    orange = (255, 166, 0, 255)
    orange_2 = (255, 190, 38, 255)

    # Minimal transparent padding so the icon appears folder-sized in Explorer.
    tile_rect = [18 * scale, 18 * scale, 1006 * scale, 1006 * scale]
    tile_radius = round(220 * scale)
    tile_mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(tile_mask).rounded_rectangle(tile_rect, radius=tile_radius, fill=255)

    # Subtle dark fill, close to the user's reference.
    tile_grad = gradient(size, bg, bg_2)
    img.alpha_composite(Image.composite(tile_grad, Image.new("RGBA", (size, size), (0, 0, 0, 0)), tile_mask))

    # Slight inner vignette.
    shade = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    sdraw = ImageDraw.Draw(shade)
    sdraw.ellipse([-130 * scale, -130 * scale, 1130 * scale, 1130 * scale], fill=(0, 0, 0, 34))
    shade = shade.filter(ImageFilter.GaussianBlur(55 * scale))
    img.alpha_composite(Image.composite(shade, Image.new("RGBA", (size, size), (0, 0, 0, 0)), tile_mask))

    # Orange outline.
    draw = ImageDraw.Draw(img)
    draw.rounded_rectangle(
        [26 * scale, 26 * scale, 998 * scale, 998 * scale],
        radius=tile_radius,
        outline=orange,
        width=max(2, round(18 * scale)),
    )

    # Simple centered bolt, matching the selected reference.
    bolt_points = [
        (580, 146),
        (306, 542),
        (482, 542),
        (396, 884),
        (718, 420),
        (536, 420),
    ]
    bolt_points = [(x * scale, y * scale) for x, y in bolt_points]

    # Small shadow keeps the flat yellow bolt readable on dark fill.
    bolt_shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(bolt_shadow).polygon(
        [(x + 18 * scale, y + 18 * scale) for x, y in bolt_points],
        fill=(0, 0, 0, 72),
    )
    bolt_shadow = bolt_shadow.filter(ImageFilter.GaussianBlur(12 * scale))
    img.alpha_composite(bolt_shadow)

    bolt_mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(bolt_mask).polygon(bolt_points, fill=255)
    bolt_grad = gradient(size, orange_2[:3], orange[:3])
    img.alpha_composite(Image.composite(bolt_grad, Image.new("RGBA", (size, size), (0, 0, 0, 0)), bolt_mask))

    # Crisp dark trim on the right edge like the reference screenshot.
    edge_points = [
        (580, 146),
        (536, 420),
        (718, 420),
        (396, 884),
        (482, 542),
    ]
    edge_points = [(x * scale, y * scale) for x, y in edge_points]
    draw.line(edge_points, fill=(28, 24, 20, 155), width=max(2, round(12 * scale)), joint="curve")

    # Repaint bolt face so the trim does not muddy the center.
    bolt_mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(bolt_mask).polygon(bolt_points, fill=255)
    img.alpha_composite(Image.composite(bolt_grad, Image.new("RGBA", (size, size), (0, 0, 0, 0)), bolt_mask))

    return img


def save_png(path: Path, size: int) -> Image.Image:
    img = draw_icon(1024).resize((size, size), Image.Resampling.LANCZOS)
    img.save(path)
    return img


def cleanup_stale_icons() -> None:
    keep = {
        "32x32.png",
        "128x128.png",
        "128x128@2x.png",
        "icon.png",
        "icon.ico",
    }
    for path in ICONS.iterdir():
        if path.is_file() and path.name not in keep:
            path.unlink()


def main() -> None:
    cleanup_stale_icons()
    img32 = save_png(ICONS / "32x32.png", 32)
    img128 = save_png(ICONS / "128x128.png", 128)
    img256 = save_png(ICONS / "128x128@2x.png", 256)
    img1024 = draw_icon(1024)
    img1024.save(ICONS / "icon.png")
    img1024.save(
        ICONS / "icon.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print(f"Generated icons in {ICONS}")


if __name__ == "__main__":
    main()
