//! Мир движка: несколько существ, тик игры, конфликты за клетки и пересчёт
//! маршрута.
//!
//! Это единственный код, который сводит вместе поиск маршрута ([`super::route`]),
//! движение по нему ([`super::walk`]), проходимость ([`super::passage`]),
//! занятость ([`super::occupancy`]) и длительность шага ([`super::pace`]).
//! Правил игры — кто куда идёт и почему — здесь нет: их добавляет часть 3
//! эпика #33.

use super::CreatureId;
use super::grid::{Cell, Direction, Grid};
use super::occupancy::{Holder, Occupancy};
use super::pace::Pace;
use super::passage::Passage;
use super::rng::Rng;
use super::route::{self, RouteRules};
use super::walk::{Step, WalkState, Walker};

/// Политика мира: числа, которые движок выбрал сам, потому что данных о них нет
/// (docs/rust.md, «Решение: пересчёт без дрожи» и «Решение: кто уступает
/// клетку»).
#[derive(Debug, Clone, Copy)]
pub struct WorldRules {
    /// Сколько тиков существо стоит с несъеденным маршрутом, прежде чем его
    /// пересчитать: клетка впереди могла освободиться через тик-два, и дёргать
    /// поиск каждый тик незачем.
    pub wait_before_repath_ticks: u32,
    /// Минимальный интервал между поисками маршрута у одного существа, в тиках:
    /// защита от дрожи, когда и обход не проходит.
    pub repath_cooldown_ticks: u32,
    /// Правила поиска маршрута.
    pub route: RouteRules,
}

impl Default for WorldRules {
    fn default() -> WorldRules {
        WorldRules {
            wait_before_repath_ticks: 12,
            repath_cooldown_ticks: 60,
            route: RouteRules::default(),
        }
    }
}

/// Что произошло с существом на этом тике.
///
/// События — дискретные переходы; текущее состояние спрайта даёт
/// [`World::state`], по нему визуализация ставит спрайт в клетку и выбирает
/// анимацию.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Шаг начат: столько-то тиков существо идёт из `from` в `to`.
    StepStarted {
        creature: CreatureId,
        from: Cell,
        to: Cell,
        direction: Direction,
        ticks: u32,
    },
    /// Шаг закончен: существо стоит в `cell`.
    StepFinished {
        creature: CreatureId,
        cell: Cell,
        direction: Direction,
    },
    /// Маршрут съеден целиком, существо остановилось.
    Arrived { creature: CreatureId, cell: Cell },
    /// Маршрут назначен — первый или пересчитанный: столько-то шагов впереди.
    RouteSet { creature: CreatureId, steps: usize },
    /// Клетку впереди держит чужое существо.
    Waiting {
        creature: CreatureId,
        cell: Cell,
        blocked_by: CreatureId,
    },
    /// Клетка впереди — или клетка, через угол которой шёл шаг, — стала
    /// непроходима.
    TerrainBlocked { creature: CreatureId, cell: Cell },
    /// Маршрута до цели нет: существо сняло приказ и стоит на месте.
    NoRoute { creature: CreatureId, goal: Cell },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldError {
    UnknownCreature,
    OutsideGrid,
    CellTaken,
    /// Клетка непроходима: встать в неё нельзя, даже если она свободна.
    ImpassableCell,
    /// Маршрута нет: цель недоступна, существо никуда не послано.
    NoRoute,
}

/// Чем занята клетка впереди существа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    None,
    Terrain,
    Creature(CreatureId),
}

/// Существо в мире: его движение плюс учёт приказов и ожидания.
#[derive(Debug, Clone)]
struct Actor {
    walker: Walker,
    /// Клетка, куда существо послано; держится, пока существо не дошло.
    goal: Option<Cell>,
    /// Новый приказ, ещё не превращённый в маршрут: получен, когда существо
    /// шло шагом, и исполнится на границе клетки.
    pending: bool,
    /// Сколько тиков существо стоит с маршрутом, который не может начать.
    waiting_ticks: u32,
    /// Тик последнего поиска маршрута.
    last_search: u64,
}

