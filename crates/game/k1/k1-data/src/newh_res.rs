//! `NEWH<width><language>.RES` — the images of the new-game menu
//! (`NEWH640R.RES`, `NEWH800E.RES`, …): a plain series of 13 RL-compressed
//! 16-bit images — the background, the tile underlay, the selection dot, the
//! three heroes' portrait pairs (selected/unselected) and the page buttons
//! (Slavik `game_newgame.cpp:96-124`, `resources.cpp:582-630`).
//!
//! Confirmed over every file of both editions (RUS and ENG, 640 and 800):
//! 13 records, each consumed to its declared size, the series ending on the
//! last byte of the file.
//!
//! The portraits (records 3–8) are byte-identical between the editions; the
//! background carries the baked-in title of the edition's language.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, NewhError>;

#[derive(Debug, Error)]
pub enum NewhError {
    /// The file ended inside the record series.
    #[error("unexpected end of file at byte {at}")]
    UnexpectedEnd { at: usize },
    /// A record's decoded bytes did not fill its declared size, or ran past it.
    #[error("image record {index}: decoded {consumed} bytes, the record holds {declared}")]
    ImageSizeMismatch {
        index: usize,
        consumed: usize,
        declared: usize,
    },
    /// The file did not hold the 13 records of the menu.
    #[error("expected 13 image records, got {actual}")]
    ImageCountMismatch { actual: usize },
}

/// The record count of the menu: background, tile, dot, three portrait pairs,
/// four button states.
pub const IMAGE_COUNT: usize = 13;

/// The record numbers of the heroes' portrait pairs, by `Game.N` hero number:
/// hero 0 (box 2 of the original) is the pair 7/8, hero 1 (box 0) — 3/4,
/// hero 2 (box 1) — 5/6. The first of each pair is the selected variant, the
/// second the plain one (`game_newgame.cpp:123-133`).
pub const PORTRAIT_RECORDS: [usize; 3] = [7, 3, 5];

/// One menu image, decoded into straight-alpha RGBA (4 bytes per pixel, row
/// major).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

/// The menu file: the 13 images in record order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewhRes {
    pub images: Vec<Image>,
}

/// The 5-to-8 bit scale of the original's `Lkp5To8` table (gfx.h:62-65).
fn scale5(value: u16) -> u8 {
    ((value * 255 + 15) / 31) as u8
}

/// Parses the 13-record series; the last record must end on the last byte.
pub fn parse(data: &[u8]) -> Result<NewhRes> {
    let mut pos = 0usize;
    let mut images = Vec::with_capacity(IMAGE_COUNT);
    for index in 0..IMAGE_COUNT {
        pos += 4; // the record's unknown leading bytes
        if pos + 4 > data.len() {
            return Err(NewhError::UnexpectedEnd { at: pos });
        }
        let declared = usize::try_from(u32::from_le_bytes(
            data[pos..pos + 4].try_into().expect("4 bytes"),
        ))
        .unwrap_or(usize::MAX);
        pos += 4;
        let end = pos
            .checked_add(declared)
            .ok_or(NewhError::UnexpectedEnd { at: pos })?;
        if end > data.len() {
            return Err(NewhError::UnexpectedEnd { at: end });
        }
        let (image, consumed) = decode_rl16(&data[pos..end])?;
        if consumed != declared {
            return Err(NewhError::ImageSizeMismatch {
                index,
                consumed,
                declared,
            });
        }
        images.push(image);
        pos = end;
    }
    if pos != data.len() {
        return Err(NewhError::ImageCountMismatch {
            actual: images.len(),
        });
    }
    Ok(NewhRes { images })
}

