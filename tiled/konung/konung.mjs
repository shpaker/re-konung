// Расширение Tiled: открывает оригинальные карты «Князя» (Konung.N) как обычные
// карты Tiled и сохраняет их обратно. Меняются тайлы, декорации и объекты карты,
// а в Game.N рядом с картой — персонажи, отряды, клады и переходы этой карты;
// GRAPH.RES, Objects.res и exe лишь читаются. Слои — docs/k1/tiled.md.

import * as k1 from "./lib/k1.mjs";

// Tiled выбирает скриптовый формат по суффиксу «.расширение», а у карт Князя
// расширение — номер локации. Поэтому формат регистрируется на каждый номер.
const MAX_MAP_NUMBER = 50;

// Тени — чёрная маска под слоем объектов; Python-рендер берёт ту же прозрачность
const SHADOW_OPACITY = 0.4;

// Слои проходимости: тайл — 4 клетки мелкой сетки в ромбе тайла, номер тайла —
// маска отмеченных клеток. Цвета — как в `konung k1 map --foot`
const FOOT_LAYERS = [
    { name: "foot", flag: k1.FOOT_FLAGS.blocked, color: 0x6eff2828 },
    { name: "foot-arrows", flag: k1.FOOT_FLAGS.arrows, color: 0x6e2878ff },
];
const FOOT_MASKS = 16;

// Значения полей для декораций и объектов, поставленных в Tiled: самые частые
// в картах дистрибутива (docs/k1/formats/map.md)
const NEW_DECORATION_UNKNOWN = "0016000000000000";
const NEW_OBJECT = {
    unknown: "00000000",
    flames: "0000000000000000ffffffffffffffffffffffff",
    frameTime: 0,
    flags: 0,
    frame: 0,
};

function readBuffer(path) {
    const file = new BinaryFile(path, BinaryFile.ReadOnly);
    const buffer = file.readAll();
    file.close();
    return buffer;
}

// Регистр имён в дистрибутиве разный: Konung.1, KONUNG.2, GRAPH.RES, Objects.res
function findFile(directory, name) {
    const wanted = name.toLowerCase();
    for (const entry of File.directoryEntries(directory, File.Files))
        if (entry.toLowerCase() === wanted)
            return `${directory}/${entry}`;
    throw new Error(`в ${directory} нет ${name}`);
}

function toImage(decoded) {
    return new Image(decoded.pixels.buffer, decoded.width, decoded.height, Image.Format_ARGB32);
}

// Декодирование GRAPH.RES и Objects.res — секунды, поэтому кэш на сессию Tiled
const graphCache = new Map();
const catalogCache = new Map();
const objectsCache = new Map();
const tablesCache = new Map();

// Тайлсеты, созданные скриптом, живут, пока на них есть ссылка из JS: если
// сборщик мусора уберёт обёртку, Tiled 1.12.2 продолжает рисовать тайлы, но
// map.tilesets даёт null, а следующая сборка мусора или закрытие карты роняют
// Tiled. Поэтому ссылки держатся, пока карта открыта (ключ — путь файла)
const liveTilesets = new Map();
let readingTilesets = [];

function newTileset(name) {
    const tileset = new Tileset(name);
    readingTilesets.push(tileset);
    return tileset;
}

tiled.assetAboutToBeClosed.connect(asset => liveTilesets.delete(asset.fileName));

function loadGraph(directory) {
    const path = findFile(directory, "graph.res");
    if (!graphCache.has(path))
        graphCache.set(path, k1.parseGraph(readBuffer(path)));
    return graphCache.get(path);
}

function loadCatalog(directory) {
    const path = findFile(directory, "objects.res");
    if (!catalogCache.has(path))
        catalogCache.set(path, k1.parseObjectsCatalog(readBuffer(path)));
    return catalogCache.get(path);
}

// Таблицы снаряжения из exe: в RUS это konung.res, в ENG — KONUNG.EXE.
// Без exe персонажи рисуются без снаряжения
const NO_TABLES = { equipment: [], drawOrder: [[], [], [], [], [], [], [], []], flameAnimations: null };
const flamesCache = new Map();

// Кадры огней FLAMES.RES; без файла огней нет
function loadFlames(directory) {
    let path;
    try {
        path = findFile(directory, "flames.res");
    } catch (error) {
        return null;
    }
    if (!flamesCache.has(path))
        flamesCache.set(path, k1.parseFlames(readBuffer(path)));
    return flamesCache.get(path);
}

function loadTables(directory) {
    for (const name of ["konung.exe", "konung.res"]) {
        let path;
        try {
            path = findFile(directory, name);
        } catch (error) {
            continue;
        }
        if (!tablesCache.has(path))
            tablesCache.set(path, k1.parseExecutable(readBuffer(path)));
        return tablesCache.get(path);
    }
    return NO_TABLES;
}

