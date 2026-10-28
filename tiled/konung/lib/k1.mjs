// Разбор и запись ресурсов «Князя» (1999) без Tiled API: на входе ArrayBuffer,
// на выходе простые объекты, и обратно для карт. Повторяет эталон konung/k1 на
// Python; при расхождении прав Python — у него тесты. Форматы — docs/k1/formats/.

export const MAP_WIDTH = 80;
export const MAP_HEIGHT = 160;
const FINE_WIDTH = 160;
const FINE_HEIGHT = 320;
export const FOOT_SIZE = FINE_WIDTH * FINE_HEIGHT;
export const MAX_DECORATIONS = 1000;
export const MAX_OBJECTS = 1000;
const DECORATION_SIZE = 16;
const OBJECT_SIZE = 36;
// Номер тайла в тайловом слое — один байт: номер в GRAPH.RES + 1
export const MAX_LAYER_TILE = 254;
export const MAP_SIZE =
    MAP_WIDTH * MAP_HEIGHT * 2 + FOOT_SIZE + MAX_DECORATIONS * DECORATION_SIZE + MAX_OBJECTS * OBJECT_SIZE;

// Ромб тайла 114×64, шаг по x — 116, шаг строки — 32, нечётные строки сдвинуты на 58
export const TILE_STEP_X = 116;
export const TILE_STEP_Y = 32;
export const GROUND_TILE_WIDTH = 114;
export const GROUND_TILE_HEIGHT = 64;

const PALETTE_BANK_COLORS = 256 * 256;
const CURSOR_COUNT = 11;
const TILE_SLOTS = 512;
const OBJECT_SLOTS = 512;
const DYNAMIC_OBJECTS = 30;
export const SIMPLE_OBJECT_SLOTS = OBJECT_SLOTS - DYNAMIC_OBJECTS;
const OBJECT_FRAMES = 8;
const FLAME_SLOTS = 20;

class Reader {
    constructor(buffer, pos = 0) {
        this.view = new DataView(buffer);
        this.bytes = new Uint8Array(buffer);
        this.pos = pos;
    }
    u8() { return this.view.getUint8(this.pos++); }
    s8() { return this.view.getInt8(this.pos++); }
    u16() { const v = this.view.getUint16(this.pos, true); this.pos += 2; return v; }
    s16() { const v = this.view.getInt16(this.pos, true); this.pos += 2; return v; }
    u32() { const v = this.view.getUint32(this.pos, true); this.pos += 4; return v; }
    s32() { const v = this.view.getInt32(this.pos, true); this.pos += 4; return v; }
    hex(count) {
        let out = "";
        for (let i = 0; i < count; ++i)
            out += this.bytes[this.pos + i].toString(16).padStart(2, "0");
        this.pos += count;
        return out;
    }
}

// RGB555 → 0xAARRGGBB, 5 бит растягиваются до 8 повтором старших битов
function argb(color) {
    const r = (color >> 10) & 0x1f, g = (color >> 5) & 0x1f, b = color & 0x1f;
    return (0xff000000 | (((r << 3) | (r >> 2)) << 16) | (((g << 3) | (g >> 2)) << 8) | ((b << 3) | (b >> 2))) >>> 0;
}

// Построчное RLE (docs/k1/formats/rle.md). pixel(reader) читает один пиксель
// и возвращает цвет ARGB; без pixel — тень: только маска.
function decodeRle(reader, pixel) {
    const width = reader.s16();
    const height = reader.s16();
    reader.pos += height * 2;
    const pixels = new Uint32Array(width * height);
    for (let y = 0; y < height; ++y) {
        let x = 0;
        for (;;) {
            const n = reader.u8();
            if (n === 0)
                break;
            if (n & 0x80) {
                x += n & 0x7f;
                continue;
            }
            if (x + n > width)
                throw new Error(`RLE: строка ${y} шире ${width}`);
            const start = y * width + x;
            for (let i = 0; i < n; ++i)
                pixels[start + i] = pixel ? pixel(reader) : 0xff000000;
            x += n;
        }
    }
    return { width, height, pixels };
}

function indexed(palette) {
    return reader => palette[reader.u8()];
}

export function parseGraph(buffer) {
    const reader = new Reader(buffer);
    const bank = new Uint32Array(PALETTE_BANK_COLORS);
    for (let i = 0; i < PALETTE_BANK_COLORS; ++i)
        bank[i] = argb(reader.u16());
    const palette = offset => bank.subarray(offset, offset + 256);

    for (let i = 0; i < CURSOR_COUNT; ++i) {
        reader.u32();
        const size = reader.u32();
        reader.pos += size;
    }

    reader.u32();
    // Размер картинок тайлов — от конца таблицы; за ними блоки персонажей
    const tilesSize = reader.u32();
    const entries = [];
    for (let i = 0; i < TILE_SLOTS; ++i)
        entries.push([reader.s32(), reader.s32()]);
    const dataStart = reader.pos;

    const tiles = [];
    entries.forEach(([offset, paletteOffset], index) => {
        if (offset < 0 || paletteOffset < 0)
            return;
        reader.pos = dataStart + offset;
        // Смещение палитры записано в байтах от начала банка
        tiles[index] = decodeRle(reader, indexed(palette(paletteOffset / 2)));
    });
    const characters = parseCharacterBlocks(buffer, dataStart + tilesSize);
    return { palette, tiles, characters };
}

// Каталог OBJECTS.RES: кадры простых объектов декодируются по запросу
export function parseObjectsCatalog(buffer) {
    const reader = new Reader(buffer, 8);
    const offsets = [];
    for (let i = 0; i < OBJECT_SLOTS; ++i) {
        offsets.push(reader.s32());
        reader.s32();
    }
    return { buffer, dataStart: reader.pos, offsets };
}

export function readSimpleObject(catalog, graph, objectId) {
    const offset = catalog.offsets[DYNAMIC_OBJECTS + objectId];
    if (offset === undefined || offset < 0)
        return null;
    const reader = new Reader(catalog.buffer, catalog.dataStart + offset);
    const flags = reader.u32();
    reader.u32();
    const flameCount = reader.s32();
    const frameCount = reader.u32();
    reader.u32();
    const paletteOffset = reader.u32() / 2;
    const frameTime = reader.s32();
    // Точки огней: вид и левый верхний угол кадра от позиции объекта
    const flames = [];
    for (let i = 0; i < flameCount; ++i)
        flames.push({ kind: reader.s8(), x: reader.s16(), y: reader.s16() });
    reader.pos += (FLAME_SLOTS - flameCount) * 5;

    const dataOffsets = [];
    for (let i = 0; i < OBJECT_FRAMES; ++i)
        dataOffsets.push([reader.s32(), reader.s32()]);
    const placement = [];
    for (let i = 0; i < OBJECT_FRAMES; ++i)
        placement.push({ shadow: [reader.s16(), reader.s16()], image: [reader.s16(), reader.s16()] });
    const dataStart = reader.pos;

    const pixel = indexed(graph.palette(paletteOffset));
    const frames = dataOffsets.map(([shadowAt, imageAt], i) => {
        let image = null, shadow = null;
        if (imageAt >= 0) {
            reader.pos = dataStart + imageAt;
            image = decodeRle(reader, pixel);
        }
        if (shadowAt >= 0) {
            reader.pos = dataStart + shadowAt;
            shadow = decodeRle(reader, null);
        }
        return { image, shadow, offset: placement[i].image, shadowOffset: placement[i].shadow };
    });
    return { objectId, flags, flameCount, flames, frameCount, frameTime, frames };
}

