"""Командная строка: `konung <команда>`."""

import argparse
import dataclasses
import json
import sys
from importlib.metadata import version
from pathlib import Path

from konung import disc, images
from konung.k1 import (
    characters,
    executable,
    flames_res,
    game_state,
    graph_res,
    lights_res,
    map_file,
    objects_res,
    render,
)


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
    files = map_file.TUTORIAL_FILES if args.tutorial else map_file.GAME_FILES
    if args.tutorial and not map_file.map_numbers(args.game, files):
        sys.exit("карт обучения нет (они есть только в ENG)")
    if args.state is not None and args.state not in game_state.numbers(args.game, files):
        sys.exit(f"нет {files.state}.{args.state}")
    state = None if args.state is None else game_state.load(args.game, args.state, files)
    tables = None if objects is None else executable.load(args.game)
    flames = None
    if objects is not None:
        flames = (flames_res.load(_find_name(args.game, "flames.res")), tables.flame_animations)
    masks = lights_res.load(_find_name(args.game, "lights.res")) if args.lights else None
    numbers = map_file.map_numbers(args.game, files) if args.number is None else [args.number]
    args.out.mkdir(parents=True, exist_ok=True)
    for number in numbers:
        entities = None if state is None else game_state.on_map(state, number, args.state)
        sprites = []
        if entities is not None:
            for character, x, y in characters.placed(entities, state):
                px, py = game_state.character_position(x, y)
                sprite = characters.compose(character, px, py, state, graph, objects, tables)
                if sprite is not None:
                    sprites.append(sprite)
        konung_map = map_file.load(args.game, number, files)
        image = render.render(konung_map, graph, objects, sprites, flames)
        if masks is not None and (path := map_file.lights_path(args.game, number, files)):
            render.draw_lights(image, map_file.parse_lights(path.read_bytes()), masks)
        if args.foot:
            render.draw_foot(image, konung_map)
        if entities is not None:
            # Персонажей видно спрайтами; маркеры — только когда спрайтов нет
            render.draw_game(image, entities, characters=objects is None)
        suffix = "" if state is None else f"-game{args.state}"
        suffix += "-lights" * args.lights + "-foot" * args.foot
        path = args.out / f"{files.map}{number:02d}{suffix}.png"
        image.save(path)
        print(path)


def cmd_k1_sprites(args: argparse.Namespace) -> None:
    graph = graph_res.load(_find_name(args.game, "graph.res"))
    objects = objects_res.load(_find_name(args.game, "objects.res"))
    # Палитра модели или динамического объекта — у персонажей Game.0, которые им рисуются
    state = game_state.load(args.game, 0)
    palettes: dict[tuple[bool, int], int] = {}
    for character in state.characters:
        if character is not None:
            key = (character.has_dynamic_sprite, character.sprite_set)
            palettes.setdefault(key, character.palette_offset // 2)
    sheets = [(f"model{i}", s, palettes.get((False, i), 0)) for i, s in enumerate(graph.models)]
    sheets += [
        (f"equipment{i:02d}", s, executable.EQUIPMENT_PALETTES[i] * characters.COLORS_PER_PALETTE)
        for i, s in enumerate(graph.equipment)
    ]
    sheets += [
        (f"dynamic{i:02d}", s, palettes.get((True, i), 0))
        for i, s in sorted(objects.dynamic.items())
    ]
    args.out.mkdir(parents=True, exist_ok=True)
    for name, sprites, palette in sheets:
        sheet = render.sprite_sheet(sprites, graph, palette)
        if sheet is not None:
            sheet.save(args.out / f"{name}.png")
    print(f"{len(sheets)} спрайтов → {args.out}")


def cmd_k1_flames(args: argparse.Namespace) -> None:
    flames = flames_res.load(_find_name(args.game, "flames.res"))
    tables = executable.load(args.game)
    args.out.mkdir(parents=True, exist_ok=True)
    for kind, (first, last) in enumerate(tables.flame_animations):
        for index in range(first, last + 1):
            image = flames_res.to_rgba(flames.frames[index])
            image.save(args.out / f"flame{kind}-{index:02d}.png")
    print(f"{len(flames.frames)} кадров огней → {args.out}")


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
    p.add_argument(
        "--foot", action="store_true", help="проходимость поверх карты: красное — непроходимо"
    )
    p.add_argument("--lights", action="store_true", help="свет LIGHTS.N поверх карты")
    p.add_argument(
        "--tutorial",
        action="store_true",
        help="карты обучения Tutorial.N (только ENG): свет TLights.N, --game — номер TGame.N",
    )
    p.set_defaults(func=cmd_k1_map)

    p = k1.add_parser("sprites", help="спрайты персонажей, снаряжения и монстров в PNG")
    p.add_argument("game", type=Path, help="папка установленной игры")
    p.add_argument("out", type=Path)
    p.set_defaults(func=cmd_k1_sprites)

    p = k1.add_parser("flames", help="кадры огней FLAMES.RES в PNG по видам")
    p.add_argument("game", type=Path, help="папка установленной игры")
    p.add_argument("out", type=Path)
    p.set_defaults(func=cmd_k1_flames)

    p = k1.add_parser("game", help="начальное состояние Game.N в JSON")
    p.add_argument("game", type=Path, help="папка установленной игры")
    p.add_argument("out", type=Path)
    p.add_argument("-n", "--number", type=int, default=0, help="номер Game.N (герой 0–2)")
    p.set_defaults(func=cmd_k1_game)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