// Кадр 0 каждого простого объекта: в картах дистрибутива кадр всегда 0 или 0xFF.
// У анимированного объекта все кадры в общей рамке (animation), а картинка,
// тень и их смещения — первого кадра этой рамки. bottom — нижний край кадра 0
// без рамки: по нему игра сортирует объекты
function loadObjects(directory, graph) {
    const path = findFile(directory, "objects.res");
    if (!objectsCache.has(path)) {
        const catalog = loadCatalog(directory);
        const objects = [];
        for (let objectId = 0; objectId < k1.SIMPLE_OBJECT_SLOTS; ++objectId) {
            const simple = k1.readSimpleObject(catalog, graph, objectId);
            if (!simple || !simple.frames[0].image)
                continue;
            const first = simple.frames[0];
            const entry = {
                image: first.image, offset: first.offset, shadow: first.shadow, shadowOffset: first.shadowOffset,
                bottom: first.offset[1] + first.image.height, animation: k1.objectAnimation(simple),
                flames: simple.flames,
            };
            const animation = entry.animation;
            if (animation) {
                entry.image = animation.image.images[0];
                entry.offset = animation.image.offset;
                entry.shadow = animation.shadow ? animation.shadow.images[0] : null;
                entry.shadowOffset = animation.shadow ? animation.shadow.offset : first.shadowOffset;
            }
            objects[objectId] = entry;
        }
        objectsCache.set(path, objects);
    }
    return objectsCache.get(path);
}

// Остальные кадры — новые тайлы того же тайлсета; первый тайл получает анимацию
function animateTile(tileset, tile, images, duration, properties) {
    const frames = [{ tileId: tile.id, duration }];
    for (const image of images.slice(1)) {
        const frame = tileset.addTile();
        frame.setImage(toImage(image));
        setProperties(frame, properties);
        frames.push({ tileId: frame.id, duration });
    }
    tile.frames = frames;
}

// Тайлсет-коллекция, где номер тайла Tiled совпадает с номером в источнике
function buildTileset(name, count, imageOf, propertiesOf) {
    const tileset = newTileset(name);
    tileset.objectAlignment = Tileset.TopLeft;
    const empty = [];
    for (let index = 0; index < count; ++index) {
        const tile = tileset.addTile();
        const decoded = imageOf(index);
        if (decoded) {
            tile.setImage(toImage(decoded));
            if (propertiesOf)
                setProperties(tile, propertiesOf(index));
        } else {
            empty.push(tile);
        }
    }
    if (empty.length)
        tileset.removeTiles(empty);
    return tileset;
}

// Картинка тайла проходимости: ромбы 58 × 32 отмеченных клеток
function footImage(mask, color) {
    const width = k1.TILE_STEP_X, height = k1.GROUND_TILE_HEIGHT;
    const halfX = width / 4, halfY = k1.TILE_STEP_Y / 2;
    const pixels = new Uint32Array(width * height);
    k1.FINE_CELL_CENTERS.forEach(([cx, cy], i) => {
        if (!(mask >> i & 1))
            return;
        for (let y = cy - halfY; y < cy + halfY; ++y)
            for (let x = cx - halfX; x < cx + halfX; ++x)
                if (Math.abs(x + 0.5 - cx) / halfX + Math.abs(y + 0.5 - cy) / halfY <= 1)
                    pixels[y * width + x] = color;
    });
    return { width, height, pixels };
}

function isGroundSized(decoded) {
    return decoded.width === k1.GROUND_TILE_WIDTH && decoded.height === k1.GROUND_TILE_HEIGHT;
}

function setProperties(target, properties) {
    for (const [name, value] of Object.entries(properties))
        target.setProperty(name, value);
}

function addTileObject(group, tile, x, y, properties) {
    const object = new MapObject();
    object.tile = tile;
    object.x = x;
    object.y = y;
    object.width = tile.width;
    object.height = tile.height;
    setProperties(object, properties);
    group.addObject(object);
    return object;
}

function readMap(fileName) {
    readingTilesets = [];
    const map = buildMap(fileName);
    liveTilesets.set(fileName, readingTilesets);
    return map;
}