export function parseMap(buffer) {
    if (buffer.byteLength !== MAP_SIZE)
        throw new Error(`карта ${buffer.byteLength} байт, ожидалось ${MAP_SIZE}`);
    const reader = new Reader(buffer);
    const ground = [], overlay = [];
    for (let y = 0; y < MAP_HEIGHT; ++y) {
        const g = [], o = [];
        for (let x = 0; x < MAP_WIDTH; ++x) {
            g.push(reader.u8());
            o.push(reader.u8());
        }
        ground.push(g);
        overlay.push(o);
    }
    const foot = reader.bytes.slice(reader.pos, reader.pos + FOOT_SIZE);
    reader.pos += FOOT_SIZE;

    const decorations = [];
    for (let slot = 0; slot < MAX_DECORATIONS; ++slot) {
        const tile = reader.s32();
        const unknown = reader.hex(8);
        const x = reader.s16(), y = reader.s16();
        if (tile >= 0)
            decorations.push({ slot, tile, x, y, unknown });
    }

    const objects = [];
    for (let slot = 0; slot < MAX_OBJECTS; ++slot) {
        const start = reader.pos;
        const objectId = reader.s32();
        if (objectId >= 0) {
            const unknown = reader.hex(4);
            const flames = reader.hex(FLAME_SLOTS);
            const frameTime = reader.s16();
            const x = reader.s16(), y = reader.s16();
            const flags = reader.u8(), frame = reader.u8();
            objects.push({ slot, objectId, x, y, flames, frameTime, flags, frame, unknown });
        }
        reader.pos = start + OBJECT_SIZE;
    }
    return { ground, overlay, foot, decorations, objects };
}

export function toHex(bytes) {
    let out = "";
    for (const byte of bytes)
        out += byte.toString(16).padStart(2, "0");
    return out;
}

export function fromHex(text, length) {
    if (typeof text !== "string" || text.length !== length * 2 || /[^0-9a-f]/i.test(text))
        throw new Error(`ожидалось ${length} байт в шестнадцатеричном виде`);
    const bytes = new Uint8Array(length);
    for (let i = 0; i < length; ++i)
        bytes[i] = parseInt(text.substr(i * 2, 2), 16);
    return bytes;
}

// Карта обратно в Konung.N. Декорации и объекты — подряд с нулевого слота,
// остальные слоты — байты 0xFF, как во всех картах дистрибутива
export function serializeMap(data) {
    if (data.decorations.length > MAX_DECORATIONS)
        throw new Error(`декораций ${data.decorations.length}, максимум ${MAX_DECORATIONS}`);
    if (data.objects.length > MAX_OBJECTS)
        throw new Error(`объектов ${data.objects.length}, максимум ${MAX_OBJECTS}`);

    const bytes = new Uint8Array(MAP_SIZE).fill(0xff);
    const view = new DataView(bytes.buffer);
    let pos = 0;
    for (let y = 0; y < MAP_HEIGHT; ++y) {
        for (let x = 0; x < MAP_WIDTH; ++x) {
            bytes[pos++] = data.ground[y][x];
            bytes[pos++] = data.overlay[y][x];
        }
    }
    bytes.set(data.foot, pos);
    pos += FOOT_SIZE;

    const decorationsStart = pos;
    data.decorations.forEach((decoration, slot) => {
        const at = decorationsStart + slot * DECORATION_SIZE;
        view.setInt32(at, decoration.tile, true);
        bytes.set(fromHex(decoration.unknown, 8), at + 4);
        view.setInt16(at + 12, decoration.x, true);
        view.setInt16(at + 14, decoration.y, true);
    });

    const objectsStart = decorationsStart + MAX_DECORATIONS * DECORATION_SIZE;
    data.objects.forEach((object, slot) => {
        const at = objectsStart + slot * OBJECT_SIZE;
        view.setInt32(at, object.objectId, true);
        bytes.set(fromHex(object.unknown, 4), at + 4);
        bytes.set(fromHex(object.flames, FLAME_SLOTS), at + 8);
        view.setInt16(at + 28, object.frameTime, true);
        view.setInt16(at + 30, object.x, true);
        view.setInt16(at + 32, object.y, true);
        view.setUint8(at + 34, object.flags);
        view.setUint8(at + 35, object.frame);
    });
    return bytes.buffer;
}

// Левый верхний угол тайла клетки
export function tilePosition(column, row) {
    return [column * TILE_STEP_X + (row & 1) * (TILE_STEP_X / 2), row * TILE_STEP_Y];
}

// Позиция объекта: клетка мелкой сетки (вдвое мельче тайловой) → пиксели
export function objectPosition(x, y) {
    return [x * (TILE_STEP_X / 2) - (y & 1) * (TILE_STEP_X / 4), y * (TILE_STEP_Y / 2)];
}

// Обратно: пиксели → ближайшая клетка мелкой сетки
export function objectCell(x, y) {
    const row = Math.round(y / (TILE_STEP_Y / 2));
    return [Math.round((x + (row & 1) * (TILE_STEP_X / 4)) / (TILE_STEP_X / 2)), row];
}

// Байт проходимости: младшие 4 бита 0xF — непроходимо, флаг 0x40 — здесь
// застревают стрелы (гипотеза). В картах дистрибутива других значений нет
export const FOOT_FLAGS = {
    blocked: { test: value => (value & 0x0f) === 0x0f, set: value => value | 0x0f, clear: value => value & 0xf0 },
    arrows: { test: value => (value & 0x40) !== 0, set: value => value | 0x40, clear: value => value & ~0x40 },
};

// Четыре клетки мелкой сетки в ромбе тайла: верхняя, левая, правая, нижняя.
// Каждая клетка лежит ровно в одном тайле; клетки за краем возвращаются тоже
export function fineCells(column, row) {
    const x = 2 * column + (row & 1), y = 2 * row;
    return [[x, y], [x, y + 1], [x + 1, y + 1], [x, y + 2]];
}

// Середины этих клеток в картинке тайла 116 × 64; ромб клетки — 58 × 32
export const FINE_CELL_CENTERS = [[58, 16], [29, 32], [87, 32], [58, 48]];

function footIndex(x, y) {
    return x < FINE_WIDTH && y < FINE_HEIGHT ? y * FINE_WIDTH + x : -1;
}

// Маска тайла: бит i — у клетки i из fineCells стоит флаг
export function footMask(foot, column, row, flag) {
    let mask = 0;
    fineCells(column, row).forEach(([x, y], i) => {
        const at = footIndex(x, y);
        if (at >= 0 && flag.test(foot[at]))
            mask |= 1 << i;
    });
    return mask;
}

