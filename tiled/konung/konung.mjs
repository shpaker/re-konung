// Расширение Tiled: открывает оригинальные карты «Князя» (Konung.N) как обычные
// карты Tiled и сохраняет их обратно. Меняются только тайлы, декорации и
// объекты карты; GRAPH.RES и Objects.res лишь читаются. Слои — docs/k1/tiled.md.

import * as k1 from "./lib/k1.mjs";

// Tiled выбирает скриптовый формат по суффиксу «.расширение», а у карт Князя
// расширение — номер локации. Поэтому формат регистрируется на каждый номер.
const MAX_MAP_NUMBER = 50;

// Тени — чёрная маска под слоем объектов; Python-рендер берёт ту же прозрачность
const SHADOW_OPACITY = 0.4;

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
const objectsCache = new Map();

function loadGraph(directory) {
    const path = findFile(directory, "graph.res");
    if (!graphCache.has(path))
        graphCache.set(path, k1.parseGraph(readBuffer(path)));
    return graphCache.get(path);
}

// Кадр 0 каждого простого объекта: в картах дистрибутива кадр всегда 0 или 0xFF
function loadObjects(directory, graph) {
    const path = findFile(directory, "objects.res");
    if (!objectsCache.has(path)) {
        const catalog = k1.parseObjectsCatalog(readBuffer(path));
        const objects = [];
        for (let objectId = 0; objectId < k1.SIMPLE_OBJECT_SLOTS; ++objectId) {
            const simple = k1.readSimpleObject(catalog, graph, objectId);
            if (simple && simple.frames[0].image)
                objects[objectId] = simple.frames[0];
        }
        objectsCache.set(path, objects);
    }
    return objectsCache.get(path);
}

// Тайлсет-коллекция, где номер тайла Tiled совпадает с номером в источнике
function buildTileset(name, count, imageOf, propertiesOf) {
    const tileset = new Tileset(name);
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
    // Проходимость Tiled не показывает, но при сохранении она нужна целиком
    setProperties(map, { source: FileInfo.fileName(fileName), foot: k1.toHex(data.foot) });

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
    return map;
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
    const bottom = item => item.y + item.frame.offset[1] + item.frame.image.height;
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
    const foot = k1.fromHex(map.property("foot"), k1.FOOT_SIZE);

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

function writeMap(map, fileName) {
    try {
        const buffer = k1.serializeMap(collectMap(map));
        const file = new BinaryFile(fileName, BinaryFile.WriteOnly);
        file.write(buffer);
        file.commit();
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
