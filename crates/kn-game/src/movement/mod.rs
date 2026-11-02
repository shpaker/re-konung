//! Общий движок перемещения: изометрическая сетка мелкой решётки, проходимость
//! и занятость клеток, поиск маршрута, движение по маршруту и игровой тик.
//!
//! Слой не знает ни о Bevy, ни о специфике «Князя 1»: механика перемещения в
//! K1 и K2 предположительно почти одинакова (AGENTS.md, «Приоритет: современный
//! опыт»), поэтому она живёт здесь, а сведения части приходят извне:
//!
//! - проходимость клетки — [`passage::Passage`] (данные карты, объекты);
//! - длительность шага в тиках — [`pace::Pace`] (число кадров анимации);
//! - занятость клеток — состояние движка ([`occupancy::Occupancy`]): только он
//!   знает, кто куда идёт шагом.
//!
//! Часов движок не ведёт: он считает тики, а сколько в тике миллисекунд знает
//! внешний мир — для K1 это 60 мс, [`crate::TICK_MS`] (Slavik `game.h:30`).
//!
//! Правила игры — кто куда идёт и почему — не здесь: их добавляет часть 3
//! эпика #33. Выдуманных правил движок не содержит: всё, что не подтверждено по
//! оригиналу, либо отдаётся поставщику данных, либо оформлено решением проекта с
//! обоснованием в `docs/rust.md`.
//!
//! Поиск маршрута ([`route`]) и движение по нему ([`walk`]) разведены по
//! разным модулям: у поиска нет понятия тика, у движения — знания о том, как
//! маршрут искался. [`world`] сводит их вместе на одном тике.

pub mod grid;
pub mod occupancy;
pub mod pace;
pub mod passage;
pub mod rng;
pub mod route;
pub mod walk;
pub mod world;

/// Существо движка: его номер, выданный [`world::World::add`].
///
/// Номер — это и порядок обхода на тике: он детерминирован, и спор за клетку
/// решает тот, кто раньше по номеру (docs/rust.md, «Решение: кто уступает
/// клетку»).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CreatureId(u32);

impl CreatureId {
    pub const fn new(raw: u32) -> CreatureId {
        CreatureId(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

pub use grid::{Cell, Direction, FINE_HEIGHT, FINE_WIDTH, Grid, ODD_ROW_SHIFT, STEP_X, STEP_Y};
pub use occupancy::{Holder, Occupancy};
pub use pace::Pace;
pub use passage::Passage;
pub use rng::Rng;
pub use route::{Leg, Route, RouteRules, find_route};
pub use walk::{Step, WalkState, Walker};
pub use world::{Event, World, WorldError, WorldRules};