// Меняются только клетки, где маска разошлась с байтом: остальные биты
// байта и клетки, нетронутые в Tiled, остаются как в файле
export function applyFootMask(foot, column, row, flag, mask) {
    fineCells(column, row).forEach(([x, y], i) => {
        const at = footIndex(x, y);
        if (at < 0)
            return;
        const wanted = (mask >> i & 1) === 1;
        if (wanted !== flag.test(foot[at]))
            foot[at] = wanted ? flag.set(foot[at]) : flag.clear(foot[at]);
    });
}

// ---------------------------------------------------------------------------
// Свет: LIGHTS.N карты и маски LIGHTS.RES (docs/k1/formats/lights-res.md),
// эталон — konung/k1/map_file.py (parse_lights, serialize_lights) и lights_res.py

export const LIGHTS_SIZE = MAP_WIDTH * MAP_HEIGHT + FOOT_SIZE;
export const LIGHT_MASKS = 19;
export const LIGHT_MASK_WIDTH = 114;
export const LIGHT_MASK_HEIGHT = 64;
// Две маски в начале LIGHTS.RES (64 × 64 и 64 × 43): Slavik их не использует
const LIGHTS_RES_TILES_AT = 64 * 64 + 64 * 43;
const LIGHTS_RES_SIZE = LIGHTS_RES_TILES_AT + LIGHT_MASKS * LIGHT_MASK_WIDTH * LIGHT_MASK_HEIGHT;
// Slavik переводит яркость в 0–255 как 255 / 100 · v
const LIGHT_BRIGHTNESS_SCALE = 100;

// Маски тайлов: яркость на пиксель
export function parseLightsRes(buffer) {
    if (buffer.byteLength !== LIGHTS_RES_SIZE)
        throw new Error(`LIGHTS.RES ${buffer.byteLength} байт, ожидалось ${LIGHTS_RES_SIZE}`);
    const bytes = new Uint8Array(buffer);
    const size = LIGHT_MASK_WIDTH * LIGHT_MASK_HEIGHT;
    const masks = [];
    for (let i = 0; i < LIGHT_MASKS; ++i)
        masks.push(bytes.slice(LIGHTS_RES_TILES_AT + i * size, LIGHTS_RES_TILES_AT + (i + 1) * size));
    return masks;
}

// Картинка маски цветом color (0xRRGGBB): прозрачность — яркость. Маска заполняет
// весь прямоугольник, а игра светит ею только пиксели тайла — вне ромба прозрачно
export function lightImage(mask, color) {
    const width = LIGHT_MASK_WIDTH, height = LIGHT_MASK_HEIGHT;
    const pixels = new Uint32Array(width * height);
    for (let y = 0; y < height; ++y)
        for (let x = 0; x < width; ++x)
            if (Math.abs(x + 0.5 - width / 2) / (width / 2) + Math.abs(y + 0.5 - height / 2) / (height / 2) <= 1) {
                const alpha = Math.floor(mask[y * width + x] * 255 / LIGHT_BRIGHTNESS_SCALE);
                pixels[y * width + x] = ((alpha << 24) | color) >>> 0;
            }
    return { width, height, pixels };
}

// Номер маски + 1 на клетку тайловой сетки, 0 — света нет: [строка][столбец]
export function parseLights(buffer) {
    if (buffer.byteLength !== LIGHTS_SIZE)
        throw new Error(`свет ${buffer.byteLength} байт, ожидалось ${LIGHTS_SIZE}`);
    const bytes = new Uint8Array(buffer);
    const tiles = [];
    for (let y = 0; y < MAP_HEIGHT; ++y)
        tiles.push(Array.from(bytes.subarray(y * MAP_WIDTH, (y + 1) * MAP_WIDTH)));
    return tiles;
}

// Обратно в LIGHTS.N. Мелкий блок — номер маски тайла на 4 его клетки
// (fineCells), у края — 0: так устроены все файлы дистрибутива
export function serializeLights(tiles) {
    const bytes = new Uint8Array(LIGHTS_SIZE);
    const fineAt = MAP_WIDTH * MAP_HEIGHT;
    for (let y = 0; y < MAP_HEIGHT; ++y)
        for (let x = 0; x < MAP_WIDTH; ++x) {
            const value = tiles[y][x];
            if (!Number.isInteger(value) || value < 0 || value > LIGHT_MASKS)
                throw new Error(`свет (${x}, ${y}): маска ${value}, нужно от 0 до ${LIGHT_MASKS}`);
            bytes[y * MAP_WIDTH + x] = value;
            for (const [fx, fy] of fineCells(x, y)) {
                const at = footIndex(fx, fy);
                if (at >= 0)
                    bytes[fineAt + at] = value;
            }
        }
    return bytes.buffer;
}

// ---------------------------------------------------------------------------
// Game.N — начальное состояние новой игры (docs/k1/formats/game-state.md),
// эталон — konung/k1/game_state.py. Запись копирует исходный файл и меняет
// только то, что правится в Tiled: места, направления, прямоугольники,
// клады и переходы. Клады и переходы пересобираются подряд с нулевого слота.

const ITEM_SLOTS = 8192, ITEM_SIZE = 16;
const GROUP_SLOTS = 200, GROUP_SIZE = 32;
export const LOOT_SLOTS = 500;
const LOOT_SIZE = 64;
export const EXIT_SLOTS = 250;
const EXIT_SIZE = 17;
const CHARACTER_SLOTS = 2000, CHARACTER_SIZE = 256;
const VILLAGE_SLOTS = 20, VILLAGE_SIZE = 1205;
const GROUPS_AT = ITEM_SLOTS * ITEM_SIZE;
const LOOT_AT = GROUPS_AT + GROUP_SLOTS * GROUP_SIZE;
const EXITS_AT = LOOT_AT + LOOT_SLOTS * LOOT_SIZE;
const CHARACTERS_AT = EXITS_AT + EXIT_SLOTS * EXIT_SIZE;
const VILLAGES_AT = CHARACTERS_AT + CHARACTER_SLOTS * CHARACTER_SIZE;
export const GAME_STATE_SIZE = VILLAGES_AT + VILLAGE_SLOTS * VILLAGE_SIZE;
const JOBS_AT = 0x3d0, JOB_SIZE = 8;
export const JOB_ROLES = ["elder", "healer", "merchant", "smith", "voevoda"];
// В кладе предметы — 22 × s16 с 0x14
const LOOT_ITEMS_AT = 0x14, LOOT_ITEMS_SIZE = 44;
export const NEW_LOOT_ITEMS = "00".repeat(LOOT_ITEMS_SIZE);
export const NEW_LOOT_RECORD = "00".repeat(LOOT_SIZE);

export const PLAYER_GROUP = 0x20;
const FIRST_FIXED_GROUP = 0x51;
const VILLAGE_GROUP_BIT = 0x80;
const DYNAMIC_SPRITE_BIT = 0x40;

function isFilled(bytes, at, size, value) {
    for (let i = 0; i < size; ++i)
        if (bytes[at + i] !== value)
            return false;
    return true;
}

function rect(left, top, right, bottom) {
    return { left, top, right, bottom };
}