function buildMap(fileName) {
    const directory = FileInfo.path(fileName);
    const data = k1.parseMap(readBuffer(fileName));
    const graph = loadGraph(directory);
    const objects = loadObjects(directory, graph);

    const map = new TileMap();
    map.orientation = TileMap.Staggered;
    map.staggerAxis = TileMap.StaggerY;
    map.staggerIndex = TileMap.StaggerOdd;
    map.setSize(k1.MAP_WIDTH, k1.MAP_HEIGHT);
    map.setTileSize(k1.TILE_STEP_X, k1.GROUND_TILE_HEIGHT);
    map.backgroundColor = "#000000";
    // foot — исходные байты проходимости: правки слоёв проходимости ложатся
    // поверх них, а клетки у края карты без тайла пишутся отсюда. mapNumber — номер карты, чьи объекты Game.N показаны и пишутся при сохранении
    const number = Number(FileInfo.suffix(fileName));
    setProperties(map, { source: FileInfo.fileName(fileName), mapNumber: number, foot: k1.toHex(data.foot) });

    const graphTileset = buildTileset("graph", graph.tiles.length, index => graph.tiles[index]);
    const shadowsTileset = buildTileset("shadows", objects.length,
        objectId => objects[objectId]?.shadow, objectId => ({ objectId }));
    const objectsTileset = buildTileset("objects", objects.length,
        objectId => objects[objectId]?.image, objectId => ({
            objectId,
            offsetX: objects[objectId].offset[0],
            offsetY: objects[objectId].offset[1],
            shadowOffsetX: objects[objectId].shadowOffset[0],
            shadowOffsetY: objects[objectId].shadowOffset[1],
        }));
    // Кадры анимированных объектов несут те же свойства: объект, поставленный
    // любым кадром, сохраняется как этот объект
    objects.forEach((entry, objectId) => {
        const animation = entry && entry.animation;
        if (!animation)
            return;
        animateTile(objectsTileset, objectsTileset.tile(objectId), animation.image.images, animation.duration,
            objectsTileset.tile(objectId).properties());
        if (animation.shadow)
            animateTile(shadowsTileset, shadowsTileset.tile(objectId), animation.shadow.images, animation.duration,
                { objectId });
    });
    map.addTileset(graphTileset);
    map.addTileset(shadowsTileset);
    map.addTileset(objectsTileset);

    // Клетки с тайлом не размера ромба Tiled поставил бы от нижнего края,
    // а игра — от верхнего. Их место — в слое декораций, от верхнего левого угла.
    const oddCells = [];
    for (const [name, cells] of [["ground", data.ground], ["overlay", data.overlay]]) {
        const layer = new TileLayer(name);
        layer.resize(Qt.size(k1.MAP_WIDTH, k1.MAP_HEIGHT));
        const edit = layer.edit();
        cells.forEach((row, y) => row.forEach((value, x) => {
            if (!value)
                return;
            const index = value - 1;
            const decoded = graph.tiles[index];
            if (!decoded)
                return;
            if (isGroundSized(decoded))
                edit.setTile(x, y, graphTileset.tile(index));
            else
                oddCells.push({ layer: name, x, y, index });
        }));
        edit.apply();
        map.addLayer(layer);
    }

    const decorations = new ObjectGroup("decorations");
    decorations.drawOrder = ObjectGroup.IndexOrder;
    for (const cell of oddCells) {
        const [x, y] = k1.tilePosition(cell.x, cell.y);
        addTileObject(decorations, graphTileset.tile(cell.index), x, y,
            { cellLayer: cell.layer, cellX: cell.x, cellY: cell.y });
    }
    for (const decoration of data.decorations) {
        const tile = graphTileset.tile(decoration.tile);
        if (tile)
            addTileObject(decorations, tile, decoration.x, decoration.y,
                { slot: decoration.slot, unknown: decoration.unknown });
    }
    map.addLayer(decorations);

    addObjects(map, objects, shadowsTileset, objectsTileset, data.objects);
    addFlames(map, directory, objects, data.objects);
    addGameStates(map, directory, number, graph);
    addFoot(map, data.foot);
    return map;
}

// Проходимость — над всеми слоями, скрыта: включить, когда нужна
function addFoot(map, foot) {
    for (const { name, flag, color } of FOOT_LAYERS) {
        const tileset = buildTileset(name, FOOT_MASKS, mask => mask ? footImage(mask, color) : null);
        map.addTileset(tileset);
        const layer = new TileLayer(name);
        layer.resize(Qt.size(k1.MAP_WIDTH, k1.MAP_HEIGHT));
        const edit = layer.edit();
        for (let y = 0; y < k1.MAP_HEIGHT; ++y)
            for (let x = 0; x < k1.MAP_WIDTH; ++x) {
                const mask = k1.footMask(foot, x, y, flag);
                if (mask)
                    edit.setTile(x, y, tileset.tile(mask));
            }
        edit.apply();
        layer.visible = false;
        map.addLayer(layer);
    }
}

// Game.N рядом с картой: начальные состояния за героев 0–2 (TGame.N обучения — нет)
function gameFiles(directory) {
    const found = [];
    for (const entry of File.directoryEntries(directory, File.Files)) {
        const match = /^game\.(\d+)$/i.exec(entry);
        if (match)
            found.push({ hero: Number(match[1]), path: `${directory}/${entry}` });
    }
    return found.sort((a, b) => a.hero - b.hero);
}

