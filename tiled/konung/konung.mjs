// Расширение Tiled: открывает оригинальные карты «Князя» (Konung.N) как обычные
// карты Tiled. Только чтение. Раскладка слоёв — docs/k1/tiled.md.

import * as k1 from "./lib/k1.mjs";

// Tiled выбирает скриптовый формат по суффиксу «.расширение», а у карт Князя
// расширение — номер локации. Поэтому формат регистрируется на каждый номер.
const MAX_MAP_NUMBER = 50;

// Тени — чёрная маска под слоем объектов; Python-рендер берёт ту же прозрачность
const SHADOW_OPACITY = 0.4;

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

// Декодирование GRAPH.RES — секунды, поэтому кэш на сессию Tiled
const graphCache = new Map();

function loadGraph(directory) {
    const path = findFile(directory, "graph.res");
    if (!graphCache.has(path))
        graphCache.set(path, k1.parseGraph(readBuffer(path)));
    return graphCache.get(path);
}

function buildGraphTileset(graph) {
    const tileset = new Tileset("graph");
    tileset.objectAlignment = Tileset.TopLeft;
    const empty = [];
    // Номер тайла Tiled совпадает с номером в GRAPH.RES (с нуля)
    for (let index = 0; index < graph.tiles.length; ++index) {
        const tile = tileset.addTile();
        const decoded = graph.tiles[index];
        if (decoded)
            tile.setImage(toImage(decoded));
        else
            empty.push(tile);
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

    const map = new TileMap();
    map.orientation = TileMap.Staggered;
    map.staggerAxis = TileMap.StaggerY;
    map.staggerIndex = TileMap.StaggerOdd;
    map.setSize(k1.MAP_WIDTH, k1.MAP_HEIGHT);
    map.setTileSize(k1.TILE_STEP_X, k1.GROUND_TILE_HEIGHT);
    map.backgroundColor = "#000000";
    setProperties(map, { source: FileInfo.fileName(fileName) });

    const graphTileset = buildGraphTileset(graph);
    map.addTileset(graphTileset);

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

    addObjects(map, directory, graph, data.objects);
    return map;
}

function addObjects(map, directory, graph, placed) {
    const catalog = k1.parseObjectsCatalog(readBuffer(findFile(directory, "objects.res")));
    const objectsTileset = new Tileset("objects");
    objectsTileset.objectAlignment = Tileset.TopLeft;
    const shadowsTileset = new Tileset("shadows");
    shadowsTileset.objectAlignment = Tileset.TopLeft;

    const frames = new Map();
    function frameTiles(objectId, frameIndex) {
        const key = `${objectId}:${frameIndex}`;
        if (frames.has(key))
            return frames.get(key);
        let result = null;
        const simple = k1.readSimpleObject(catalog, graph, objectId);
        if (simple) {
            const frame = simple.frames[frameIndex]?.image ? simple.frames[frameIndex] : simple.frames[0];
            if (frame.image) {
                const tile = objectsTileset.addTile();
                tile.setImage(toImage(frame.image));
                setProperties(tile, { objectId, frame: frameIndex });
                let shadow = null;
                if (frame.shadow) {
                    shadow = shadowsTileset.addTile();
                    shadow.setImage(toImage(frame.shadow));
                    setProperties(shadow, { objectId, frame: frameIndex });
                }
                result = { tile, shadow, frame };
            }
        }
        frames.set(key, result);
        return result;
    }

    const drawn = [];
    for (const object of placed) {
        const tiles = frameTiles(object.objectId, object.frame);
        if (!tiles)
            continue;
        const [x, y] = k1.objectPosition(object.x, object.y);
        drawn.push({ object, tiles, x, y });
    }
    // Ближе к зрителю то, что ниже на экране: порядок по нижнему краю, как в игре;
    // при равенстве — по слоту в файле карты
    const bottom = item => item.y + item.tiles.frame.offset[1] + item.tiles.frame.image.height;
    drawn.sort((a, b) => bottom(a) - bottom(b) || a.object.slot - b.object.slot);

    map.addTileset(shadowsTileset);
    map.addTileset(objectsTileset);

    const shadows = new ObjectGroup("shadows");
    shadows.opacity = SHADOW_OPACITY;
    const objects = new ObjectGroup("objects");
    objects.drawOrder = ObjectGroup.IndexOrder;
    for (const { object, tiles, x, y } of drawn) {
        const { frame } = tiles;
        if (tiles.shadow)
            addTileObject(shadows, tiles.shadow, x + frame.shadowOffset[0], y + frame.shadowOffset[1],
                { slot: object.slot, objectId: object.objectId });
        addTileObject(objects, tiles.tile, x + frame.offset[0], y + frame.offset[1], {
            slot: object.slot,
            objectId: object.objectId,
            cellX: object.x,
            cellY: object.y,
            flags: object.flags,
            frame: object.frame,
            frameTime: object.frameTime,
            flames: object.flames,
            unknown: object.unknown,
        });
    }
    map.addLayer(shadows);
    map.addLayer(objects);
}

for (let number = 1; number <= MAX_MAP_NUMBER; ++number) {
    tiled.registerMapFormat(`konung1-${number}`, {
        name: `Князь, карта ${number}`,
        extension: String(number),
        read: readMap,
    });
}
