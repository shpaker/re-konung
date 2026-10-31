//! Ключи текстур и их декодирование в RGBA — чистые функции без Bevy.
//!
//! Ключ однозначно определяет картинку, поэтому потребитель (рендер) кэширует
//! декодирование по ключу. Цвета и формы оверлеев — из эталона
//! `konung/k1/render.py`; текстовые подписи маркеров не рисуются (в модели
//! сцены нет текста — его показывает UI просмотрщика).

use k1_data::{
    characters::LayerSource,
    flames_res::{self, FlamesRes},
    graph_res::GraphRes,
    lights_res::{self, LightsRes},
    objects_res::ObjectsRes,
    rle::{self, IndexedImage},
};

/// Тени — чёрная маска с прозрачностью 0.4 (как слой shadows в Tiled)
pub const SHADOW_ALPHA: u8 = 102;
pub const LIGHT_COLOR: (u8, u8, u8) = (255, 210, 120);
pub const FOOT_BLOCKED_COLOR: [u8; 4] = [255, 40, 40, 110];
pub const FOOT_ARROWS_COLOR: [u8; 4] = [40, 120, 255, 110];
pub const MARKER_RADIUS: i32 = 8;

/// Цвета маркеров Game.N — как MARKER_COLORS в `render.py`.
pub mod marker_colors {
    pub const SPAWN: [u8; 4] = [255, 64, 64, 255];
    pub const LEASH: [u8; 4] = [255, 160, 0, 160];
    pub const START: [u8; 4] = [0, 255, 0, 255];
    pub const LOOT: [u8; 4] = [255, 215, 0, 255];
    pub const EXIT: [u8; 4] = [64, 160, 255, 255];
    pub const ARRIVAL: [u8; 4] = [0, 255, 255, 255];
    pub const JOB: [u8; 4] = [255, 0, 255, 255];
}

/// Ромб проходимости: вдвое шире клетки мелкой сетки по x и вдвое выше по y —
/// половины шагов из `render.draw_foot` (FINE_STEP_X // 2, FINE_STEP_Y).
pub const FOOT_DIAMOND: (i32, i32) = (
    k1_data::game_state::FINE_STEP_X,
    2 * k1_data::game_state::FINE_STEP_Y,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureKey {
    Tile {
        index: usize,
    },
    ObjectFrame {
        object_id: usize,
        frame: usize,
    },
    ObjectShadow {
        object_id: usize,
        frame: usize,
    },
    FlameFrame {
        index: usize,
    },
    /// Картинка спрайта персонажа; palette — номер цвета в банке GRAPH.RES
    SpriteImage {
        source: LayerSource,
        image_id: u16,
        palette: u32,
    },
    SpriteShadow {
        source: LayerSource,
        image_id: u16,
    },
    LightMask {
        index: usize,
    },
    FootDiamond {
        color: [u8; 4],
    },
    /// Кольцо маркера Game.N (радиус [MARKER_RADIUS], толщина 3 — приближение PIL)
    MarkerRing {
        color: [u8; 4],
    },
    /// Однотонный пиксель 1×1: прямоугольники маркеров растягиваются из него
    Solid {
        color: [u8; 4],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgba {
    fn from_indexed(image: &IndexedImage, palette: &[(u8, u8, u8)]) -> Self {
        Self {
            width: image.width as u32,
            height: image.height as u32,
            data: rle::indexed_to_rgba(image, palette),
        }
    }

    fn from_shadow(image: &IndexedImage) -> Self {
        let mut data = Vec::with_capacity(image.width * image.height * 4);
        for &alpha in &image.alpha {
            let alpha = if alpha != 0 { SHADOW_ALPHA } else { 0 };
            data.extend_from_slice(&[0, 0, 0, alpha]);
        }
        Self {
            width: image.width as u32,
            height: image.height as u32,
            data,
        }
    }

    fn raster(width: i32, height: i32, pixel: impl Fn(i32, i32) -> [u8; 4]) -> Self {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&pixel(x, y));
            }
        }
        Self {
            width: width as u32,
            height: height as u32,
            data,
        }
    }
}

fn sprites_for<'a>(
    source: LayerSource,
    graph: &'a GraphRes<'a>,
    objects: &'a ObjectsRes<'a>,
) -> Option<&'a k1_data::graph_res::Sprites<'a>> {
    match source {
        LayerSource::Model(index) => graph.models.get(index),
        LayerSource::Equipment(index) => graph.equipment.get(index),
        LayerSource::Dynamic(index) => objects.dynamic.get(index).and_then(Option::as_ref),
    }
}