// FNV-1a: заметить, что Game.N изменился после открытия карты
function checksum(buffer) {
    const bytes = new Uint8Array(buffer);
    let hash = 0x811c9dc5;
    for (let i = 0; i < bytes.length; ++i)
        hash = Math.imul(hash ^ bytes[i], 0x01000193);
    return (hash >>> 0).toString(16).padStart(8, "0");
}

// Каждый Game.N — группа слоёв game.K; видна первая. docs/k1/tiled.md
function addGameStates(map, directory, number, graph) {
    const files = gameFiles(directory);
    if (!files.length)
        return;
    const catalog = loadCatalog(directory);
    const dynamic = new Map();
    const dynamicSprites = slot => {
        if (!dynamic.has(slot))
            dynamic.set(slot, k1.readDynamicObject(catalog, slot));
        return dynamic.get(slot);
    };
    const tables = loadTables(directory);
    files.forEach(({ hero, path }, index) => {
        const buffer = readBuffer(path);
        const state = k1.parseGameState(buffer);
        const group = buildGameGroup(map, state, hero, number, graph, dynamicSprites, tables);
        group.visible = index === 0;
        group.setProperty("checksum", checksum(buffer));
        map.addLayer(group);
    });
}

function addPoint(layer, name, [x, y], properties) {
    const object = new MapObject(name);
    object.shape = MapObject.Point;
    object.x = x;
    object.y = y;
    setProperties(object, properties);
    layer.addObject(object);
    return object;
}

function addRect(layer, name, area, properties) {
    const [x, y, width, height] = k1.rectPixels(area);
    const object = new MapObject(name);
    object.x = x;
    object.y = y;
    object.width = width;
    object.height = height;
    setProperties(object, properties);
    layer.addObject(object);
    return object;
}

function buildGameGroup(map, state, hero, number, graph, dynamicSprites, tables) {
    const entities = k1.entitiesOnMap(state, number, hero);
    const group = new GroupLayer(`game.${hero}`);

    // Тайл на персонажа: модель со снаряжением одной картинкой, кадры его
    // анимации — в общей рамке; offsetX/offsetY — смещение рамки от точки
    const charactersTileset = newTileset(`characters.${hero}`);
    charactersTileset.objectAlignment = Tileset.TopLeft;
    const shadowsTileset = newTileset(`character-shadows.${hero}`);
    shadowsTileset.objectAlignment = Tileset.TopLeft;
    const sprites = new Map();
    const spriteOf = character => {
        if (!sprites.has(character.slot)) {
            const animation = k1.composeAnimation(character, state, graph, dynamicSprites, tables);
            let entry = null;
            if (animation) {
                const { image, shadow } = animation;
                const properties = { character: character.slot, offsetX: image.offset[0], offsetY: image.offset[1] };
                const tile = charactersTileset.addTile();
                tile.setImage(toImage(image.images[0]));
                setProperties(tile, properties);
                if (image.images.length > 1)
                    animateTile(charactersTileset, tile, image.images, animation.duration, properties);
                let shadowTile = null;
                if (shadow) {
                    shadowTile = shadowsTileset.addTile();
                    shadowTile.setImage(toImage(shadow.images[0]));
                    if (shadow.images.length > 1)
                        animateTile(shadowsTileset, shadowTile, shadow.images, animation.duration, {});
                }
                entry = {
                    tile, shadowTile, offset: image.offset, shadowOffset: shadow && shadow.offset,
                    bottom: animation.bottom,
                };
            }
            sprites.set(character.slot, entry);
        }
        return sprites.get(character.slot);
    };

    // Где рисовать: NPC и жители — на своих местах, бродячий отряд — первым
    // членом в середине прямоугольника появления
    const placed = entities.characters.map(character => ({ character, cell: [character.x, character.y], preview: false }));
    for (const g of entities.groups) {
        const first = state.characters[g.firstCharacter];
        if (g.roaming && g.size > 0 && first) {
            const cell = [((g.spawn.left + g.spawn.right + 1) / 2) | 0, ((g.spawn.top + g.spawn.bottom + 1) / 2) | 0];
            placed.push({ character: first, cell, preview: true });
        }
    }
    const drawn = [];
    for (const item of placed) {
        const entry = spriteOf(item.character);
        if (!entry)
            continue;
        const [x, y] = k1.characterPosition(...item.cell);
        drawn.push(Object.assign({}, item, { entry, x, y }));
    }
    // Ближе к зрителю то, что ниже на экране
    const bottom = item => item.y + item.entry.bottom;
    drawn.sort((a, b) => bottom(a) - bottom(b) || a.character.slot - b.character.slot);

    const shadows = new ObjectGroup("shadows");
    shadows.opacity = SHADOW_OPACITY;
    shadows.locked = true;
    const groups = new ObjectGroup("groups");
    const exits = new ObjectGroup("exits");
    const loot = new ObjectGroup("loot");
    const arrivals = new ObjectGroup("arrivals");
    const villages = new ObjectGroup("villages");
    const start = new ObjectGroup("start");
    const mobs = new ObjectGroup("mobs");
    mobs.drawOrder = ObjectGroup.IndexOrder;
    mobs.locked = true;
    const characters = new ObjectGroup("characters");
    characters.drawOrder = ObjectGroup.IndexOrder;

    for (const item of drawn) {
        const { tile, shadowTile, offset, shadowOffset } = item.entry;
        if (shadowTile)
            addTileObject(shadows, shadowTile, item.x + shadowOffset[0], item.y + shadowOffset[1], {});
        const layer = item.preview ? mobs : characters;
        const c = item.character;
        addTileObject(layer, tile, item.x + offset[0], item.y + offset[1], {
            character: c.slot,
            group: c.group,
            classId: c.classId,
            level: c.level,
            direction: c.direction,
            nameId: c.nameId,
            spriteSet: c.spriteSet,
        }).name = `c${c.slot}`;
    }
    for (const g of entities.groups) {
        const common = { group: g.slot, kind: g.kind, size: g.size, maxSize: g.maxSize };
        addRect(groups, `g${g.slot} spawn`, g.spawn, Object.assign({}, common, { area: "spawn" }));
        addRect(groups, `g${g.slot} leash`, g.leash, Object.assign({}, common, { area: "leash" }));
    }
    for (const exit of entities.exits)
        addRect(exits, `→ ${exit.targetMap}`, exit.zone, {
            slot: exit.slot, targetMap: exit.targetMap, arrivalX: exit.arrivalX, arrivalY: exit.arrivalY,
            direction: exit.direction, unknown: exit.unknown,
        });
    for (const entry of entities.loot)
        addPoint(loot, `$ ${entry.gold}`, k1.characterPosition(entry.x, entry.y), {
            slot: entry.slot, gold: entry.gold, items: entry.items, record: entry.record,
        });
    for (const exit of entities.arrivals)
        addPoint(arrivals, `← ${exit.mapId}`, k1.characterPosition(exit.arrivalX, exit.arrivalY), {
            slot: exit.slot, fromMap: exit.mapId, direction: exit.direction,
        });
    for (const { village, job } of entities.jobs)
        addPoint(villages, job.role, k1.characterPosition(job.x, job.y), {
            village: village.slot, job: job.job, direction: job.direction,
        });
    // У отряда игрока прямоугольник появления — точка старта: top = y, left = x, bottom = направление
    if (entities.start)
        addPoint(start, "start", k1.characterPosition(entities.start.spawn.left, entities.start.spawn.top), {
            direction: entities.start.spawn.bottom,
        });

    if (charactersTileset.tileCount)
        map.addTileset(charactersTileset);
    if (shadowsTileset.tileCount)
        map.addTileset(shadowsTileset);
    for (const layer of [shadows, groups, exits, loot, arrivals, villages, start, mobs, characters])
        group.addLayer(layer);
    return group;
}

