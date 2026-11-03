//! Последовательная little-endian запись полей в буфер (парная к [`crate::reader`]).

/// Пишет little-endian поля в конец буфера, как `struct.pack` у Python.
#[derive(Debug, Default)]
pub struct Writer {
    out: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            out: Vec::with_capacity(capacity),
        }
    }

    pub fn u8(&mut self, value: u8) {
        self.out.push(value);
    }

    pub fn s8(&mut self, value: i8) {
        self.out.push(value as u8);
    }

    pub fn u16(&mut self, value: u16) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    pub fn s16(&mut self, value: i16) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    pub fn u32(&mut self, value: u32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    pub fn s32(&mut self, value: i32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    pub fn f32(&mut self, value: f32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    pub fn bytes(&mut self, data: &[u8]) {
        self.out.extend_from_slice(data);
    }

    pub fn zeros(&mut self, count: usize) {
        self.out.resize(self.out.len() + count, 0);
    }

    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use crate::reader::Reader;

    use super::*;

    #[test]
    fn writes_fields_in_order() {
        let mut writer = Writer::new();
        writer.u8(1);
        writer.s8(-2);
        writer.u16(0x0304);
        writer.s16(-1);
        writer.s32(-1);
        writer.f32(1.5);
        writer.bytes(&[9, 9]);
        let data = writer.into_vec();
        assert_eq!(
            data,
            [
                1, 0xFE, 0x04, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0xC0, 0x3F, 9,
                9
            ]
        );
        let mut reader = Reader::new(&data);
        assert_eq!(reader.u8().unwrap(), 1);
        assert_eq!(reader.s8().unwrap(), -2);
        assert_eq!(reader.u16().unwrap(), 0x0304);
        assert_eq!(reader.s16().unwrap(), -1);
        assert_eq!(reader.s32().unwrap(), -1);
        assert_eq!(reader.f32().unwrap(), 1.5);
        assert_eq!(reader.bytes(2).unwrap(), &[9, 9]);
    }

    #[test]
    fn zeros_extend_buffer() {
        let mut writer = Writer::new();
        writer.u8(7);
        writer.zeros(3);
        assert_eq!(writer.into_vec(), [7, 0, 0, 0]);
    }
}
