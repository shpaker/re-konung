//! LIGHTS.RES — маски света (docs/k1/formats/lights-res.md, эталон — `konung/k1/lights_res.py`).
//!
//! Заголовка нет: маска 64 × 64, маска 64 × 43 и 19 масок тайлов 114 × 64, байт на
//! пиксель — яркость. Номер маски тайла хранит первый блок LIGHTS.N ([crate::map::Lights]).

use thiserror::Error;

pub type Result<T> = std::result::Result<T, LightsError>;

#[derive(Debug, Error)]
pub enum LightsError {
    #[error("LIGHTS.RES {actual} байт, ожидалось {SIZE}")]
    WrongSize { actual: usize },
}

pub const TILE_MASKS: usize = 19;
pub const TILE_MASK_WIDTH: usize = 114;
pub const TILE_MASK_HEIGHT: usize = 64;
// Две маски в начале файла: Slavik их читает, но не использует, назначение неизвестно
pub const FIRST_SIZE: (usize, usize) = (64, 64);
pub const SECOND_SIZE: (usize, usize) = (64, 43);
pub const SIZE: usize = FIRST_SIZE.0 * FIRST_SIZE.1
    + SECOND_SIZE.0 * SECOND_SIZE.1
    + TILE_MASKS * TILE_MASK_WIDTH * TILE_MASK_HEIGHT;
/// Slavik переводит яркость в 0–255 как 255 / 100 · v; в файле она не больше 43
pub const BRIGHTNESS_SCALE: u8 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightsRes {
    pub first: Vec<u8>,
    pub second: Vec<u8>,
    /// Маски тайлов: в LIGHTS.N маска i записана как i + 1, 0 — света нет
    pub tiles: Vec<Vec<u8>>,
}

pub fn parse(data: &[u8]) -> Result<LightsRes> {
    if data.len() != SIZE {
        return Err(LightsError::WrongSize { actual: data.len() });
    }
    let second_at = FIRST_SIZE.0 * FIRST_SIZE.1;
    let tiles_at = second_at + SECOND_SIZE.0 * SECOND_SIZE.1;
    let mask = TILE_MASK_WIDTH * TILE_MASK_HEIGHT;
    let tiles = (0..TILE_MASKS)
        .map(|i| data[tiles_at + i * mask..tiles_at + (i + 1) * mask].to_vec())
        .collect();
    Ok(LightsRes {
        first: data[..second_at].to_vec(),
        second: data[second_at..tiles_at].to_vec(),
        tiles,
    })
}

/// Пиксель маски внутри ромба тайла 114 × 64.
pub fn in_diamond(x: usize, y: usize) -> bool {
    let half_x = TILE_MASK_WIDTH as f64 / 2.0;
    let half_y = TILE_MASK_HEIGHT as f64 / 2.0;
    (x as f64 + 0.5 - half_x).abs() / half_x + (y as f64 + 0.5 - half_y).abs() / half_y <= 1.0
}

/// Маска тайла цветом color в RGBA-байты, прозрачность — яркость, как её растягивает Slavik.
///
/// Маска заполняет весь прямоугольник, а игра светит ею только пиксели своего
/// тайла (Slavik `game.cpp:996-1003`), поэтому вне ромба она прозрачна.
pub fn to_rgba(mask: &[u8], color: (u8, u8, u8)) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(TILE_MASK_WIDTH * TILE_MASK_HEIGHT * 4);
    for (i, &value) in mask.iter().enumerate() {
        let alpha = if in_diamond(i % TILE_MASK_WIDTH, i / TILE_MASK_WIDTH) {
            u16::from(value) * 255 / u16::from(BRIGHTNESS_SCALE)
        } else {
            0
        };
        rgba.extend_from_slice(&[color.0, color.1, color.2, alpha as u8]);
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_diamond_covers_center_and_skips_corners() {
        assert!(in_diamond(TILE_MASK_WIDTH / 2, TILE_MASK_HEIGHT / 2));
        assert!(!in_diamond(0, 0));
        assert!(!in_diamond(TILE_MASK_WIDTH - 1, 0));
        assert!(!in_diamond(0, TILE_MASK_HEIGHT - 1));
        assert!(!in_diamond(TILE_MASK_WIDTH - 1, TILE_MASK_HEIGHT - 1));
    }

    #[test]
    fn to_rgba_scales_brightness_inside_diamond_only() {
        let mask = vec![43u8; TILE_MASK_WIDTH * TILE_MASK_HEIGHT];
        let rgba = to_rgba(&mask, (255, 210, 120));
        assert_eq!(rgba.len(), TILE_MASK_WIDTH * TILE_MASK_HEIGHT * 4);
        // Угол вне ромба — прозрачный
        assert_eq!(&rgba[0..4], &[255, 210, 120, 0]);
        // Центр: 43 * 255 / 100 = 109
        let center = (TILE_MASK_HEIGHT / 2 * TILE_MASK_WIDTH + TILE_MASK_WIDTH / 2) * 4;
        assert_eq!(&rgba[center..center + 4], &[255, 210, 120, 109]);
    }

    #[test]
    fn wrong_size_is_an_error() {
        assert!(matches!(
            parse(&[0u8; SIZE - 1]),
            Err(LightsError::WrongSize { .. })
        ));
        let parsed = parse(&[7u8; SIZE]).unwrap();
        assert_eq!(parsed.tiles.len(), TILE_MASKS);
        assert_eq!(parsed.first.len(), FIRST_SIZE.0 * FIRST_SIZE.1);
        assert_eq!(parsed.second.len(), SECOND_SIZE.0 * SECOND_SIZE.1);
    }
}