export function parseGameState(buffer) {
    if (buffer.byteLength !== GAME_STATE_SIZE)
        throw new Error(`Game.N ${buffer.byteLength} байт, ожидалось ${GAME_STATE_SIZE}`);
    const bytes = new Uint8Array(buffer);
    const view = new DataView(buffer);
    const s16 = at => view.getInt16(at, true);

    // Предмет нужен только для снаряжения: номер описания (байт 3); пустой — 16 × 0xFF
    const itemInfo = slot => {
        const at = slot * ITEM_SIZE;
        return isFilled(bytes, at, ITEM_SIZE, 0xff) ? null : view.getInt8(at + 3);
    };
    // Вид предмета (байт 0): по нему выбирается таблица названий (itemName)
    const itemType = slot => {
        const at = slot * ITEM_SIZE;
        return isFilled(bytes, at, ITEM_SIZE, 0xff) ? null : view.getInt8(at);
    };

    // Прямоугольники отряда в файле вперемешку: top, top, bottom, bottom, left, left, right, right
    const groups = [];
    for (let slot = 0; slot < GROUP_SLOTS; ++slot) {
        const at = GROUPS_AT + slot * GROUP_SIZE;
        if (isFilled(bytes, at, GROUP_SIZE, 0))
            continue;
        const kind = bytes[at + 0x1e];
        groups.push({
            slot,
            firstCharacter: s16(at),
            spawn: rect(s16(at + 0x10), s16(at + 0x08), s16(at + 0x14), s16(at + 0x0c)),
            leash: rect(s16(at + 0x12), s16(at + 0x0a), s16(at + 0x16), s16(at + 0x0e)),
            maxSize: bytes[at + 0x18],
            mapId: bytes[at + 0x1b],
            size: bytes[at + 0x1c],
            kind,
            roaming: kind < FIRST_FIXED_GROUP && !(kind & VILLAGE_GROUP_BIT),
        });
    }

    const characters = [];
    for (let slot = 0; slot < CHARACTER_SLOTS; ++slot) {
        const at = CHARACTERS_AT + slot * CHARACTER_SIZE;
        if (isFilled(bytes, at, CHARACTER_SIZE, 0))
            continue;
        const equipment = [];
        for (let i = 0; i < 6; ++i)
            equipment.push(s16(at + 0x66 + i * 2));
        characters[slot] = {
            slot,
            state: bytes[at],
            direction: bytes[at + 1],
            classId: bytes[at + 4],
            group: bytes[at + 5],
            y: s16(at + 8),
            x: s16(at + 0x0a),
            frame: bytes[at + 0x10],
            level: bytes[at + 0x33],
            equipment,
            nameId: bytes[at + 0xd4],
            nickId: bytes[at + 0xd5],
            spriteSet: bytes[at + 0xd6],
            // Смещение в байтах → номер цвета в банке
            paletteOffset: view.getInt32(at + 0xd8, true) / 2,
            dynamic: !!(bytes[at + 4] & DYNAMIC_SPRITE_BIT),
        };
    }

    // Список кладов и переходов обрывает первая запись с картой 0, как в игре
    const loot = [];
    for (let slot = 0; slot < LOOT_SLOTS; ++slot) {
        const at = LOOT_AT + slot * LOOT_SIZE;
        const mapId = s16(at + 0x12);
        if (mapId === 0)
            break;
        loot.push({
            slot, mapId, y: s16(at + 8), x: s16(at + 0x0a), gold: s16(at + 0x10),
            items: toHex(bytes.subarray(at + LOOT_ITEMS_AT, at + LOOT_ITEMS_AT + LOOT_ITEMS_SIZE)),
            record: toHex(bytes.subarray(at, at + LOOT_SIZE)),
        });
    }
    const exits = [];
    for (let slot = 0; slot < EXIT_SLOTS; ++slot) {
        const at = EXITS_AT + slot * EXIT_SIZE;
        const targetMap = bytes[at + 4];
        if (targetMap === 0)
            break;
        exits.push({
            slot,
            direction: bytes[at + 2],
            mapId: view.getInt8(at + 3),
            targetMap,
            arrivalY: s16(at + 5),
            arrivalX: s16(at + 7),
            zone: rect(s16(at + 0x0b), s16(at + 9), s16(at + 0x0f), s16(at + 0x0d)),
            unknown: toHex(bytes.subarray(at, at + 2)),
        });
    }

    const villages = [];
    for (let slot = 0; slot < VILLAGE_SLOTS; ++slot) {
        const at = VILLAGES_AT + slot * VILLAGE_SIZE;
        if (isFilled(bytes, at, VILLAGE_SIZE, 0))
            continue;
        const jobs = JOB_ROLES.map((role, job) => {
            const jobAt = at + JOBS_AT + job * JOB_SIZE;
            return {
                job, role, direction: bytes[jobAt], character: s16(jobAt + 2),
                y: s16(jobAt + 4), x: s16(jobAt + 6),
            };
        });
        villages.push({ slot, mapId: bytes[at + 3], jobs });
    }
    return { buffer, itemInfo, itemType, groups, characters, loot, exits, villages };
}

// Карта, где начинается новая игра за героя: карту в отряде игрока игра не читает
export function startMap(hero) {
    return 3 * hero + 1;
}

// Что стоит на карте number (как game_state.on_map)
export function entitiesOnMap(state, number, hero) {
    const player = state.groups.find(g => g.slot === 0 && g.kind === PLAYER_GROUP) || null;
    const groups = state.groups.filter(g => g.kind !== PLAYER_GROUP && g.mapId === number);
    const characters = [];
    for (const group of groups) {
        if (group.roaming)
            continue;
        for (let index = group.firstCharacter; index < group.firstCharacter + group.size; ++index)
            if (state.characters[index])
                characters.push(state.characters[index]);
    }
    const jobs = [];
    for (const village of state.villages)
        if (village.mapId === number)
            for (const job of village.jobs)
                jobs.push({ village, job });
    return {
        groups,
        characters,
        start: hero !== undefined && startMap(hero) === number ? player : null,
        loot: state.loot.filter(l => l.mapId === number),
        exits: state.exits.filter(e => e.mapId === number),
        arrivals: state.exits.filter(e => e.targetMap === number),
        jobs,
    };
}

