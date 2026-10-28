from konung import rle
from konung.binary import Reader
from konung.k1 import characters, executable, game_state, graph_res, map_file, objects_res


def test_graph_res_is_parsed_to_the_last_byte(k1_file):
    data = k1_file("graph.res").read_bytes()
    graph = graph_res.parse(data)
    assert graph.size == len(data)
    assert len(graph.cursors) == 11
    ground = [t for t in graph.tiles.values() if (t.image.width, t.image.height) == (114, 64)]
    assert len(graph.tiles) == 305
    assert len(ground) == 221


def _last_byte(sprites: graph_res.Sprites) -> int:
    """Конец последней картинки или тени спрайта; заодно все они декодируются."""
    end = 0
    for shadow, image in sprites.table:
        if image >= 0:
            reader = Reader(sprites.data, sprites.images_start + image)
            rle.decode_indexed(reader)
            end = max(end, reader.pos)
        if shadow > 0:
            reader = Reader(sprites.data, sprites.images_start + shadow)
            rle.decode_shadow(reader)
            end = max(end, reader.pos)
    return end


def _frames_have_images(sprites: graph_res.Sprites) -> bool:
    return all(
        sprites.table[frame.image_id][1] >= 0
        for animation in sprites.sequences
        for sequence in animation
        for frame in sequence
    )


def test_character_sprites_end_with_their_blocks(k1_file):
    graph = graph_res.load(k1_file("graph.res"))
    assert len(graph.models) == 3
    assert len(graph.equipment) == 24
    for block, sprites in zip(graph.character_blocks, graph.models + graph.equipment, strict=True):
        assert _last_byte(sprites) == block.end
        assert _frames_have_images(sprites)


def test_objects_res_has_all_objects(k1_file):
    data = k1_file("objects.res").read_bytes()
    objects = objects_res.parse(data)
    assert len(objects.simple) == 463
    assert all(obj.frames[0].image is not None for obj in objects.simple.values())
    assert sorted(objects.dynamic) == list(range(30))
    for slot, sprites in objects.dynamic.items():
        # Размер блока — в каталоге, блок — от метки и размера перед заголовком
        offset, size = Reader(data, 8 + slot * 8).s32(), Reader(data, 12 + slot * 8).s32()
        assert _last_byte(sprites) == 8 + 512 * 8 + offset + size, slot
        assert _frames_have_images(sprites), slot


def test_executable_tables_are_found(k1_game):
    tables = executable.load(k1_game)
    assert len(tables.equipment) == executable.EQUIPMENT_COUNT
    assert all(-1 <= info.sprite_set < 24 for info in tables.equipment)
    assert len(tables.draw_order) == 8
    assert all(sorted(row) == list(range(6)) for row in tables.draw_order)


def test_every_character_on_every_map_is_drawn(k1_game, k1_file):
    graph = graph_res.load(k1_file("graph.res"))
    objects = objects_res.load(k1_file("objects.res"))
    tables = executable.load(k1_game)
    for hero in game_state.numbers(k1_game):
        state = game_state.load(k1_game, hero)
        for number in map_file.map_numbers(k1_game):
            entities = game_state.on_map(state, number, hero)
            for character, x, y in characters.placed(entities, state):
                sprite = characters.compose(character, x, y, state, graph, objects, tables)
                assert sprite is not None, (hero, number, character.slot)
                if not character.has_dynamic_sprite:
                    worn = [i for i in character.equipment if i > 0]
                    drawn = [
                        i for i in worn if tables.equipment[state.items[i].info_id].sprite_set >= 0
                    ]
                    assert len(sprite.layers) == 1 + len(drawn), (hero, character.slot)