impl Actor {
    fn new(walker: Walker) -> Actor {
        Actor {
            walker,
            goal: None,
            pending: false,
            waiting_ticks: 0,
            last_search: 0,
        }
    }
}

/// Мир перемещения: сетка, проходимость, занятость и существа на одном тике.
pub struct World<P: Passage, K: Pace> {
    grid: Grid,
    passage: P,
    pace: K,
    rules: WorldRules,
    occupancy: Occupancy,
    actors: Vec<Actor>,
    rng: Rng,
    tick: u64,
    /// События приказов, отданных между тиками: выходят первыми на следующем
    /// [`World::tick_step`], в порядке отдачи. Единственный канал событий —
    /// именно `tick_step`, поэтому приказ не может исчезнуть молча.
    pending_events: Vec<Event>,
}

impl<P: Passage, K: Pace> World<P, K> {
    /// `seed` — начало последовательности генератора, который развязывает
    /// равные маршруты: тот же seed и те же данные дают те же события.
    pub fn new(grid: Grid, passage: P, pace: K, rules: WorldRules, seed: u64) -> World<P, K> {
        World {
            grid,
            passage,
            pace,
            rules,
            occupancy: Occupancy::new(),
            actors: Vec::new(),
            rng: Rng::new(seed),
            tick: 0,
            pending_events: Vec::new(),
        }
    }

    pub fn grid(&self) -> Grid {
        self.grid
    }

    pub fn rules(&self) -> WorldRules {
        self.rules
    }

    pub fn rules_mut(&mut self) -> &mut WorldRules {
        &mut self.rules
    }

    pub fn passage(&self) -> &P {
        &self.passage
    }

    /// Поставщик проходимости можно менять: поваленное дерево или новый лес
    /// переключают клетку, а маршруты пересчитывает движок.
    pub fn passage_mut(&mut self) -> &mut P {
        &mut self.passage
    }

    pub fn pace(&self) -> &K {
        &self.pace
    }

    /// Номер тика, прошедший с создания мира.
    pub fn tick_number(&self) -> u64 {
        self.tick
    }

    /// Поставить существо в клетку; номер — следующий по порядку.
    ///
    /// Непроходимая клетка — `Err(ImpassableCell)`: существо внутри стены
    /// ломало бы инвариант «никто не стоит в непроходимой клетке», который мир
    /// проверяет на каждом тике.
    pub fn add(&mut self, cell: Cell) -> Result<CreatureId, WorldError> {
        if !self.grid.contains(cell) {
            return Err(WorldError::OutsideGrid);
        }
        if !self.passage.is_walkable(cell) {
            return Err(WorldError::ImpassableCell);
        }
        if self.occupancy.holder(cell).is_some() {
            return Err(WorldError::CellTaken);
        }
        let creature = CreatureId::new(self.actors.len() as u32);
        self.occupancy.stand(cell, creature);
        self.actors.push(Actor::new(Walker::new(creature, cell)));
        Ok(creature)
    }

    fn index(&self, creature: CreatureId) -> Option<usize> {
        let index = creature.raw() as usize;
        self.actors
            .get(index)
            .filter(|actor| actor.walker.creature == creature)
            .map(|_| index)
    }

    /// Послать существо в клетку и вернуть число шагов маршрута.
    ///
    /// Если существо уже идёт шагом, приказ не отменяет начатый шаг: маршрут
    /// пересчитается на границе клетки, когда клетку ещё можно отменить, и
    /// тогда возвращается `Ok(0)`.
    /// `Err(NoRoute)` — маршрута нет, существо остаётся на месте.
    ///
    /// События поиска (`RouteSet`/`NoRoute`) кладутся в общий канал мира и
    /// выходят первым делом на следующем [`World::tick_step`] — вне тика движок
    /// событий не отдаёт, но и не теряет их.
    pub fn move_to(&mut self, creature: CreatureId, goal: Cell) -> Result<usize, WorldError> {
        let index = self.index(creature).ok_or(WorldError::UnknownCreature)?;
        if !self.grid.contains(goal) {
            return Err(WorldError::OutsideGrid);
        }
        self.actors[index].goal = Some(goal);
        self.actors[index].pending = true;
        self.actors[index].waiting_ticks = 0;
        if self.actors[index].walker.is_walking() {
            return Ok(0);
        }
        let mut events = Vec::new();
        let steps = self.search_route(index, &mut events);
        self.pending_events.append(&mut events);
        steps.ok_or(WorldError::NoRoute)
    }

