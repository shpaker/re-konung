"""Командная строка: `konung <команда>`."""

import argparse
import dataclasses
import json
import sys
from importlib.metadata import version
from pathlib import Path

from konung import disc, images
from konung.k1 import game_state, graph_res, map_file, objects_res, render


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


def cmd_k1_map(args: argparse.Namespace) -> None:
    graph = graph_res.load(_find_name(args.game, "graph.res"))
    objects = None if args.no_objects else objects_res.load(_find_name(args.game, "objects.res"))
    state = None if args.state is None else game_state.load(args.game, args.state)
    numbers = map_file.map_numbers(args.game) if args.number is None else [args.number]
    args.out.mkdir(parents=True, exist_ok=True)
    for number in numbers:
        image = render.render(map_file.load(args.game, number), graph, objects)
        if state is not None:
            render.draw_game(image, game_state.on_map(state, number, args.state))
        suffix = "" if state is None else f"-game{args.state}"
        path = args.out / f"konung{number:02d}{suffix}.png"
        image.save(path)
        print(path)


def cmd_k1_game(args: argparse.Namespace) -> None:
    state = game_state.load(args.game, args.number)
    args.out.mkdir(parents=True, exist_ok=True)
    path = args.out / f"game{args.number}.json"

    def plain(value):
        if isinstance(value, bytes):
            return value.hex()
        raise TypeError(type(value))

    path.write_text(
        json.dumps(dataclasses.asdict(state), default=plain, ensure_ascii=False, indent=1),
        encoding="utf-8",
    )
    print(path)


def _find_name(directory: Path, name: str) -> Path:
    """Файл по имени без учёта регистра: в дистрибутиве регистр имён разный."""
    for path in directory.iterdir():
        if path.name.lower() == name.lower():
            return path
    raise FileNotFoundError(f"{directory}: нет {name}")


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

    p = k1.add_parser("map", help="карта Konung.N целиком в PNG")
    p.add_argument("game", type=Path, help="папка установленной игры")
    p.add_argument("out", type=Path)
    p.add_argument("-n", "--number", type=int, help="номер карты; без него — все")
    p.add_argument("--no-objects", action="store_true", help="только земля и декорации")
    p.add_argument(
        "--game", dest="state", type=int, help="номер Game.N: маркеры персонажей и переходов"
    )
    p.set_defaults(func=cmd_k1_map)

    p = k1.add_parser("game", help="начальное состояние Game.N в JSON")
    p.add_argument("game", type=Path, help="папка установленной игры")
    p.add_argument("out", type=Path)
    p.add_argument("-n", "--number", type=int, default=0, help="номер Game.N (герой 0–2)")
    p.set_defaults(func=cmd_k1_game)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