// Состояние обратно в Game.N: исходные байты, поверх — правки из state
export function serializeGameState(state) {
    if (state.loot.length > LOOT_SLOTS)
        throw new Error(`кладов ${state.loot.length}, максимум ${LOOT_SLOTS}`);
    if (state.exits.length > EXIT_SLOTS)
        throw new Error(`переходов ${state.exits.length}, максимум ${EXIT_SLOTS}`);
    const bytes = new Uint8Array(state.buffer.slice(0));
    const view = new DataView(bytes.buffer);
    const setS16 = (at, value) => view.setInt16(at, value, true);

    for (const group of state.groups) {
        const at = GROUPS_AT + group.slot * GROUP_SIZE;
        const { spawn, leash } = group;
        [spawn.top, leash.top, spawn.bottom, leash.bottom, spawn.left, leash.left, spawn.right, leash.right]
            .forEach((value, i) => setS16(at + 8 + i * 2, value));
    }
    for (const character of state.characters) {
        if (!character)
            continue;
        const at = CHARACTERS_AT + character.slot * CHARACTER_SIZE;
        bytes[at + 1] = character.direction;
        setS16(at + 8, character.y);
        setS16(at + 0x0a, character.x);
    }
    for (const village of state.villages) {
        for (const job of village.jobs) {
            const at = VILLAGES_AT + village.slot * VILLAGE_SIZE + JOBS_AT + job.job * JOB_SIZE;
            bytes[at] = job.direction;
            setS16(at + 4, job.y);
            setS16(at + 6, job.x);
        }
    }

    bytes.fill(0, LOOT_AT, EXITS_AT);
    state.loot.forEach((loot, slot) => {
        if (loot.mapId <= 0)
            throw new Error(`клад ${loot.slot}: карта ${loot.mapId} обрывает список`);
        const at = LOOT_AT + slot * LOOT_SIZE;
        bytes.set(fromHex(loot.record, LOOT_SIZE), at);
        setS16(at + 8, loot.y);
        setS16(at + 0x0a, loot.x);
        setS16(at + 0x10, loot.gold);
        setS16(at + 0x12, loot.mapId);
        bytes.set(fromHex(loot.items, LOOT_ITEMS_SIZE), at + LOOT_ITEMS_AT);
    });
    bytes.fill(0, EXITS_AT, CHARACTERS_AT);
    state.exits.forEach((exit, slot) => {
        if (exit.targetMap <= 0 || exit.targetMap > 255)
            throw new Error(`переход ${exit.slot}: целевая карта ${exit.targetMap}`);
        const at = EXITS_AT + slot * EXIT_SIZE;
        bytes.set(fromHex(exit.unknown, 2), at);
        bytes[at + 2] = exit.direction;
        view.setInt8(at + 3, exit.mapId);
        bytes[at + 4] = exit.targetMap;
        setS16(at + 5, exit.arrivalY);
        setS16(at + 7, exit.arrivalX);
        const zone = exit.zone;
        [zone.top, zone.left, zone.bottom, zone.right].forEach((value, i) => setS16(at + 9 + i * 2, value));
    });
    return bytes.buffer;
}

// Клетка мелкой сетки: шаг 58 по x, 16 по строке; точка персонажа — середина клетки
const FINE_STEP_X = TILE_STEP_X / 2;
const FINE_STEP_Y = TILE_STEP_Y / 2;
const CELL_HALF_X = FINE_STEP_X / 2;
const CELL_HALF_Y = FINE_STEP_Y / 2;

export function characterPosition(x, y) {
    return [x * FINE_STEP_X + (2 - (y & 1)) * CELL_HALF_X, y * FINE_STEP_Y + FINE_STEP_Y];
}

export function characterCell(x, y) {
    const row = Math.round((y - FINE_STEP_Y) / FINE_STEP_Y);
    return [Math.round((x - (2 - (row & 1)) * CELL_HALF_X) / FINE_STEP_X), row];
}

// Прямоугольник клеток → пиксели по краям клеток: [x, y, ширина, высота]
export function rectPixels(area) {
    const left = area.left * FINE_STEP_X;
    const top = area.top * FINE_STEP_Y + FINE_STEP_Y - CELL_HALF_Y;
    const right = area.right * FINE_STEP_X + FINE_STEP_X + CELL_HALF_X;
    const bottom = area.bottom * FINE_STEP_Y + FINE_STEP_Y + CELL_HALF_Y;
    return [left, top, right - left, bottom - top];
}

export function rectCells(x, y, width, height) {
    return rect(
        Math.round(x / FINE_STEP_X),
        Math.round((y - FINE_STEP_Y + CELL_HALF_Y) / FINE_STEP_Y),
        Math.round((x + width - FINE_STEP_X - CELL_HALF_X) / FINE_STEP_X),
        Math.round((y + height - FINE_STEP_Y - CELL_HALF_Y) / FINE_STEP_Y),
    );
}

// ---------------------------------------------------------------------------
// Анимированные спрайты: персонажи GRAPH.RES и динамические объекты OBJECTS.RES
// (docs/k1/formats/graph-res.md, objects-res.md). Картинки — по запросу.

const DIRECTIONS = 8;
const CHARACTER_MODELS = 3;
const EQUIPMENT_SETS = 24;
const CHARACTER_ANIMATIONS = 10, CHARACTER_FRAMES = 12, CHARACTER_IMAGES = 768;
const DYNAMIC_ANIMATIONS = 6, DYNAMIC_FRAMES = 18, DYNAMIC_IMAGES = 512;
const COLORS_PER_PALETTE = 256;
// Тик игры, мс: на нём меняются кадры объектов, персонажей и огней (Slavik game.h:30)
export const TICK_MS = 60;
// Палитры наборов снаряжения: в данных exe их нет — по Slavik resources.cpp:15
const EQUIPMENT_PALETTES = [2, 2, 2, 2, 4, 4, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 2, 2, 5, 5, 5, 5];

function readSprites(buffer, start, animations, frames, images, weapons) {
    const reader = new Reader(buffer, start);
    const counts = [], ids = [];
    for (let i = 0; i < animations * DIRECTIONS; ++i) {
        counts.push(reader.s16());
        const row = [];
        for (let f = 0; f < frames; ++f)
            row.push(reader.s16());
        ids.push(row);
    }
    const weapon = [];
    for (let i = 0; i < animations * DIRECTIONS; ++i) {
        const row = [];
        for (let f = 0; f < frames; ++f)
            row.push(weapons ? [reader.s16(), reader.s16()] : [0, 0]);
        weapon.push(row);
    }
    const table = [];
    for (let i = 0; i < images; ++i)
        table.push([reader.s32(), reader.s32()]);
    const sequences = [];
    for (let animation = 0; animation < animations; ++animation) {
        const row = [];
        for (let direction = 0; direction < DIRECTIONS; ++direction) {
            const index = animation * DIRECTIONS + direction;
            const sequence = [];
            for (let f = 0; f < frames; ++f) {
                const shadowOffset = [reader.s16(), reader.s16()];
                const imageOffset = [reader.s16(), reader.s16()];
                if (f < counts[index])
                    sequence.push({ imageId: ids[index][f], imageOffset, shadowOffset, weaponOffset: weapon[index][f] });
            }
            row.push(sequence);
        }
        sequences.push(row);
    }
    return { buffer, imagesStart: reader.pos, sequences, table };
}

function spriteImage(sprites, imageId, palette) {
    const offset = sprites.table[imageId][1];
    if (offset < 0)
        return null;
    return decodeRle(new Reader(sprites.buffer, sprites.imagesStart + offset), indexed(palette));
}

// Нулевое смещение тени игра тоже считает пустым
function spriteShadow(sprites, imageId) {
    const offset = sprites.table[imageId][0];
    if (offset <= 0)
        return null;
    return decodeRle(new Reader(sprites.buffer, sprites.imagesStart + offset), null);
}