    /// Снять приказ: существо доходит начатый шаг и останавливается.
    pub fn stop(&mut self, creature: CreatureId) -> Result<(), WorldError> {
        let index = self.index(creature).ok_or(WorldError::UnknownCreature)?;
        self.actors[index].goal = None;
        self.actors[index].pending = false;
        self.actors[index].waiting_ticks = 0;
        self.actors[index].walker.clear_route();
        Ok(())
    }

    pub fn state(&self, creature: CreatureId) -> Option<WalkState> {
        let index = self.index(creature)?;
        Some(self.actors[index].walker.state())
    }

    /// Состояния всех существ — в порядке их номеров, чтобы обход был
    /// детерминирован.
    pub fn states(&self) -> Vec<WalkState> {
        self.actors
            .iter()
            .map(|actor| actor.walker.state())
            .collect()
    }

    pub fn cell_of(&self, creature: CreatureId) -> Option<Cell> {
        let index = self.index(creature)?;
        Some(self.actors[index].walker.cell())
    }

    pub fn creature_at(&self, cell: Cell) -> Option<CreatureId> {
        self.occupancy.occupant(cell)
    }

    /// Кто держит клетку: стоя или резервом шага.
    pub fn holder(&self, cell: Cell) -> Option<Holder> {
        self.occupancy.holder(cell)
    }

    pub fn occupancy(&self) -> &Occupancy {
        &self.occupancy
    }

    /// Один тик игры: сначала события приказов, отданных между тиками, потом
    /// приходы, потом пересчёты и новые шаги.
    ///
    /// Это единственный канал событий движка: всё, что произошло (включая поиск
    /// маршрута по приказу `move_to`), выходит отсюда, в порядке тика.
    ///
    /// Приходы первыми, поэтому существо, сошедшее с клетки на этом тике, не
    /// мешает тому, кто входит в неё в этом же тике. Шаг длительностью `d`
    /// тиков, начатый на тике `t`, заканчивается на тике `t + d`: ровно `d`
    /// тиков занято ходьбой, и паузы между шагами нет.
    pub fn tick_step(&mut self) -> Vec<Event> {
        self.tick += 1;
        let mut events = std::mem::take(&mut self.pending_events);
        self.finish_steps(&mut events);
        self.start_steps(&mut events);
        events
    }

    /// Завершить начатые шаги.
    fn finish_steps(&mut self, events: &mut Vec<Event>) {
        for index in 0..self.actors.len() {
            let creature = self.actors[index].walker.creature;
            let previous = self.actors[index].walker.cell();
            if let Some(step) = self.actors[index].walker.step()
                && step.elapsed >= step.ticks
                && !self.can_stand(step.to)
            {
                self.cancel_step(index, step, events);
                continue;
            }
            if !self.actors[index].walker.tick() {
                continue;
            }
            let cell = self.actors[index].walker.cell();
            self.occupancy.release(previous, creature);
            // Клетку резервировало это существо, поэтому занять её удаётся
            // всегда; инвариант проверяется в debug, в release он стоит ноль.
            let held = self.occupancy.stand(cell, creature);
            debug_assert!(held, "на приходе клетку {cell:?} держит не то существо");
            let direction = Direction::between(previous, cell);
            if let Some(direction) = direction {
                events.push(Event::StepFinished {
                    creature,
                    cell,
                    direction,
                });
            }
        }
    }

    /// Отменить шаг, клетка назначения которого стала непроходима: существо
    /// остаётся там, где стояло, и ждёт пересчёта.
    fn cancel_step(&mut self, index: usize, step: Step, events: &mut Vec<Event>) {
        let creature = self.actors[index].walker.creature;
        self.actors[index].walker.cancel_step();
        self.occupancy.release(step.to, creature);
        self.actors[index].waiting_ticks = self.rules.wait_before_repath_ticks;
        events.push(Event::TerrainBlocked {
            creature,
            cell: step.to,
        });
    }