/// Decodes one RL16 image: `w`, `h`, a skipped per-line length table, then
/// per line a byte stream — `0` ends the line, a byte with the high bit set
/// skips that many transparent pixels, any other byte copies that many
/// 16-bit RGB555 pixels (Slavik `resources.cpp:582-630`). Returns the RGBA
/// image and the number of bytes consumed.
fn decode_rl16(data: &[u8]) -> Result<(Image, usize)> {
    let mut pos = 0usize;
    let read_u16 = |pos: &mut usize| -> Result<u16> {
        let Some(bytes) = data.get(*pos..*pos + 2) else {
            return Err(NewhError::UnexpectedEnd { at: *pos });
        };
        *pos += 2;
        Ok(u16::from_le_bytes(bytes.try_into().expect("2 bytes")))
    };
    let width = read_u16(&mut pos)?;
    let height = read_u16(&mut pos)?;
    pos += usize::from(height) * 2; // the per-line byte lengths, never read
    let mut rgba = vec![0u8; usize::from(width) * usize::from(height) * 4];
    for line in 0..usize::from(height) {
        let mut cursor = usize::from(width) * line;
        loop {
            let Some(&count) = data.get(pos) else {
                return Err(NewhError::UnexpectedEnd { at: pos });
            };
            pos += 1;
            if count == 0 {
                break;
            }
            if count & 0x80 != 0 {
                cursor += usize::from(count & 0x7F);
            } else {
                for _ in 0..count {
                    let clr = read_u16(&mut pos)?;
                    let (r, g, b) = (
                        scale5(clr >> 10 & 31),
                        scale5(clr >> 5 & 31),
                        scale5(clr & 31),
                    );
                    let Some(pixel) = rgba.get_mut(4 * cursor..4 * cursor + 4) else {
                        return Err(NewhError::UnexpectedEnd { at: pos });
                    };
                    pixel.copy_from_slice(&[r, g, b, 255]);
                    cursor += 1;
                }
            }
        }
    }
    Ok((
        Image {
            width,
            height,
            rgba,
        },
        pos,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal well-formed file: one tiny image, twice — never shipped,
    /// the algorithm only.
    #[test]
    fn a_series_is_parsed_record_by_record() {
        // image: w=2 h=2, line lengths skipped (2*2 bytes), two rows:
        // row 0: skip 1, copy 1 pixel (red); row 1: copy 2 pixels, then 0.
        let mut image = Vec::new();
        image.extend_from_slice(&2u16.to_le_bytes());
        image.extend_from_slice(&2u16.to_le_bytes());
        image.extend_from_slice(&[0; 4]);
        image.push(0x81); // skip one
        image.push(1); // copy one pixel
        image.extend_from_slice(&[31, 0]); // RGB555 blue
        image.push(0);
        image.push(2);
        image.extend_from_slice(&[31, 0, 31, 0]);
        image.push(0);
        let record = {
            let mut record = Vec::new();
            record.extend_from_slice(&[0; 4]);
            record.extend_from_slice(&(image.len() as u32).to_le_bytes());
            record.extend_from_slice(&image);
            record
        };
        let data = record.repeat(13);
        let parsed = parse(&data).unwrap();
        assert_eq!(parsed.images.len(), 13);
        let image = &parsed.images[0];
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(
            &image.rgba,
            &[
                0, 0, 0, 0, 0, 0, 255, 255, //
                0, 0, 255, 255, 0, 0, 255, 255,
            ]
        );
    }

    #[test]
    fn a_single_image_decodes() {
        let mut image = Vec::new();
        image.extend_from_slice(&2u16.to_le_bytes());
        image.extend_from_slice(&2u16.to_le_bytes());
        image.extend_from_slice(&[0; 4]);
        image.push(0x81);
        image.push(1);
        image.extend_from_slice(&[31, 0]);
        image.push(0);
        image.push(2);
        image.extend_from_slice(&[31, 0, 31, 0]);
        image.push(0);
        let (img, consumed) = decode_rl16(&image).unwrap();
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(consumed, 19);
        assert_eq!(img.rgba.len(), 16);
    }

    #[test]
    fn a_truncated_series_is_an_error() {
        let data = [0u8; 12];
        assert!(matches!(parse(&data), Err(NewhError::UnexpectedEnd { .. })));
    }

    #[test]
    fn trailing_bytes_are_an_error() {
        // 13 minimal well-formed records (a 0×0 image is four bytes of
        // header) plus trailing garbage: the series must end on the file.
        let record = {
            let mut record = Vec::new();
            record.extend_from_slice(&[0; 4]);
            record.extend_from_slice(&4u32.to_le_bytes());
            record.extend_from_slice(&0u16.to_le_bytes());
            record.extend_from_slice(&0u16.to_le_bytes());
            record
        };
        let mut data = Vec::new();
        data.extend(record.repeat(13));
        data.extend_from_slice(&[1, 2, 3]);
        assert!(matches!(
            parse(&data),
            Err(NewhError::ImageCountMismatch { .. })
        ));
    }
}