// Модели людей и наборы снаряжения: блоки персонажей сразу после тайлов
function parseCharacterBlocks(buffer, start) {
    const reader = new Reader(buffer, start);
    const sprites = [];
    for (let i = 0; i < CHARACTER_MODELS + EQUIPMENT_SETS; ++i) {
        const blockStart = reader.pos;
        reader.u32();
        const size = reader.u32();
        sprites.push(readSprites(buffer, reader.pos, CHARACTER_ANIMATIONS, CHARACTER_FRAMES, CHARACTER_IMAGES, true));
        reader.pos = blockStart + size;
    }
    return { models: sprites.slice(0, CHARACTER_MODELS), equipment: sprites.slice(CHARACTER_MODELS) };
}

export function readDynamicObject(catalog, slot) {
    const offset = catalog.offsets[slot];
    if (slot >= DYNAMIC_OBJECTS || offset === undefined || offset < 0)
        return null;
    // Пропускаем метку и размер блока
    return readSprites(catalog.buffer, catalog.dataStart + offset + 8,
        DYNAMIC_ANIMATIONS, DYNAMIC_FRAMES, DYNAMIC_IMAGES, false);
}

// ---------------------------------------------------------------------------
// Таблицы exe (docs/k1/formats/executable.md): набор снаряжения у описания
// предмета и порядок слотов при отрисовке. Ищутся по строению, как в
// konung/k1/executable.py.

const EQUIPMENT_INFO_SIZE = 64, EQUIPMENT_NAME_SIZE = 44, EQUIPMENT_COUNT = 70;
const SLOTS = 6, DRAW_ORDER_ROW_SIZE = SLOTS * 4, DRAW_ORDER_ROWS = 72;

function isEquipmentTable(bytes, view, start) {
    for (let index = 0; index < EQUIPMENT_COUNT; ++index) {
        const at = start + index * EQUIPMENT_INFO_SIZE;
        if (bytes[at] < 0x20)
            return false;
        let end = at;
        while (end < at + EQUIPMENT_NAME_SIZE && bytes[end] !== 0)
            ++end;
        if (end === at + EQUIPMENT_NAME_SIZE || !isFilled(bytes, end, at + EQUIPMENT_NAME_SIZE - end, 0))
            return false;
        const spriteSet = view.getInt16(at + EQUIPMENT_INFO_SIZE - 2, true);
        if (spriteSet < -1 || spriteSet >= EQUIPMENT_SETS)
            return false;
    }
    return true;
}

function isDrawOrderRow(view, at) {
    let seen = 0;
    for (let i = 0; i < SLOTS; ++i) {
        const slot = view.getInt32(at + i * 4, true);
        if (slot < 0 || slot >= SLOTS || seen & (1 << slot))
            return false;
        seen |= 1 << slot;
    }
    return true;
}

export function parseExecutable(buffer) {
    const bytes = new Uint8Array(buffer);
    const view = new DataView(buffer);
    const found = [];
    for (let start = 0; start + EQUIPMENT_COUNT * EQUIPMENT_INFO_SIZE <= bytes.length; start += 4)
        if (isEquipmentTable(bytes, view, start))
            found.push(start);
    if (found.length !== 1)
        throw new Error(`таблица снаряжения в exe не найдена однозначно (${found.length})`);
    const equipment = [], equipmentNames = [];
    for (let index = 0; index < EQUIPMENT_COUNT; ++index) {
        const at = found[0] + index * EQUIPMENT_INFO_SIZE;
        equipment.push(view.getInt16(at + EQUIPMENT_INFO_SIZE - 2, true));
        equipmentNames.push(textAt(bytes, at, EQUIPMENT_NAME_SIZE) || "");
    }

    const runs = [];
    for (let alignment = 0; alignment < DRAW_ORDER_ROW_SIZE; alignment += 4) {
        let runStart = 0, run = 0;
        for (let at = alignment; at + DRAW_ORDER_ROW_SIZE <= bytes.length; at += DRAW_ORDER_ROW_SIZE) {
            if (isDrawOrderRow(view, at)) {
                if (run === 0)
                    runStart = at;
                ++run;
                continue;
            }
            if (run === DRAW_ORDER_ROWS)
                runs.push(runStart);
            run = 0;
        }
    }
    if (runs.length !== 1)
        throw new Error(`порядок отрисовки в exe не найден однозначно (${runs.length})`);
    // Последние 8 строк — по направлениям для людей без оружия в руках
    const first = runs[0] + (DRAW_ORDER_ROWS - DIRECTIONS) * DRAW_ORDER_ROW_SIZE;
    const drawOrder = [];
    for (let direction = 0; direction < DIRECTIONS; ++direction) {
        const row = [];
        for (let i = 0; i < SLOTS; ++i)
            row.push(view.getInt32(first + direction * DRAW_ORDER_ROW_SIZE + i * 4, true));
        drawOrder.push(row);
    }
    // Без названий рисовать можно: если они не нашлись однозначно (другое
    // издание) — names и texts null, а снаряжение и огни остаются
    let names = null, texts = null;
    try {
        names = Object.assign({ equipment: equipmentNames }, findItemNames(bytes, view));
        texts = findTexts(bytes);
    } catch (error) {
        names = texts = null;
    }
    return { equipment, drawOrder, flameAnimations: findFlameAnimations(view, bytes.length), names, texts };
}

// Тексты exe — записи фиксированной длины «текст + нули», как в
// konung/k1/executable.py: RUS в CP866, ENG латиницей (CP866 читает и её)
const NAME_SIZE = 32, MONSTER_NAMES = 30, PEOPLE_NAMES = 171, NICKNAMES = 21;
const NAMES_BLOCK = MONSTER_NAMES + PEOPLE_NAMES + NICKNAMES;
const MAP_NAME_SIZE = 40, MAP_NAMES = 56;
const ITEM_INFO_SIZE = 128, ITEM_NAME_SIZE = 120;
const ITEM_TABLES = { accessories: 9, alchemies: 14, misc: 54 };
// Классы 0x41–0x50 называются по виду из таблицы монстров, с 0x51 — по имени
const MONSTER_CLASS = 0x40, FIRST_VILLAGER_CLASS = 0x51;

// CP866 целиком, как Python-кодек cp866: 0x80–0xAF — А…п, 0xB0–0xDF — псевдографика,
// 0xE0–0xEF — р…я, 0xF0–0xFF — Ё, ё, Є, є, Ї, ї, Ў, ў, °, ∙, ·, √, №, ¤, ■ и неразрывный пробел
const CP866_BOX = "░▒▓│┤╡╢╖╕╣║╗╝╜╛┐" +
    "└┴┬├─┼╞╟╚╔╩╦╠═╬╧" +
    "╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀";
const CP866_TAIL = "ЁёЄєЇїЎў°∙·√№¤■ ";

function cp866(byte) {
    if (byte < 0x80)
        return String.fromCharCode(byte);
    if (byte < 0xb0)
        return String.fromCharCode(0x410 + byte - 0x80);
    if (byte < 0xe0)
        return CP866_BOX[byte - 0xb0];
    if (byte < 0xf0)
        return String.fromCharCode(0x440 + byte - 0xe0);
    return CP866_TAIL[byte - 0xf0];
}

// Запись «текст + нули» (непустой текст из байт ≥ 0x20, дальше только нули);
// без сборки строки — для поиска таблиц
function isText(bytes, at, size) {
    let end = at;
    while (end < at + size && bytes[end] !== 0) {
        if (bytes[end] < 0x20)
            return false;
        ++end;
    }
    return end !== at && isFilled(bytes, end, at + size - end, 0);
}