    /// Начать шаги там, где существо ждёт, исполнить новые приказы и пересчитать
    /// заблокированные маршруты.
    ///
    /// Обход — по номерам существ: он детерминирован и решает спор за клетку,
    /// потому что резерв исключительный (docs/rust.md, «Решение: кто уступает
    /// клетку»).
    fn start_steps(&mut self, events: &mut Vec<Event>) {
        for index in 0..self.actors.len() {
            if self.actors[index].walker.is_walking() {
                continue;
            }
            let creature = self.actors[index].walker.creature;
            if self.actors[index].pending {
                // Без `continue`: новый маршрут, найденный на этом тике, тут же
                // расходуется первым шагом, иначе между приходом и началом шага
                // вставлялся бы пустой тик (60 мс простоя при смене цели).
                self.search_route(index, events);
            }
            if self.actors[index].walker.finish() {
                self.actors[index].goal = None;
                let cell = self.actors[index].walker.cell();
                events.push(Event::Arrived { creature, cell });
                continue;
            }
            if !self.actors[index].walker.is_waiting() {
                self.actors[index].waiting_ticks = 0;
                continue;
            }
            let from = self.actors[index].walker.cell();
            let Some(next) = self.actors[index].walker.next_cell() else {
                continue;
            };
            match self.block_reason(creature, from, next) {
                Block::None => self.begin_step(index, next, events),
                Block::Creature(other) => {
                    events.push(Event::Waiting {
                        creature,
                        cell: next,
                        blocked_by: other,
                    });
                    self.actors[index].waiting_ticks += 1;
                    self.repath_if_due(index, events);
                }
                Block::Terrain => {
                    events.push(Event::TerrainBlocked {
                        creature,
                        cell: next,
                    });
                    self.actors[index].waiting_ticks = self.rules.wait_before_repath_ticks;
                    self.repath_if_due(index, events);
                }
            }
        }
    }

    /// Пересчитать маршрут, если пришло время: существо отстояло
    /// [`WorldRules::wait_before_repath_ticks`] и с прошлого поиска прошло не
    /// меньше [`WorldRules::repath_cooldown_ticks`].
    fn repath_if_due(&mut self, index: usize, events: &mut Vec<Event>) {
        let rules = self.rules;
        let actor = &self.actors[index];
        let due = actor.waiting_ticks >= rules.wait_before_repath_ticks
            && self.tick - actor.last_search >= u64::from(rules.repath_cooldown_ticks);
        if !due || actor.goal.is_none() {
            return;
        }
        self.search_route(index, events);
    }

    /// Поиск маршрута из текущей клетки к цели существа; возвращает число шагов.
    fn search_route(&mut self, index: usize, events: &mut Vec<Event>) -> Option<usize> {
        let actor = &self.actors[index];
        let (Some(goal), creature, from) = (actor.goal, actor.walker.creature, actor.walker.cell())
        else {
            return None;
        };
        self.actors[index].last_search = self.tick;
        self.actors[index].waiting_ticks = 0;
        let request = route::RouteRequest {
            grid: &self.grid,
            passage: &self.passage,
            occupancy: &self.occupancy,
            rules: &self.rules.route,
            mover: creature,
            from,
            goal,
        };
        let found = route::find_route(&request, &mut self.rng);
        match found {
            Some(route) => {
                let steps = route.len();
                self.actors[index].pending = false;
                self.actors[index].walker.set_route(&route);
                events.push(Event::RouteSet { creature, steps });
                Some(steps)
            }
            None => {
                self.actors[index].pending = false;
                self.actors[index].goal = None;
                self.actors[index].walker.clear_route();
                events.push(Event::NoRoute { creature, goal });
                None
            }
        }
    }

