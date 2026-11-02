//! Детерминированный генератор случайных чисел движка.
//!
//! Свой и лёгкий: тяжёлая зависимость ради нескольких бит на тик не нужна, а
//! разнородные `random`-крейты не дают того, что здесь важно, — одинаковый seed
//! обязан давать одинаковую последовательность событий (issue #35).
//!
//! Генератор трогает только выбор среди равноценных вариантов: порядок
//! перебора направлений при поиске маршрута. Правил игры он не задаёт — там,
//! где оригинал выбирает по данным, данные и приходят.

/// splitmix64: два умножения и три XOR на состояние в 64 бита.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub const fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// Следующее 64-битное значение.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Равномерное значение от 0 до `limit` (не включая), без остатка от
    /// взятия по модулю: старшие биты широкого произведения.
    ///
    /// `limit == 0` даёт 0 — вызывать его бессмысленно, но и паники тут не
    /// нужно: значение всё равно ни на что не влияет.
    pub fn below(&mut self, limit: u64) -> u64 {
        if limit == 0 {
            return 0;
        }
        (((self.next_u64() as u128) * (limit as u128)) >> 64) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_the_same_sequence() {
        let mut first = Rng::new(7);
        let mut second = Rng::new(7);
        for _ in 0..16 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut rng = Rng::new(1234);
        let mut seen = [0u32; 8];
        for _ in 0..800 {
            let value = rng.below(8);
            assert!(value < 8);
            seen[value as usize] += 1;
        }
        for count in seen {
            assert!(count > 0, "значение не выпало ни разу за 800 тяг");
        }
        assert_eq!(rng.below(0), 0);
    }
}
