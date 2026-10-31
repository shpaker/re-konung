//! Построчное RLE-сжатие спрайтов (docs/k1/formats/rle.md, эталон — `konung/rle.py`).
//!
//! Заголовок: ширина s16, высота s16, длины строк s16 × высота.
//! Строка — последовательность команд, байт n:
//! - 0 — конец строки;
//! - n & 0x80 — пропустить n & 0x7F прозрачных пикселей;
//! - иначе — n пикселей следом (байт индекса палитры или u16 RGB555 — зависит от вида).

use thiserror::Error;

use crate::palette::rgb555;
use crate::reader::{ReadError, Reader};

pub type Result<T> = std::result::Result<T, RleError>;

#[derive(Debug, Error)]
pub enum RleError {
    #[error("размер {width}×{height} по смещению {pos}")]
    NegativeSize { width: i16, height: i16, pos: usize },
    #[error("строка {row}: {x}+{count} пикселей больше ширины {width}")]
    RowOverflow {
        row: usize,
        x: usize,
        count: usize,
        width: usize,
    },
    #[error(transparent)]
    Read(#[from] ReadError),
}

/// Картинка с индексами палитры; alpha — 0 (прозрачно) или 255 на пиксель.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: usize,
    pub height: usize,
    pub indices: Vec<u8>,
    pub alpha: Vec<u8>,
}

/// Картинка с цветами RGB555 на пиксель; alpha — 0 или 255.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbImage {
    pub width: usize,
    pub height: usize,
    pub colors: Vec<u16>,
    pub alpha: Vec<u8>,
}

fn header(reader: &mut Reader<'_>) -> Result<(usize, usize)> {
    let header_pos = reader.pos();
    let width = reader.s16()?;
    let height = reader.s16()?;
    if width < 0 || height < 0 {
        return Err(RleError::NegativeSize {
            width,
            height,
            pos: header_pos,
        });
    }
    let (width, height) = (width as usize, height as usize);
    reader.skip(height * 2); // длины строк
    Ok((width, height))
}

/// Команда строки с пикселями: (y, x, count, смещение данных).
fn rows(
    reader: &mut Reader<'_>,
    width: usize,
    height: usize,
    pixel_size: usize,
) -> Result<Vec<(usize, usize, usize, usize)>> {
    let mut commands = Vec::new();
    for y in 0..height {
        let mut x = 0usize;
        loop {
            let n = reader.u8()? as usize;
            if n == 0 {
                break;
            }
            if n & 0x80 != 0 {
                x += n & 0x7F;
                continue;
            }
            if x + n > width {
                return Err(RleError::RowOverflow {
                    row: y,
                    x,
                    count: n,
                    width,
                });
            }
            commands.push((y, x, n, reader.pos()));
            reader.skip(n * pixel_size);
            x += n;
        }
    }
    Ok(commands)
}

/// RL8: пиксели — индексы палитры. Reader остаётся в конце картинки.
pub fn decode_indexed(reader: &mut Reader<'_>) -> Result<IndexedImage> {
    let (width, height) = header(reader)?;
    let mut image = IndexedImage {
        width,
        height,
        indices: vec![0; width * height],
        alpha: vec![0; width * height],
    };
    let commands = rows(reader, width, height, 1)?;
    let end = reader.pos();
    for (y, x, count, pos) in commands {
        let start = y * width + x;
        reader.seek(pos);
        let pixels = reader.bytes(count)?;
        image.indices[start..start + count].copy_from_slice(pixels);
        image.alpha[start..start + count].fill(255);
    }
    reader.seek(end);
    Ok(image)
}

/// RL16: пиксели — цвета RGB555 (курсоры, огни). Reader остаётся в конце картинки.
pub fn decode_rgb555(reader: &mut Reader<'_>) -> Result<RgbImage> {
    let (width, height) = header(reader)?;
    let mut image = RgbImage {
        width,
        height,
        colors: vec![0; width * height],
        alpha: vec![0; width * height],
    };
    let commands = rows(reader, width, height, 2)?;
    let end = reader.pos();
    for (y, x, count, pos) in commands {
        let start = y * width + x;
        reader.seek(pos);
        let pixels = reader.bytes(count * 2)?;
        for (i, chunk) in pixels.as_chunks::<2>().0.iter().enumerate() {
            image.colors[start + i] = u16::from_le_bytes(*chunk);
        }
        image.alpha[start..start + count].fill(255);
    }
    reader.seek(end);
    Ok(image)
}