    /// Начать следующий шаг: зарезервировать клетку и завести счётчик тиков.
    fn begin_step(&mut self, index: usize, next: Cell, events: &mut Vec<Event>) {
        let creature = self.actors[index].walker.creature;
        let from = self.actors[index].walker.cell();
        let Some(direction) = Direction::between(from, next) else {
            self.actors[index].waiting_ticks = self.rules.wait_before_repath_ticks;
            self.repath_if_due(index, events);
            return;
        };
        if !self.occupancy.reserve(next, creature) {
            return;
        }
        let ticks = self.pace.step_ticks(creature, direction);
        if self.actors[index].walker.start_step(ticks).is_none() {
            self.occupancy.release(next, creature);
            return;
        }
        self.actors[index].waiting_ticks = 0;
        events.push(Event::StepStarted {
            creature,
            from,
            to: next,
            direction,
            ticks: ticks.max(1),
        });
    }

    /// Можно ли здесь стоять: клетка в сетке и проходима.
    fn can_stand(&self, cell: Cell) -> bool {
        self.grid.contains(cell) && self.passage.is_walkable(cell)
    }

    /// Чем занята клетка впереди. Непроходимым считается и шаг через угол,
    /// когда боковая клетка закрыта.
    fn block_reason(&self, creature: CreatureId, from: Cell, next: Cell) -> Block {
        if !self.can_stand(next) {
            return Block::Terrain;
        }
        if let Some(holder) = self.occupancy.holder(next)
            && holder.creature != creature
        {
            return Block::Creature(holder.creature);
        }
        match Direction::between(from, next) {
            Some(direction)
                if route::is_open_corner(&self.grid, &self.passage, from, direction) =>
            {
                Block::None
            }
            _ => Block::Terrain,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::movement::route::Route;

    #[derive(Default, Clone)]
    struct Map {
        blocked: HashSet<Cell>,
    }

    impl Passage for Map {
        fn is_walkable(&self, cell: Cell) -> bool {
            !self.blocked.contains(&cell)
        }
    }

    #[derive(Clone, Copy)]
    struct Uniform(u32);

    impl Pace for Uniform {
        fn step_ticks(&self, _creature: CreatureId, _direction: Direction) -> u32 {
            self.0
        }
    }

    fn world(columns: u32, rows: u32) -> World<Map, Uniform> {
        World::new(
            Grid::new(columns, rows),
            Map::default(),
            Uniform(2),
            WorldRules::default(),
            0,
        )
    }

    #[test]
    fn creature_is_placed_and_taken_cells_are_refused() {
        let mut world = world(8, 8);
        let cell = Cell::new(2, 2);
        let first = world.add(cell).unwrap();
        assert_eq!(world.creature_at(cell), Some(first));
        assert_eq!(world.add(cell), Err(WorldError::CellTaken));
        assert_eq!(world.add(Cell::new(8, 0)), Err(WorldError::OutsideGrid));
    }

    #[test]
    fn add_refuses_an_impassable_cell() {
        // Существо в стене ломало бы инвариант «никто не стоит в непроходимой
        // клетке», который мир проверяет каждый тик
        let mut world = world(8, 8);
        let wall = Cell::new(3, 3);
        world.passage_mut().blocked.insert(wall);
        assert_eq!(world.add(wall), Err(WorldError::ImpassableCell));
        assert_eq!(world.occupancy().holder(wall), None);
        assert!(world.add(Cell::new(4, 4)).is_ok());
    }

    #[test]
    fn order_events_reach_the_consumer_through_the_tick() {
        // Единственный канал событий — tick_step: приказ вне тика не должен
        // теряться вместе с временным вектором
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        assert_eq!(world.move_to(creature, Cell::new(5, 2)), Ok(4));
        let events = world.tick_step();
        assert!(
            events.contains(&Event::RouteSet { creature, steps: 4 }),
            "{events:?}"
        );
        assert!(
            events.contains(&Event::StepStarted {
                creature,
                from: Cell::new(1, 2),
                to: Cell::new(2, 2),
                direction: Direction::East,
                ticks: 2,
            }),
            "{events:?}"
        );
        // и на втором тике повторного приказа уже не появляется
        assert!(
            !world
                .tick_step()
                .iter()
                .any(|event| matches!(event, Event::RouteSet { .. }))
        );
    }

    #[test]
    fn a_deferred_order_starts_its_first_step_on_the_border_tick() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        assert_eq!(world.move_to(creature, Cell::new(3, 2)), Ok(2));
        assert!(
            world
                .tick_step()
                .iter()
                .any(|event| matches!(event, Event::StepStarted { .. }))
        );
        // приказ приходит, когда существо идёт шагом: он дождётся границы клетки
        assert_eq!(world.move_to(creature, Cell::new(6, 2)), Ok(0));
        world.tick_step();
        let events = world.tick_step();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::StepFinished { .. })),
            "{events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::RouteSet { .. })),
            "{events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::StepStarted { .. })),
            "первый шаг после пересчёта должен начаться в том же тике: {events:?}"
        );
    }

    #[test]
    fn a_free_but_unreachable_goal_is_not_replaced_by_a_neighbour() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(0, 0)).unwrap();
        let goal = Cell::new(5, 5);
        // Цель свободна и проходима, но отрезана: все восемь соседей в стене
        let neighbours: Vec<Cell> = Direction::ALL
            .into_iter()
            .filter_map(|direction| goal.neighbour(direction))
            .collect();
        for cell in &neighbours {
            world.passage_mut().blocked.insert(*cell);
        }
        assert!(
            world.passage().is_walkable(goal),
            "цель должна оставаться свободной"
        );
        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        let events = world.tick_step();
        assert!(
            events.contains(&Event::NoRoute { creature, goal }),
            "рядом с целью есть достижимые клетки — но в приказе была сама цель: {events:?}"
        );
        assert_eq!(world.cell_of(creature), Some(Cell::new(0, 0)));
    }

    #[test]
    fn move_to_reports_the_route_length() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        assert_eq!(world.move_to(creature, Cell::new(5, 2)), Ok(4));
        assert_eq!(world.state(creature).unwrap().goal, Some(Cell::new(5, 2)));
        assert_eq!(
            world.move_to(CreatureId::new(9), Cell::new(1, 1)),
            Err(WorldError::UnknownCreature)
        );
        assert_eq!(
            world.move_to(creature, Cell::new(9, 9)),
            Err(WorldError::OutsideGrid)
        );
    }

    /// Шаг в 2 тика, начатый на тике 1, приводит существо в новую клетку на
    /// тике 3; следующий шаг начинается в том же тике.
    #[test]
    fn step_finishes_on_the_expected_tick_and_creature_stops() {
        let mut world = world(8, 8);
        let from = Cell::new(1, 2);
        let creature = world.add(from).unwrap();
        world.move_to(creature, Cell::new(3, 2)).unwrap();

        let events = world.tick_step();
        assert_eq!(
            events,
            vec![
                // приказ отдан вне тика: его событие выходит первым на этом тике
                Event::RouteSet { creature, steps: 2 },
                Event::StepStarted {
                    creature,
                    from,
                    to: Cell::new(2, 2),
                    direction: Direction::East,
                    ticks: 2,
                },
            ]
        );
        assert_eq!(world.tick_step(), Vec::new(), "середина шага пуста");
        assert_eq!(world.cell_of(creature), Some(from));

        let events = world.tick_step();
        assert!(events.contains(&Event::StepFinished {
            creature,
            cell: Cell::new(2, 2),
            direction: Direction::East,
        }));
        assert!(events.contains(&Event::StepStarted {
            creature,
            from: Cell::new(2, 2),
            to: Cell::new(3, 2),
            direction: Direction::East,
            ticks: 2,
        }));
        assert_eq!(world.cell_of(creature), Some(Cell::new(2, 2)));

        world.tick_step();
        let events = world.tick_step();
        assert_eq!(world.cell_of(creature), Some(Cell::new(3, 2)));
        assert!(
            events.contains(&Event::Arrived {
                creature,
                cell: Cell::new(3, 2)
            }),
            "{events:?}"
        );
        assert_eq!(
            world.tick_step(),
            Vec::new(),
            "остановившееся существо шумит"
        );
        assert!(!world.states()[0].moving);
    }

    #[test]
    fn reservation_is_dropped_when_the_target_becomes_impassable() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(2, 2)).unwrap();
        world.move_to(creature, Cell::new(5, 2)).unwrap();
        world.tick_step();
        assert_eq!(
            world.holder(Cell::new(3, 2)).map(|holder| holder.creature),
            Some(creature)
        );
        world.passage_mut().blocked.insert(Cell::new(3, 2));
        world.tick_step();
        let events = world.tick_step();
        assert!(
            events.contains(&Event::TerrainBlocked {
                creature,
                cell: Cell::new(3, 2)
            }),
            "{events:?}"
        );
        assert_eq!(world.cell_of(creature), Some(Cell::new(2, 2)));
        assert_eq!(world.holder(Cell::new(3, 2)), None, "резерв не снят");
        assert_eq!(world.creature_at(Cell::new(2, 2)), Some(creature));
    }

    #[test]
    fn order_issued_mid_step_is_taken_at_the_border() {
        let mut world = world(12, 12);
        let from = Cell::new(2, 2);
        let creature = world.add(from).unwrap();
        world.move_to(creature, Cell::new(10, 2)).unwrap();
        world.tick_step();
        let other = Cell::new(2, 8);
        // Новый приказ посреди шага: начатый шаг не отменяется.
        assert_eq!(world.move_to(creature, other), Ok(0));
        world.tick_step();
        assert_eq!(world.cell_of(creature), Some(from));
        let events = world.tick_step();
        assert_eq!(world.cell_of(creature), Some(Cell::new(3, 2)));
        assert!(
            events.iter().any(
                |event| matches!(event, Event::RouteSet { creature: c, .. } if *c == creature)
            ),
            "{events:?}"
        );
        assert_eq!(world.state(creature).unwrap().goal, Some(other));
    }

    #[test]
    fn stop_keeps_the_current_step_but_drops_the_route() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 1)).unwrap();
        world.move_to(creature, Cell::new(6, 1)).unwrap();
        world.tick_step();
        world.stop(creature).unwrap();
        // Начатый шаг доходит до конца: два тика по 2 тика на шаг.
        world.tick_step();
        world.tick_step();
        assert_eq!(
            world.cell_of(creature),
            Some(Cell::new(2, 1)),
            "начатый шаг отменён"
        );
        for _ in 0..8 {
            world.tick_step();
        }
        assert_eq!(world.cell_of(creature), Some(Cell::new(2, 1)));
        assert_eq!(world.states()[0].goal, None);
    }

    #[test]
    fn goal_reached_without_a_step_arrives_immediately() {
        let mut world = world(8, 8);
        let cell = Cell::new(3, 3);
        let creature = world.add(cell).unwrap();
        assert_eq!(world.move_to(creature, cell), Ok(0));
        let events = world.tick_step();
        assert!(
            events.contains(&Event::Arrived { creature, cell }),
            "{events:?}"
        );
        assert_eq!(world.cell_of(creature), Some(cell));
    }

    #[test]
    fn unreachable_order_is_refused() {
        let mut world = world(12, 12);
        let creature = world.add(Cell::new(1, 6)).unwrap();
        let goal = Cell::new(10, 6);
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;
        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        assert_eq!(world.cell_of(creature), Some(Cell::new(1, 6)));
        assert_eq!(
            world.states()[0].goal,
            None,
            "отказался от недостижимой цели"
        );
        // отказ тоже доходит до потребителя: единственный канал событий — тик
        assert_eq!(world.tick_step(), vec![Event::NoRoute { creature, goal }]);
    }

    #[test]
    fn route_can_be_built_without_the_world() {
        let grid = Grid::new(8, 8);
        let map = Map::default();
        let occupancy = Occupancy::new();
        let direct =
            Route::from_cells(&[Cell::new(1, 1), Cell::new(2, 1)], Cell::new(2, 1)).unwrap();
        assert_eq!(direct.len(), 1);
        let request = route::RouteRequest {
            grid: &grid,
            passage: &map,
            occupancy: &occupancy,
            rules: &RouteRules::default(),
            mover: CreatureId::new(0),
            from: Cell::new(1, 1),
            goal: Cell::new(4, 4),
        };
        assert_eq!(
            route::find_route(&request, &mut Rng::new(0)).map(|found| found.len()),
            Some(5)
        );
    }
}
