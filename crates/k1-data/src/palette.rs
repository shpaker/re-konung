//! Цвета RGB555: 0RRRRRGG GGGBBBBB, младший байт первым
//! (эталон — `konung/palette.py`).

/// 5 бит → 8 бит: старшие биты повторяются в младших, 31 → 255.
pub fn expand5(value: u16) -> u8 {
    (((value << 3) | (value >> 2)) & 0xFF) as u8
}

pub fn rgb555(color: u16) -> (u8, u8, u8) {
    (
        expand5((color >> 10) & 0x1F),
        expand5((color >> 5) & 0x1F),
        expand5(color & 0x1F),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand5_repeats_high_bits() {
        assert_eq!(expand5(0), 0);
        assert_eq!(expand5(31), 255);
        assert_eq!(expand5(1), 0b0000_1000);
        assert_eq!(expand5(0b10101), 0b1010_1101);
    }

    #[test]
    fn rgb555_splits_channels() {
        assert_eq!(rgb555(31 << 10), (255, 0, 0));
        assert_eq!(rgb555(31 << 5), (0, 255, 0));
        assert_eq!(rgb555(31), (0, 0, 255));
        assert_eq!(rgb555(0), (0, 0, 0));
    }
}
