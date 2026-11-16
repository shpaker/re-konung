"""Render owned item icons locally; run: uv run tools/inspect_k1_inventory_icons.py."""

from pathlib import Path

from PIL import Image, ImageDraw

from konung.k1 import executable, graph_res, intr_res

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "out/issue-139/inventory-icons"


def main():
    games = [ROOT / "original/k1" / edition / "game" for edition in ("rus", "eng")]
    for game in games:
        if not game.is_dir():
            raise FileNotFoundError(f"Both owned editions are required: {game}")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    baseline = None
    for game in games:
        tables = executable.load(game)
        icons = {row.values[7] for row in tables.equipment}
        icons.update(row.values[2] for rows in tables.items.values() for row in rows)
        graph_path = next(path for path in game.iterdir() if path.name.lower() == "graph.res")
        graph = graph_res.load(graph_path)
        resources = sorted(path for path in game.iterdir() if path.name.lower().startswith("intr"))
        expected_count = 2 if game.parent.name == "rus" else 4
        if len(resources) != expected_count:
            raise ValueError(f"Expected {expected_count} owned INTR resources in {game}")
        for path in resources:
            resource = intr_res.load(path)
            width = max(resource.images[index].image.width for index in icons) + 12
            height = max(resource.images[index].image.height for index in icons) + 24
            columns = 13
            sheet = Image.new("RGB", (columns * width, ((len(icons) + 12) // 13) * height))
            draw = ImageDraw.Draw(sheet)
            decoded_icons = {}
            for position, index in enumerate(sorted(icons)):
                left, top = position % columns * width, position // columns * height
                for row in range(0, height, 8):
                    for column in range(0, width, 8):
                        color = (40, 40, 40) if (row // 8 + column // 8) % 2 else (64, 64, 64)
                        draw.rectangle(
                            (
                                left + column,
                                top + row,
                                left + min(column + 7, width - 1),
                                top + min(row + 7, height - 1),
                            ),
                            fill=color,
                        )
                entry = resource.images[index]
                image = entry.image
                palette = graph.palette(entry.palette_offset // 2)
                pixels = bytes(
                    channel
                    for color, alpha in zip(image.indices, image.alpha, strict=True)
                    for channel in (*palette[color], alpha)
                )
                decoded_icons[index] = (image.width, image.height, pixels)
                rgba = Image.frombytes("RGBA", (image.width, image.height), pixels)
                sheet.paste(rgba, (left + (width - image.width) // 2, top + 20), rgba)
                draw.text((left + 4, top + 3), str(index), fill=(255, 255, 255))
            if baseline is None:
                baseline = decoded_icons
            elif decoded_icons != baseline:
                raise ValueError(f"Item icons differ from the first owned resource: {path}")
            destination = OUTPUT / f"{game.parent.name}-{path.stem.lower()}.png"
            sheet.save(destination)
            print(
                f"{game.parent.name}/{path.name}: {len(resource.images)} images decoded, "
                f"{len(icons)} item icons -> {destination.relative_to(ROOT)}"
            )
    print("All six owned resources have identical decoded item-icon RGBA.")


if __name__ == "__main__":
    main()
