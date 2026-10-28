"""Командная строка: `konung <команда>`."""

import argparse
import sys
from importlib.metadata import version
from pathlib import Path

from konung import disc, images
from konung.k1 import graph_res


def cmd_disc(args: argparse.Namespace) -> None:
    for path in disc.extract(args.mds, args.out):
        print(path)


def cmd_k1_tiles(args: argparse.Namespace) -> None:
    graph = graph_res.load(args.graph)
    args.out.mkdir(parents=True, exist_ok=True)
    for tile in graph.tiles.values():
        image = images.indexed_to_rgba(tile.image, graph.palette(tile.palette_offset))
        image.save(args.out / f"tile{tile.index:03d}.png")
    for index, cursor in enumerate(graph.cursors):
        images.rgb555_to_rgba(cursor).save(args.out / f"cursor{index:02d}.png")
    print(f"{len(graph.tiles)} тайлов и {len(graph.cursors)} курсоров → {args.out}")


def main() -> None:
    # Консоль Windows по умолчанию не в UTF-8, а сообщения — на русском
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(prog="konung", description=__doc__)
    parser.add_argument("--version", action="version", version=version("konung"))
    commands = parser.add_subparsers(required=True)

    p = commands.add_parser("disc", help="образ MDS/MDF → track01.iso и trackNN.wav")
    p.add_argument("mds", type=Path)
    p.add_argument("out", type=Path)
    p.set_defaults(func=cmd_disc)

    k1 = commands.add_parser("k1", help="«Князь: Легенды Лесной страны»").add_subparsers(
        required=True
    )

    p = k1.add_parser("tiles", help="тайлы и курсоры из GRAPH.RES в PNG")
    p.add_argument("graph", type=Path, help="путь к GRAPH.RES")
    p.add_argument("out", type=Path)
    p.set_defaults(func=cmd_k1_tiles)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