// Огни объектов: тайлсет flames — тайл на каждый кадр FLAMES.RES, у первого кадра
// каждого вида анимация по тику; слой flames над объектами выводится из них
// и при сохранении не читается (docs/k1/tiled.md)
function addFlames(map, directory, objects, placed) {
    const frames = loadFlames(directory);
    const animations = loadTables(directory).flameAnimations;
    if (!frames || !animations)
        return;
    const tileset = buildTileset("flames", frames.length, index => frames[index], index => ({ frame: index }));
    animations.forEach(([first, last], kind) => {
        const tile = tileset.tile(first);
        tile.setProperty("kind", kind);
        const sequence = [];
        for (let index = first; index <= last; ++index)
            sequence.push({ tileId: index, duration: k1.TICK_MS });
        tile.frames = sequence;
    });
    map.addTileset(tileset);

    const layer = new ObjectGroup("flames");
    layer.locked = true;
    layer.drawOrder = ObjectGroup.IndexOrder;
    for (const object of placed) {
        const entry = objects[object.objectId];
        if (!entry || !entry.flames.length)
            continue;
        const [x, y] = k1.objectPosition(object.x, object.y);
        for (const flame of entry.flames) {
            const range = animations[flame.kind];
            if (range)
                addTileObject(layer, tileset.tile(range[0]), x + flame.x, y + flame.y, { slot: object.slot });
        }
    }
    map.addLayer(layer);
}

