//! FLAMES.RES — кадры огней: факелы, костры, горны
//! (docs/k1/formats/flames-res.md, эталон — `konung/k1/flames_res.py`).
//!
//! 74 блока подряд: u32 метка, s32 размер, картинка RL16. Какие кадры образуют
//! анимацию огня какого вида, задаёт таблица в exe ([crate::executable]).

use thiserror::Error;

use crate::palette::rgb555;
use crate::reader::{ReadError, Reader};
use crate::rle::{self, RgbImage, RleError};

pub type Result<T> = std::result::Result<T, FlamesError>;

#[derive(Debug, Error)]
pub enum FlamesError {
    #[error("огонь {index}: картинка {actual} байт, в блоке {expected}")]
    WrongBlockSize {
        index: usize,
        actual: usize,
        expected: usize,
    },
    #[error(transparent)]
    Rle(#[from] RleError),
    #[error(transparent)]
    Read(#[from] ReadError),
}

#[derive(Debug, Clone)]
pub struct FlamesRes {
    pub frames: Vec<RgbImage>,
    /// Метки блоков: у всех 0x05000000, смысл неизвестен
    pub tags: Vec<u32>,
}

pub fn parse(data: &[u8]) -> Result<FlamesRes> {
    let mut reader = Reader::new(data);
    let mut frames = Vec::new();
    let mut tags = Vec::new();
    while reader.pos() < data.len() {
        tags.push(reader.u32()?);
        let size = reader.s32()? as usize;
        let start = reader.pos();
        frames.push(rle::decode_rgb555(&mut reader)?);
        if reader.pos() != start + size {
            return Err(FlamesError::WrongBlockSize {
                index: frames.len() - 1,
                actual: reader.pos() - start,
                expected: size,
            });
        }
    }
    Ok(FlamesRes { frames, tags })
}

/// Огонь светится: прозрачность — из яркости, a = (2r + g + b) / 4 (Slavik resources.cpp:632).
///
/// Игра ещё умножает цвет на a; здесь цвет остаётся как есть, а a идёт в альфу —
/// так огонь смешивается обычной прозрачностью (гипотеза, в игре не сверено).
pub fn to_rgba(frame: &RgbImage) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(frame.width * frame.height * 4);
    for (&color, &alpha) in frame.colors.iter().zip(&frame.alpha) {
        if alpha == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let (r, g, b) = rgb555(color);
        let a = (2 * u16::from(r) + u16::from(g) + u16::from(b)) / 4;
        rgba.extend_from_slice(&[r, g, b, a as u8]);
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_rgba_takes_alpha_from_brightness() {
        let frame = RgbImage {
            width: 2,
            height: 1,
            // красный RGB555 и пустой пиксель
            colors: [31u16 << 10, 0].to_vec(),
            alpha: [255, 0].to_vec(),
        };
        // a = (2·255 + 0 + 0) / 4 = 127
        assert_eq!(to_rgba(&frame), [255, 0, 0, 127, 0, 0, 0, 0]);
    }
}