// Текст записи; null — запись не «текст + нули» (пустая тоже null)
function textAt(bytes, at, size) {
    if (!isText(bytes, at, size))
        return null;
    let text = "";
    for (let i = at; i < at + size && bytes[i] !== 0; ++i)
        text += cp866(bytes[i]);
    return text;
}

// Монстры (первые заполнены, хвост пуст), 171 имя и 21 прозвище; имя 0 и прозвище 0 пусты
function isNamesBlock(bytes, start) {
    const nicknamesStart = MONSTER_NAMES + PEOPLE_NAMES;
    // Сначала обязательно пустые записи: так отсекается почти всё сразу
    if (!isFilled(bytes, start + MONSTER_NAMES * NAME_SIZE, NAME_SIZE, 0) ||
        !isFilled(bytes, start + nicknamesStart * NAME_SIZE, NAME_SIZE, 0))
        return false;
    let seenEmpty = false;
    for (let index = 0; index < NAMES_BLOCK; ++index) {
        const at = start + index * NAME_SIZE;
        const empty = isFilled(bytes, at, NAME_SIZE, 0);
        if (index === MONSTER_NAMES || index === nicknamesStart)
            continue;
        if (index < MONSTER_NAMES && index && empty) {
            seenEmpty = true;
            continue;
        }
        if (index < MONSTER_NAMES && seenEmpty)
            return false;
        if (!isText(bytes, at, NAME_SIZE))
            return false;
    }
    return true;
}

// Серии подряд идущих записей size байт, для которых isRecord верно
function recordRuns(bytes, size, isRecord) {
    const runs = [];
    for (let alignment = 0; alignment < size; alignment += 4) {
        let runStart = 0, run = 0;
        for (let at = alignment; at + size <= bytes.length; at += size) {
            if (isRecord(at)) {
                if (run === 0)
                    runStart = at;
                ++run;
                continue;
            }
            if (run)
                runs.push({ start: runStart, count: run });
            run = 0;
        }
        if (run)
            runs.push({ start: runStart, count: run });
    }
    return runs;
}

function onlyRun(runs, count, what) {
    const found = runs.filter(run => run.count === count);
    if (found.length !== 1)
        throw new Error(`${what} в exe не найдены однозначно (${found.length})`);
    return found[0].start;
}

// count записей по step байт; текст — первые size байт записи
function readTexts(bytes, start, step, count, size = step) {
    const texts = [];
    for (let i = 0; i < count; ++i)
        texts.push(textAt(bytes, start + i * step, size) || "");
    return texts;
}

function findTexts(bytes) {
    const found = [];
    for (let start = 0; start + NAMES_BLOCK * NAME_SIZE <= bytes.length; start += 4)
        if (isText(bytes, start, NAME_SIZE) && isNamesBlock(bytes, start))
            found.push(start);
    if (found.length !== 1)
        throw new Error(`имена в exe не найдены однозначно (${found.length})`);
    const all = readTexts(bytes, found[0], NAME_SIZE, NAMES_BLOCK);
    const maps = onlyRun(recordRuns(bytes, MAP_NAME_SIZE, at => isText(bytes, at, MAP_NAME_SIZE)),
        MAP_NAMES, "названия карт");
    return {
        monsters: all.slice(0, MONSTER_NAMES),
        names: all.slice(MONSTER_NAMES, MONSTER_NAMES + PEOPLE_NAMES),
        nicknames: all.slice(MONSTER_NAMES + PEOPLE_NAMES),
        maps: readTexts(bytes, maps, MAP_NAME_SIZE, MAP_NAMES),
    };
}

// Украшения, зелья, прочее: 120 байт названия и 4 × s16, у всех обе картинки > 0
function findItemNames(bytes, view) {
    const runs = recordRuns(bytes, ITEM_INFO_SIZE, at => isText(bytes, at, ITEM_NAME_SIZE) &&
        view.getInt16(at + ITEM_NAME_SIZE + 4, true) > 0 && view.getInt16(at + ITEM_NAME_SIZE + 6, true) > 0);
    const names = {};
    for (const [table, count] of Object.entries(ITEM_TABLES))
        names[table] = readTexts(bytes, onlyRun(runs, count, `описания (${table})`), ITEM_INFO_SIZE, count,
            ITEM_NAME_SIZE);
    return names;
}

// Название предмета по виду и номеру описания; null — у вида нет таблицы
export function itemName(tables, typeId, infoId) {
    const names = tables.names;
    if (!names)
        return null;
    let table = null;
    if (typeId >= 0 && typeId <= 5 || typeId === 12)
        table = names.equipment;
    else if (typeId >= 6 && typeId <= 8)
        table = names.accessories;
    else if (typeId === 9)
        table = names.alchemies;
    else if (typeId === 11)
        table = names.misc;
    return table && infoId >= 0 && infoId < table.length ? table[infoId] : null;
}

// Имя персонажа, как его собирает игра (konung/k1/executable.character_name)
export function characterName(texts, classId, nameId, nickId) {
    if (!texts)
        return "";
    const kind = classId & 0x7f;
    if (classId & MONSTER_CLASS && kind < FIRST_VILLAGER_CLASS)
        return texts.monsters[kind - MONSTER_CLASS - 1] || "";
    const name = texts.names[nameId] || "";
    return nickId ? `${name} ${texts.nicknames[nickId] || ""}` : name;
}

// Названия предметов клада: 22 номера s16 в шестнадцатеричном виде, 0 — пусто
export function lootItemNames(state, tables, itemsHex) {
    const bytes = fromHex(itemsHex, LOOT_ITEMS_SIZE);
    const view = new DataView(bytes.buffer);
    const names = [];
    for (let i = 0; i < LOOT_ITEMS_SIZE / 2; ++i) {
        const slot = view.getInt16(i * 2, true);
        if (slot > 0 && slot < ITEM_SLOTS) {
            const type = state.itemType(slot);
            const name = type === null ? null : itemName(tables, type, state.itemInfo(slot));
            names.push(name || `#${slot}`);
        }
    }
    return names;
}

function sequenceFrame(sprites, animation, direction, frame) {
    const sequence = sprites.sequences[animation][direction];
    if (!sequence.length)
        return null;
    return frame < sequence.length ? sequence[frame] : sequence[0];
}

// Спрайт персонажа и номер анимации его состояния
function characterSprites(character, graph, dynamicSprites) {
    const sprites = character.dynamic
        ? dynamicSprites(character.spriteSet)
        : graph.characters.models[character.spriteSet];
    const animation = character.dynamic ? character.state % DYNAMIC_ANIMATIONS : character.state;
    return { sprites, animation };
}