function addObjects(map, objects, shadowsTileset, objectsTileset, placed) {
    const drawn = [];
    for (const object of placed) {
        const frame = objects[object.objectId];
        if (!frame)
            throw new Error(`объект ${object.objectId} (слот ${object.slot}) не найден в Objects.res`);
        const [x, y] = k1.objectPosition(object.x, object.y);
        drawn.push({ object, frame, x, y });
    }
    // Ближе к зрителю то, что ниже на экране: порядок по нижнему краю, как в игре;
    // при равенстве — по слоту в файле карты
    const bottom = item => item.y + item.frame.bottom;
    drawn.sort((a, b) => bottom(a) - bottom(b) || a.object.slot - b.object.slot);

    // Тени выводятся из объектов: при сохранении слой shadows не читается
    const shadows = new ObjectGroup("shadows");
    shadows.opacity = SHADOW_OPACITY;
    shadows.locked = true;
    const group = new ObjectGroup("objects");
    group.drawOrder = ObjectGroup.IndexOrder;
    for (const { object, frame, x, y } of drawn) {
        if (frame.shadow)
            addTileObject(shadows, shadowsTileset.tile(object.objectId),
                x + frame.shadowOffset[0], y + frame.shadowOffset[1], { slot: object.slot });
        addTileObject(group, objectsTileset.tile(object.objectId), x + frame.offset[0], y + frame.offset[1], {
            slot: object.slot,
            flags: object.flags,
            frame: object.frame,
            frameTime: object.frameTime,
            flames: object.flames,
            unknown: object.unknown,
        });
    }
    map.addLayer(shadows);
    map.addLayer(group);
}

function layerByName(map, name, isTileLayer) {
    for (let i = 0; i < map.layerCount; ++i) {
        const layer = map.layerAt(i);
        if (layer.name === name && (isTileLayer ? layer.isTileLayer : layer.isObjectLayer))
            return layer;
    }
    throw new Error(`нет ${isTileLayer ? "тайлового" : "объектного"} слоя «${name}»`);
}

function propertyOr(object, name, fallback) {
    const value = object.property(name);
    return value === undefined || value === null || value === "" ? fallback : value;
}

function graphTileId(tile, where) {
    if (!tile || !tile.tileset || tile.tileset.name !== "graph")
        throw new Error(`${where}: тайл не из тайлсета graph`);
    return tile.id;
}

function collectMap(map) {
    if (map.width !== k1.MAP_WIDTH || map.height !== k1.MAP_HEIGHT)
        throw new Error(`карта должна быть ${k1.MAP_WIDTH}×${k1.MAP_HEIGHT}, а она ${map.width}×${map.height}`);
    // Исходные байты проходимости; слои foot и foot-arrows меняют в них только
    // то, что правили. Клетки у края без тайла берутся отсюда как есть
    const foot = k1.fromHex(map.property("foot"), k1.FOOT_SIZE);
    for (const { name, flag } of FOOT_LAYERS) {
        const layer = layerByName(map, name, true);
        for (let y = 0; y < k1.MAP_HEIGHT; ++y)
            for (let x = 0; x < k1.MAP_WIDTH; ++x) {
                const tile = layer.tileAt(x, y);
                if (tile && (!tile.tileset || tile.tileset.name !== name))
                    throw new Error(`${name} (${x}, ${y}): тайл не из тайлсета ${name}`);
                k1.applyFootMask(foot, x, y, flag, tile ? tile.id : 0);
            }
    }

    const layers = {};
    for (const name of ["ground", "overlay"]) {
        const layer = layerByName(map, name, true);
        layers[name] = [];
        for (let y = 0; y < k1.MAP_HEIGHT; ++y) {
            const row = [];
            for (let x = 0; x < k1.MAP_WIDTH; ++x) {
                const tile = layer.tileAt(x, y);
                row.push(tile ? graphTileId(tile, `${name} (${x}, ${y})`) + 1 : 0);
                if (row[x] > k1.MAX_LAYER_TILE + 1)
                    throw new Error(`${name} (${x}, ${y}): тайл ${row[x] - 1} не помещается в байт слоя`);
            }
            layers[name].push(row);
        }
    }

    // Порядок слоя декораций — порядок отрисовки в игре и порядок слотов
    const decorations = [];
    for (const object of layerByName(map, "decorations", false).objects) {
        const id = graphTileId(object.tile, `декорация «${object.name || object.id}»`);
        const cellLayer = object.property("cellLayer");
        if (cellLayer) {
            layers[cellLayer][object.property("cellY")][object.property("cellX")] = id + 1;
            continue;
        }
        decorations.push({
            tile: id,
            x: Math.round(object.x),
            y: Math.round(object.y),
            unknown: propertyOr(object, "unknown", NEW_DECORATION_UNKNOWN),
        });
    }

    // Объекты игра сортирует сама, поэтому слоты — в прежнем порядке, новые — в конце
    const placed = layerByName(map, "objects", false).objects.map((object, index) => {
        const tile = object.tile;
        if (!tile || !tile.tileset || tile.tileset.name !== "objects")
            throw new Error(`объект «${object.name || object.id}»: тайл не из тайлсета objects`);
        const [x, y] = k1.objectCell(object.x - tile.property("offsetX"), object.y - tile.property("offsetY"));
        return {
            order: [propertyOr(object, "slot", Infinity), index],
            objectId: tile.property("objectId"),
            x,
            y,
            unknown: propertyOr(object, "unknown", NEW_OBJECT.unknown),
            flames: propertyOr(object, "flames", NEW_OBJECT.flames),
            frameTime: propertyOr(object, "frameTime", NEW_OBJECT.frameTime),
            flags: propertyOr(object, "flags", NEW_OBJECT.flags),
            frame: propertyOr(object, "frame", NEW_OBJECT.frame),
        };
    });
    placed.sort((a, b) => a.order[0] - b.order[0] || a.order[1] - b.order[1]);

    return { ground: layers.ground, overlay: layers.overlay, foot, decorations, objects: placed };
}

