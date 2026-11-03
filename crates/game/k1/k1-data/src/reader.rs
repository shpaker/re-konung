//! Последовательное чтение little-endian полей из буфера (эталон — `konung/binary.py`).

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ReadError>;

#[derive(Debug, Error)]
pub enum ReadError {
    #[error("нужно {need} байт с {pos}, а в буфере {len}")]
    UnexpectedEof { need: usize, pos: usize, len: usize },
}

/// Последовательное чтение little-endian полей из буфера.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        match self.data.get(self.pos..self.pos.saturating_add(size)) {
            Some(chunk) => {
                self.pos += size;
                Ok(chunk)
            }
            None => Err(ReadError::UnexpectedEof {
                need: size,
                pos: self.pos,
                len: self.data.len(),
            }),
        }
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn s8(&mut self) -> Result<i8> {
        Ok(self.take(1)?[0] as i8)
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn s16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn s32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        self.take(count)
    }

    pub fn skip(&mut self, count: usize) {
        self.pos += count;
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fields_in_order() {
        let data = [0x01u8, 0x02, 0x03, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x09];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.u8().unwrap(), 1);
        assert_eq!(reader.s8().unwrap(), 2);
        assert_eq!(reader.u16().unwrap(), 0x0003);
        assert_eq!(reader.s32().unwrap(), -1);
        assert_eq!(reader.u8().unwrap(), 9);
        assert_eq!(reader.pos(), 9);
    }

    #[test]
    fn bytes_skip_and_seek() {
        let data = [1u8, 2, 3, 4, 5, 6];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.bytes(2).unwrap(), &[1, 2]);
        reader.skip(2);
        reader.seek(1);
        assert_eq!(reader.u8().unwrap(), 2);
        assert_eq!(reader.len(), 6);
        assert!(!reader.is_empty());
    }

    #[test]
    fn short_buffer_is_an_error_and_keeps_position() {
        let data = [1u8, 2];
        let mut reader = Reader::new(&data);
        assert!(reader.u32().is_err());
        assert!(reader.bytes(3).is_err());
        assert_eq!(reader.pos(), 0);
    }
}