/// Картинка ключа; None — данных нет (пустой слот, выход за границы).
pub fn decode(
    key: &TextureKey,
    graph: &GraphRes<'_>,
    objects: &ObjectsRes<'_>,
    lights: &LightsRes,
    flames: &FlamesRes,
) -> Option<Rgba> {
    if let Some(rgba) = decode_overlay(key) {
        return Some(rgba);
    }
    match *key {
        TextureKey::Tile { index } => {
            let tile = graph.tile(index)?;
            Some(Rgba::from_indexed(
                &tile.image,
                &graph.palette(tile.palette_offset),
            ))
        }
        TextureKey::ObjectFrame { object_id, frame } => {
            let simple = objects.simple.get(object_id)?.as_ref()?;
            let image = simple.frames.get(frame)?.image.as_ref()?;
            Some(Rgba::from_indexed(
                image,
                &graph.palette(simple.palette_offset),
            ))
        }
        TextureKey::ObjectShadow { object_id, frame } => {
            let simple = objects.simple.get(object_id)?.as_ref()?;
            Some(Rgba::from_shadow(
                simple.frames.get(frame)?.shadow.as_ref()?,
            ))
        }
        TextureKey::FlameFrame { index } => {
            let frame = flames.frames.get(index)?;
            Some(Rgba {
                width: frame.width as u32,
                height: frame.height as u32,
                data: flames_res::to_rgba(frame),
            })
        }
        TextureKey::SpriteImage {
            source,
            image_id,
            palette,
        } => {
            let sprites = sprites_for(source, graph, objects)?;
            let image = sprites.image(image_id as i16).ok()??;
            Some(Rgba::from_indexed(&image, &graph.palette(palette as usize)))
        }
        TextureKey::SpriteShadow { source, image_id } => {
            let sprites = sprites_for(source, graph, objects)?;
            Some(Rgba::from_shadow(&sprites.shadow(image_id as i16).ok()??))
        }
        TextureKey::LightMask { index } => {
            let mask = lights.tiles.get(index)?;
            Some(Rgba {
                width: lights_res::TILE_MASK_WIDTH as u32,
                height: lights_res::TILE_MASK_HEIGHT as u32,
                data: lights_res::to_rgba(mask, LIGHT_COLOR),
            })
        }
        TextureKey::FootDiamond { .. }
        | TextureKey::MarkerRing { .. }
        | TextureKey::Solid { .. } => unreachable!("оверлеи декодируются выше"),
    }
}

/// Ключи-оверлеи рисуются чистой растеризацией, ресурсы игры не нужны.
fn decode_overlay(key: &TextureKey) -> Option<Rgba> {
    match *key {
        TextureKey::FootDiamond { color } => {
            let (width, height) = FOOT_DIAMOND;
            let (half_x, half_y) = (width as f64 / 2.0, height as f64 / 2.0);
            Some(Rgba::raster(width, height, |x, y| {
                let inside = (x as f64 + 0.5 - half_x).abs() / half_x
                    + (y as f64 + 0.5 - half_y).abs() / half_y
                    <= 1.0;
                if inside { color } else { [0, 0, 0, 0] }
            }))
        }
        TextureKey::MarkerRing { color } => {
            // Кольцо вместо PIL ellipse(width=3): внешний радиус 8.5, внутренний 5.5
            let size = 2 * MARKER_RADIUS + 1;
            let center = MARKER_RADIUS as f64 + 0.5;
            Some(Rgba::raster(size, size, |x, y| {
                let d =
                    ((x as f64 + 0.5 - center).powi(2) + (y as f64 + 0.5 - center).powi(2)).sqrt();
                if (5.5..=8.5).contains(&d) {
                    color
                } else {
                    [0, 0, 0, 0]
                }
            }))
        }
        TextureKey::Solid { color } => Some(Rgba::raster(1, 1, |_, _| color)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_is_one_pixel() {
        let rgba = decode_overlay(&TextureKey::Solid {
            color: [1, 2, 3, 4],
        })
        .unwrap();
        assert_eq!(
            (rgba.width, rgba.height, rgba.data.as_slice()),
            (1, 1, &[1u8, 2, 3, 4][..])
        );
    }

    #[test]
    fn foot_diamond_covers_center_only() {
        let rgba = decode_overlay(&TextureKey::FootDiamond {
            color: FOOT_BLOCKED_COLOR,
        })
        .unwrap();
        let (w, h) = (rgba.width as usize, rgba.height as usize);
        assert_eq!((w, h), (58, 32));
        let alpha_at = |x: usize, y: usize| rgba.data[(y * w + x) * 4 + 3];
        assert_eq!(alpha_at(w / 2, h / 2), FOOT_BLOCKED_COLOR[3]);
        assert_eq!(alpha_at(0, 0), 0);
        assert_eq!(alpha_at(w - 1, h - 1), 0);
    }

    #[test]
    fn marker_ring_is_hollow() {
        let rgba = decode_overlay(&TextureKey::MarkerRing {
            color: marker_colors::START,
        })
        .unwrap();
        let size = (2 * MARKER_RADIUS + 1) as usize;
        assert_eq!((rgba.width, rgba.height), (size as u32, size as u32));
        let alpha_at = |x: usize, y: usize| rgba.data[(y * size + x) * 4 + 3];
        // Центр пуст, крайнее кольцо на месте
        assert_eq!(alpha_at(MARKER_RADIUS as usize, MARKER_RADIUS as usize), 0);
        assert_eq!(alpha_at(size - 1, MARKER_RADIUS as usize), 255);
    }
}