function sublayer(group, name) {
    for (let i = 0; i < group.layerCount; ++i) {
        const layer = group.layerAt(i);
        if (layer.name === name && layer.isObjectLayer)
            return layer;
    }
    throw new Error(`${group.name}: нет объектного слоя «${name}»`);
}

function integerProperty(object, name, fallback, min, max) {
    const value = propertyOr(object, name, fallback);
    if (!Number.isInteger(value) || value < min || value > max)
        throw new Error(`«${object.name || object.id}»: ${name} = ${value}, нужно целое от ${min} до ${max}`);
    return value;
}

const DIRECTION_MAX = 7;

// Правки слоёв game.K переносятся в состояние, прочитанное с диска
function applyGameEdits(state, group, number, hero) {
    const entities = k1.entitiesOnMap(state, number, hero);
    const where = name => `${group.name}/${name}`;

    // Персонажи: только место и направление; добавлять и удалять нельзя —
    // члены отряда лежат подряд, свободные слоты не разобраны
    const fixed = new Map(entities.characters.map(c => [c.slot, c]));
    const seen = new Set();
    for (const object of sublayer(group, "characters").objects) {
        const tile = object.tile;
        const slot = tile ? tile.property("character") : undefined;
        const character = fixed.get(slot);
        if (!character || seen.has(slot))
            throw new Error(`${where("characters")}: «${object.name || object.id}» — не персонаж этой карты; добавлять персонажей нельзя`);
        seen.add(slot);
        [character.x, character.y] = k1.characterCell(object.x - tile.property("offsetX"), object.y - tile.property("offsetY"));
        character.direction = integerProperty(object, "direction", character.direction, 0, DIRECTION_MAX);
    }
    if (seen.size !== fixed.size)
        throw new Error(`${where("characters")}: персонажей удалять нельзя`);

    // Отряды: прямоугольники появления и погони
    const groups = new Map(entities.groups.map(g => [g.slot, g]));
    const areas = new Set();
    for (const object of sublayer(group, "groups").objects) {
        const g = groups.get(object.property("group"));
        const area = object.property("area");
        const key = `${object.property("group")}/${area}`;
        if (!g || (area !== "spawn" && area !== "leash") || areas.has(key))
            throw new Error(`${where("groups")}: «${object.name || object.id}» — не прямоугольник отряда этой карты`);
        areas.add(key);
        g[area] = k1.rectCells(object.x, object.y, object.width, object.height);
    }
    if (areas.size !== groups.size * 2)
        throw new Error(`${where("groups")}: прямоугольники отрядов удалять нельзя`);

    if (entities.start) {
        const points = sublayer(group, "start").objects;
        if (points.length !== 1)
            throw new Error(`${where("start")}: нужна ровно одна точка старта`);
        const [x, y] = k1.characterCell(points[0].x, points[0].y);
        entities.start.spawn.left = x;
        entities.start.spawn.top = y;
        entities.start.spawn.bottom = integerProperty(points[0], "direction", entities.start.spawn.bottom, 0, DIRECTION_MAX);
    }

    // Клады и переходы этой карты пересобираются: прежние на своих местах в списке,
    // новые — в конце. Копия объекта со старым slot считается новой
    const rebuild = (list, layerName, isOwn, update, create) => {
        const own = new Map(list.filter(isOwn).map(entry => [entry.slot, entry]));
        const kept = new Set(), added = [];
        for (const object of sublayer(group, layerName).objects) {
            const slot = object.property("slot");
            let entry = own.get(slot);
            if (!entry || kept.has(slot)) {
                entry = create(object);
                added.push(entry);
            } else {
                kept.add(slot);
            }
            update(entry, object);
        }
        return list.filter(entry => !isOwn(entry) || kept.has(entry.slot)).concat(added);
    };
    state.loot = rebuild(state.loot, "loot", entry => entry.mapId === number,
        (entry, object) => {
            [entry.x, entry.y] = k1.characterCell(object.x, object.y);
            entry.gold = integerProperty(object, "gold", entry.gold, -32768, 32767);
            entry.items = propertyOr(object, "items", entry.items);
        },
        () => ({ slot: null, mapId: number, gold: 0, items: k1.NEW_LOOT_ITEMS, record: k1.NEW_LOOT_RECORD }));
    state.exits = rebuild(state.exits, "exits", entry => entry.mapId === number,
        (entry, object) => {
            entry.zone = k1.rectCells(object.x, object.y, object.width, object.height);
            entry.targetMap = integerProperty(object, "targetMap", entry.targetMap, 1, 255);
            entry.arrivalX = integerProperty(object, "arrivalX", entry.arrivalX, 0, 159);
            entry.arrivalY = integerProperty(object, "arrivalY", entry.arrivalY, 0, 319);
            entry.direction = integerProperty(object, "direction", entry.direction, 0, DIRECTION_MAX);
        },
        () => ({ slot: null, mapId: number, targetMap: 0, arrivalX: 0, arrivalY: 0, direction: 0, unknown: "0000" }));

    // Прибытия сюда с других карт: место прибытия перехода по его slot.
    // Применяются после переходов: точка в arrivals главнее свойств arrivalX/Y
    for (const object of sublayer(group, "arrivals").objects) {
        const exit = state.exits.find(e => e.slot === object.property("slot") && e.targetMap === number);
        if (!exit)
            throw new Error(`${where("arrivals")}: «${object.name || object.id}» — нет такого перехода на эту карту`);
        [exit.arrivalX, exit.arrivalY] = k1.characterCell(object.x, object.y);
        exit.direction = integerProperty(object, "direction", exit.direction, 0, DIRECTION_MAX);
    }

    for (const object of sublayer(group, "villages").objects) {
        const village = state.villages.find(v => v.slot === object.property("village") && v.mapId === number);
        const job = village && village.jobs[object.property("job")];
        if (!job)
            throw new Error(`${where("villages")}: «${object.name || object.id}» — не рабочее место деревни этой карты`);
        [job.x, job.y] = k1.characterCell(object.x, object.y);
        job.direction = integerProperty(object, "direction", job.direction, 0, DIRECTION_MAX);
    }
}

