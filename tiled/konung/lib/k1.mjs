// Разбор ресурсов «Князя» (1999) без Tiled API: на входе ArrayBuffer, на выходе
// простые объекты. Повторяет эталон konung/k1 на Python; при расхождении прав
// Python — у него тесты. Форматы описаны в docs/k1/formats/.

export const MAP_WIDTH = 80;
export const MAP_HEIGHT = 160;
const FINE_WIDTH = 160;
const FINE_HEIGHT = 320;
const MAX_DECORATIONS = 1000;
const MAX_OBJECTS = 1000;
const OBJECT_SIZE = 36;
export const MAP_SIZE =
    MAP_WIDTH * MAP_HEIGHT * 2 + FINE_WIDTH * FINE_HEIGHT + MAX_DECORATIONS * 16 + MAX_OBJECTS * OBJECT_SIZE;

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
    reader.u32();
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
    return { palette, tiles };
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
    reader.pos += FLAME_SLOTS * 5;

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
    return { objectId, flags, flameCount, frameCount, frameTime, frames };
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
    reader.pos += FINE_WIDTH * FINE_HEIGHT;

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
    return { ground, overlay, decorations, objects };
}

// Левый верхний угол тайла клетки
export function tilePosition(column, row) {
    return [column * TILE_STEP_X + (row & 1) * (TILE_STEP_X / 2), row * TILE_STEP_Y];
}

// Позиция объекта: клетка мелкой сетки (вдвое мельче тайловой) → пиксели
export function objectPosition(x, y) {
    return [x * (TILE_STEP_X / 2) - (y & 1) * (TILE_STEP_X / 4), y * (TILE_STEP_Y / 2)];
}