/// Тень: пиксели без данных, важна только маска.
pub fn decode_shadow(reader: &mut Reader<'_>) -> Result<IndexedImage> {
    let (width, height) = header(reader)?;
    let mut image = IndexedImage {
        width,
        height,
        indices: vec![0; width * height],
        alpha: vec![0; width * height],
    };
    for (y, x, count, _pos) in rows(reader, width, height, 0)? {
        let start = y * width + x;
        image.alpha[start..start + count].fill(255);
    }
    Ok(image)
}

/// Индексная картинка в RGBA-байты (эталон — `konung/images.py`):
/// прозрачные пиксели — нули, остальные — цвет из палитры.
pub fn indexed_to_rgba(image: &IndexedImage, palette: &[(u8, u8, u8)]) -> Vec<u8> {
    let mut rgba = vec![0u8; image.width * image.height * 4];
    for i in 0..image.width * image.height {
        if image.alpha[i] != 0 {
            let (r, g, b) = palette[image.indices[i] as usize];
            rgba[4 * i..4 * i + 4].copy_from_slice(&[r, g, b, image.alpha[i]]);
        }
    }
    rgba
}

/// RGB555-картинка в RGBA-байты (эталон — `konung/images.py`).
pub fn rgb555_to_rgba(image: &RgbImage) -> Vec<u8> {
    let mut rgba = vec![0u8; image.width * image.height * 4];
    for i in 0..image.width * image.height {
        if image.alpha[i] != 0 {
            let (r, g, b) = rgb555(image.colors[i]);
            rgba[4 * i..4 * i + 4].copy_from_slice(&[r, g, b, image.alpha[i]]);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Заготовка картинки: заголовок, длины строк, строки команд (как test_rle.py).
    fn encode(width: usize, height: usize, rows: &[Vec<u8>]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&(width as u16).to_le_bytes());
        data.extend_from_slice(&(height as u16).to_le_bytes());
        for row in rows {
            data.extend_from_slice(&(row.len() as u16).to_le_bytes());
        }
        for row in rows {
            data.extend_from_slice(row);
        }
        data
    }

    #[test]
    fn indexed_skip_and_literal() {
        // Строка 0: пропуск 1, два пикселя 7 и 9; строка 1: три пикселя
        let data = encode(3, 2, &[vec![0x81, 2, 7, 9, 0], vec![3, 1, 2, 3, 0]]);
        let image = decode_indexed(&mut Reader::new(&data)).unwrap();
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(image.indices, [0, 7, 9, 1, 2, 3]);
        assert_eq!(image.alpha, [0, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn rgb555_reads_two_bytes_per_pixel() {
        let red = (31u16 << 10).to_le_bytes().to_vec();
        let mut row = vec![0x81, 1];
        row.extend_from_slice(&red);
        row.push(0);
        let data = encode(2, 1, &[row]);
        let image = decode_rgb555(&mut Reader::new(&data)).unwrap();
        assert_eq!(image.colors, [0, 31 << 10]);
        assert_eq!(image.alpha, [0, 255]);
    }

    #[test]
    fn shadow_has_no_pixel_data() {
        let data = encode(4, 1, &[vec![2, 0x81, 1, 0]]);
        let image = decode_shadow(&mut Reader::new(&data)).unwrap();
        assert_eq!(image.alpha, [255, 255, 0, 255]);
    }

    #[test]
    fn reader_stops_after_image() {
        let mut data = encode(1, 1, &[vec![1, 5, 0]]);
        data.extend_from_slice(b"tail");
        let mut reader = Reader::new(&data);
        decode_indexed(&mut reader).unwrap();
        assert_eq!(&data[reader.pos()..], b"tail");
    }

    #[test]
    fn negative_size_is_an_error() {
        let data = (-1i16)
            .to_le_bytes()
            .into_iter()
            .chain([0, 0])
            .collect::<Vec<u8>>();
        assert!(decode_indexed(&mut Reader::new(&data)).is_err());
    }

    #[test]
    fn row_wider_than_image_is_an_error() {
        let data = encode(2, 1, &[vec![3, 1, 2, 3, 0]]);
        assert!(matches!(
            decode_indexed(&mut Reader::new(&data)),
            Err(RleError::RowOverflow { .. })
        ));
    }

    #[test]
    fn indexed_to_rgba_applies_palette_and_alpha() {
        let data = encode(2, 1, &[vec![0x81, 1, 3, 0]]);
        let image = decode_indexed(&mut Reader::new(&data)).unwrap();
        let palette = [(0u8, 0u8, 0u8); 256]
            .into_iter()
            .enumerate()
            .map(|(i, _)| (i as u8, 10, 20))
            .collect::<Vec<_>>();
        assert_eq!(
            indexed_to_rgba(&image, &palette),
            [0, 0, 0, 0, 3, 10, 20, 255]
        );
    }
}