// Кадр персонажа одной картинкой (как konung/k1/characters.compose):
// image — модель и снаряжение, offset — левый верхний угол от точки персонажа,
// shadow — маска тени со своим смещением. dynamicSprites(slot) — динамический объект.
// frameIndex — номер кадра анимации, по умолчанию — из записи персонажа
export function composeCharacter(character, state, graph, dynamicSprites, tables, frameIndex = character.frame) {
    const { sprites, animation } = characterSprites(character, graph, dynamicSprites);
    if (!sprites)
        return null;
    const frame = sequenceFrame(sprites, animation, character.direction, frameIndex);
    if (!frame)
        return null;
    const body = spriteImage(sprites, frame.imageId, graph.palette(character.paletteOffset));
    if (!body)
        return null;
    const layers = [{ image: body, x: frame.imageOffset[0], y: frame.imageOffset[1] }];
    if (!character.dynamic) {
        for (const slot of tables.drawOrder[character.direction]) {
            const item = character.equipment[slot];
            const info = item > 0 ? state.itemInfo(item) : null;
            if (info === null || info < 0 || info >= tables.equipment.length)
                continue;
            const set = tables.equipment[info];
            if (set < 0)
                continue;
            const gear = graph.characters.equipment[set];
            const gearFrame = sequenceFrame(gear, animation, character.direction, frameIndex);
            const image = gearFrame && spriteImage(gear, gearFrame.imageId,
                graph.palette(EQUIPMENT_PALETTES[set] * COLORS_PER_PALETTE));
            if (image)
                layers.push({
                    image,
                    x: frame.weaponOffset[0] + gearFrame.imageOffset[0],
                    y: frame.weaponOffset[1] + gearFrame.imageOffset[1],
                });
        }
    }

    const left = Math.min(...layers.map(l => l.x)), top = Math.min(...layers.map(l => l.y));
    const right = Math.max(...layers.map(l => l.x + l.image.width));
    const bottom = Math.max(...layers.map(l => l.y + l.image.height));
    const width = right - left, height = bottom - top;
    const pixels = new Uint32Array(width * height);
    for (const layer of layers) {
        const { image } = layer;
        for (let y = 0; y < image.height; ++y) {
            const row = (layer.y - top + y) * width + (layer.x - left);
            for (let x = 0; x < image.width; ++x) {
                const color = image.pixels[y * image.width + x];
                if (color >>> 24)
                    pixels[row + x] = color;
            }
        }
    }
    const shadowImage = spriteShadow(sprites, frame.imageId);
    return {
        image: { width, height, pixels },
        offset: [left, top],
        shadow: shadowImage && { image: shadowImage, offset: frame.shadowOffset },
    };
}

// ---------------------------------------------------------------------------
// Анимации для Tiled: кадры тайла Tiled рисует от одной точки, поэтому кадры
// с разными смещениями кладутся в общую рамку (объединение прямоугольников).

// frames — [{ image, offset } | null]; null — кадра нет (пустая картинка рамки)
export function alignFrames(frames) {
    const present = frames.filter(Boolean);
    if (!present.length)
        return null;
    const left = Math.min(...present.map(f => f.offset[0]));
    const top = Math.min(...present.map(f => f.offset[1]));
    const right = Math.max(...present.map(f => f.offset[0] + f.image.width));
    const bottom = Math.max(...present.map(f => f.offset[1] + f.image.height));
    const width = right - left, height = bottom - top;
    const images = frames.map(frame => {
        const pixels = new Uint32Array(width * height);
        if (frame) {
            const { image } = frame;
            for (let y = 0; y < image.height; ++y)
                pixels.set(image.pixels.subarray(y * image.width, (y + 1) * image.width),
                    (frame.offset[1] - top + y) * width + (frame.offset[0] - left));
        }
        return { width, height, pixels };
    });
    return { offset: [left, top], images };
}

// Все кадры анимации персонажа в общей рамке (как characters.animation).
// bottom — нижний край первого кадра от точки персонажа: по нему сортирует игра
export function composeAnimation(character, state, graph, dynamicSprites, tables) {
    const { sprites, animation } = characterSprites(character, graph, dynamicSprites);
    if (!sprites)
        return null;
    const count = sprites.sequences[animation][character.direction].length;
    const frames = [];
    for (let index = 0; index < count; ++index) {
        const frame = composeCharacter(character, state, graph, dynamicSprites, tables, index);
        if (frame)
            frames.push(frame);
    }
    if (!frames.length)
        return null;
    return {
        image: alignFrames(frames),
        shadow: alignFrames(frames.map(f => f.shadow)),
        bottom: frames[0].offset[1] + frames[0].image.height,
        duration: TICK_MS,
    };
}

// Кадры анимированного простого объекта в общей рамке; null — объект не анимирован.
// Кадр меняется раз в frameTime тиков (Slavik game_play.cpp:1003-1017)
export function objectAnimation(simple) {
    if (simple.frameCount < 2 || simple.frameTime < 1)
        return null;
    const frames = simple.frames.slice(0, simple.frameCount);
    return {
        image: alignFrames(frames.map(f => f.image && { image: f.image, offset: f.offset })),
        shadow: alignFrames(frames.map(f => f.shadow && { image: f.shadow, offset: f.shadowOffset })),
        duration: simple.frameTime * TICK_MS,
    };
}

// ---------------------------------------------------------------------------
// Огни (docs/k1/formats/flames-res.md), эталон — konung/k1/flames_res.py.

export const FLAME_KINDS = 7;
export const FLAME_FRAMES = 74;

// Анимации огней в exe: 7 пар (первый, последний кадр) по 2 × s32, подряд
// и без пропусков от кадра 0 до последнего; оба конца включены
function findFlameAnimations(view, length) {
    const found = [];
    for (let start = 0; start + FLAME_KINDS * 8 <= length; start += 4) {
        if (view.getInt32(start, true) !== 0)
            continue;
        const pairs = [];
        for (let kind = 0; kind < FLAME_KINDS; ++kind)
            pairs.push([view.getInt32(start + kind * 8, true), view.getInt32(start + kind * 8 + 4, true)]);
        const contiguous = pairs.every(([first, last], kind) =>
            first <= last && (kind === 0 || first === pairs[kind - 1][1] + 1));
        if (contiguous && pairs[FLAME_KINDS - 1][1] === FLAME_FRAMES - 1)
            found.push(pairs);
    }
    if (found.length !== 1)
        throw new Error(`таблица огней в exe не найдена однозначно (${found.length})`);
    return found[0];
}

// Огонь светится: альфа — из яркости, a = (2r + g + b) / 4; цвет остаётся как есть
function flamePixel(reader) {
    const color = reader.u16();
    const r5 = (color >> 10) & 0x1f, g5 = (color >> 5) & 0x1f, b5 = color & 0x1f;
    const r = (r5 << 3) | (r5 >> 2), g = (g5 << 3) | (g5 >> 2), b = (b5 << 3) | (b5 >> 2);
    const a = (2 * r + g + b) >> 2;
    return ((a << 24) | (r << 16) | (g << 8) | b) >>> 0;
}

// 74 блока: u32 метка, s32 размер, картинка RL16
export function parseFlames(buffer) {
    const reader = new Reader(buffer);
    const frames = [];
    while (reader.pos < buffer.byteLength) {
        reader.u32();
        const size = reader.s32();
        const start = reader.pos;
        frames.push(decodeRle(reader, flamePixel));
        reader.pos = start + size;
    }
    return frames;
}