// Сумма Game.K после своего сохранения: «файл чужой» — только если он не совпадает
// ни с открытым, ни с записанным этой картой. Ключ — путь карты и имя группы
const savedChecksums = new Map();

// Game.K для каждой группы game.K: берётся с диска рядом с сохраняемой картой
// (иначе — рядом с открытой), получает правки этой карты и пишется рядом с сохраняемой
function collectGameStates(map, fileName) {
    const number = map.property("mapNumber");
    const target = FileInfo.path(fileName);
    const source = map.fileName ? FileInfo.path(map.fileName) : target;
    const result = [];
    for (let i = 0; i < map.layerCount; ++i) {
        const group = map.layerAt(i);
        const match = /^game\.(\d+)$/.exec(group.name);
        if (!group.isGroupLayer || !match)
            continue;
        const hero = Number(match[1]);
        let path;
        try {
            path = findFile(target, `game.${hero}`);
        } catch (error) {
            path = findFile(source, `game.${hero}`);
        }
        const buffer = readBuffer(path);
        const key = `${fileName}|${group.name}`;
        const current = checksum(buffer);
        if (current !== group.property("checksum") && current !== savedChecksums.get(key))
            throw new Error(`${FileInfo.fileName(path)} изменился после открытия карты (сохранена другая карта?) — откройте карту заново`);
        const state = k1.parseGameState(buffer);
        applyGameEdits(state, group, number, hero);
        const output = path.startsWith(`${target}/`) ? path : `${target}/${FileInfo.fileName(path)}`;
        result.push({ key, path: output, buffer: k1.serializeGameState(state) });
    }
    return result;
}

function writeBuffer(path, buffer) {
    const file = new BinaryFile(path, BinaryFile.WriteOnly);
    file.write(buffer);
    file.commit();
}

// Сначала собирается всё, потом пишется: ошибка в правках не оставит
// Konung.N и Game.N рассогласованными
function writeMap(map, fileName) {
    try {
        const buffer = k1.serializeMap(collectMap(map));
        const states = collectGameStates(map, fileName);
        writeBuffer(fileName, buffer);
        for (const { key, path, buffer: state } of states) {
            writeBuffer(path, state);
            savedChecksums.set(key, checksum(state));
        }
    } catch (error) {
        return `Карта не сохранена: ${error.message}`;
    }
    return undefined;
}

for (let number = 1; number <= MAX_MAP_NUMBER; ++number) {
    tiled.registerMapFormat(`konung1-${number}`, {
        name: `Князь, карта ${number}`,
        extension: String(number),
        read: readMap,
        write: writeMap,
    });
}
