//! Движение по маршруту: существо идёт шаг за шагом, шаг длится заданное
//! число тиков.
//!
//! Поиском маршрута этот код не занимается и про A\*, проходимость и занятость
//! ничего не знает — он потребляет готовый маршрут ([`super::route::Route`]) и
//! отвечает на один вопрос: в какой клетке существо стоит сейчас и куда идёт
//! шагом (issue #35: «поиск маршрута и движение по нему — разные части кода»).

use super::CreatureId;
use super::grid::{Cell, Direction};
use super::route::Route;

/// Шаг существа: в какую клетку оно идёт и сколько тиков это длится.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub to: Cell,
    pub direction: Direction,
    /// Сколько тиков длится шаг — ровно столько, сколько сказал поставщик данных.
    pub ticks: u32,
    /// Сколько тиков шага прошло, включая тот, на котором шаг начался.
    pub elapsed: u32,
}

/// Состояние одного существа: где стоит, куда идёт и какой анимацией занято.
///
/// Ровно это нужно внешнему миру, чтобы поставить спрайт в клетку и выбрать
/// анимацию; как спрайт рисуют, движок не знает.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkState {
    pub creature: CreatureId,
    /// Клетка, в которой существо стоит.
    pub cell: Cell,
    /// Клетка, из которой пришли в текущую: по ней визуализация сортирует
    /// спрайты и досчитывает скольжение.
    pub entered_from: Option<Cell>,
    /// Куда существо просили идти; `None` — дошло или идёт без цели.
    pub goal: Option<Cell>,
    /// Пиксельная точка клетки, где существо стоит.
    pub pixel: (i32, i32),
    /// Направление, к которому существо повёрнуто: меняется на границе клетки.
    pub direction: Option<Direction>,
    /// Идёт ли сейчас шаг.
    pub moving: bool,
    /// Клетка, в которую идёт шаг.
    pub step_to: Option<Cell>,
    pub step_ticks: u32,
    pub step_elapsed: u32,
}

impl WalkState {
    /// Сколько тиков осталось до конца текущего шага.
    pub fn ticks_left_in_step(&self) -> u32 {
        self.step_ticks.saturating_sub(self.step_elapsed)
    }
}

/// Существо, идущее по маршруту.
///
/// Маршрут потребляется по одному шагу: шаг идёт `ticks` тиков, после чего
/// существо переходит в следующую клетку. Во время шага существо занимает
/// прежнюю клетку и резервирует новую; направление меняется только на границе
/// клетки (issue #35, критерий 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walker {
    pub creature: CreatureId,
    cell: Cell,
    entered_from: Option<Cell>,
    goal: Option<Cell>,
    /// Оставшиеся шаги; первый — куда существо идёт сейчас или собирается.
    route: Vec<Direction>,
    step: Option<Step>,
}

impl Walker {
    /// Существо без маршрута, стоящее в клетке.
    pub fn new(creature: CreatureId, cell: Cell) -> Walker {
        Walker {
            creature,
            cell,
            entered_from: None,
            goal: None,
            route: Vec::new(),
            step: None,
        }
    }

    pub fn cell(&self) -> Cell {
        self.cell
    }

    pub fn entered_from(&self) -> Option<Cell> {
        self.entered_from
    }

    /// Клетка, куда существо просили идти (после обхода занятой цели — фактическая
    /// конечная клетка маршрута).
    pub fn goal(&self) -> Option<Cell> {
        self.goal
    }

    pub fn route_len(&self) -> usize {
        self.route.len()
    }

    pub fn is_walking(&self) -> bool {
        self.step.is_some()
    }

    /// Идёт ли существо к цели — шагом или между шагами.
    pub fn is_moving(&self) -> bool {
        !self.route.is_empty() || self.step.is_some()
    }

    /// Ждёт ли существо на месте с несъеденным маршрутом: клетка впереди
    /// занята или стала непроходимой.
    pub fn is_waiting(&self) -> bool {
        self.step.is_none() && !self.route.is_empty()
    }

    /// Направление, к которому существо повёрнуто: направление шага, а стоя —
    /// направление следующего шага маршрута, а после прихода — направление,
    /// откуда оно пришло.
    pub fn facing(&self) -> Option<Direction> {
        if let Some(step) = self.step {
            return Some(step.direction);
        }
        if let Some(&next) = self.route.first() {
            return Some(next);
        }
        self.entered_from
            .and_then(|from| Direction::between(from, self.cell))
    }

    /// Клетка, в которую смотрит следующий шаг.
    pub fn next_cell(&self) -> Option<Cell> {
        let direction = *self.route.first()?;
        self.cell.neighbour(direction)
    }

    pub fn step(&self) -> Option<Step> {
        self.step
    }

    /// Сколько тиков остаётся до конца текущего шага.
    pub fn ticks_left_in_step(&self) -> u32 {
        self.step
            .map(|step| step.ticks.saturating_sub(step.elapsed))
            .unwrap_or(0)
    }

    /// Взять маршрут: существо пойдёт по нему из своей текущей клетки.
    ///
    /// Маршрут заменяется целиком и только когда существо не идёт шагом:
    /// посреди шага клетку уже не отменить, поэтому пересчёт ждут на границе
    /// клетки (docs/rust.md, «Решение: пересчёт без дрожи»).
    /// `false` — шаг идёт, маршрут остался прежним.
    pub fn set_route(&mut self, route: &Route) -> bool {
        if self.step.is_some() {
            return false;
        }
        self.route = route.legs().iter().map(|leg| leg.direction).collect();
        self.goal = Some(route.goal());
        true
    }

    /// Отменить маршрут; существо остаётся там, где стоит.
    pub fn clear_route(&mut self) {
        self.route.clear();
        self.goal = None;
    }

    /// Начать следующий шаг длительностью `ticks` тиков.
    ///
    /// Клетку назначения существо уже зарезервировало за собой — это забота
    /// [`super::world`], здесь только учёт времени. `None` — идти некуда или шаг
    /// уже идёт. Длительность 0 считается одним тиком: нулевого шага не бывает,
    /// а паниковать из-за данных нельзя.
    pub fn start_step(&mut self, ticks: u32) -> Option<Direction> {
        if self.step.is_some() {
            return None;
        }
        let direction = *self.route.first()?;
        let to = self.cell.neighbour(direction)?;
        self.step = Some(Step {
            to,
            direction,
            ticks: ticks.max(1),
            elapsed: 1,
        });
        Some(direction)
    }

    /// Продвинуть существо на тик. `true` — на этом тике шаг закончился и
    /// существо встало в новую клетку.
    ///
    /// Шаг длительностью `d` тиков завершается на `d`-м тике после начала:
    /// ровно `d` тиков занято ходьбой, и паузы между шагами нет.
    pub fn tick(&mut self) -> bool {
        let Some(step) = self.step else {
            return false;
        };
        if step.elapsed < step.ticks {
            self.step = Some(Step {
                elapsed: step.elapsed + 1,
                ..step
            });
            return false;
        }
        self.step = None;
        // Маршрут мог быть снят посреди шага (`clear_route`): цель шага живёт в
        // самом шаге, поэтому съедание маршрута от него не зависит.
        if !self.route.is_empty() {
            self.route.remove(0);
        }
        self.entered_from = Some(self.cell);
        self.cell = step.to;
        true
    }

    /// Прервать начатый шаг: существо остаётся в прежней клетке, маршрут не
    /// сдвигается. Так мир откатывает шаг, клетка назначения которого стала
    /// непроходима, пока существо шёл.
    pub fn cancel_step(&mut self) -> Option<Step> {
        self.step.take()
    }

    /// Снять цель, когда маршрут съеден целиком. `true` — существо дошло.
    pub fn finish(&mut self) -> bool {
        if self.step.is_none() && self.route.is_empty() && self.goal.is_some() {
            self.goal = None;
            return true;
        }
        false
    }

    /// Что нужно наружу, чтобы поставить спрайт в клетку и выбрать анимацию.
    pub fn state(&self) -> WalkState {
        WalkState {
            creature: self.creature,
            cell: self.cell,
            entered_from: self.entered_from,
            goal: self.goal,
            pixel: self.cell.centre(),
            direction: self.facing(),
            moving: self.step.is_some(),
            step_to: self.step.map(|step| step.to),
            step_ticks: self.step.map_or(0, |step| step.ticks),
            step_elapsed: self.step.map_or(0, |step| step.elapsed),
        }
    }

    /// Клетки, которые существо держит: под собой и зарезервированную шагом.
    pub fn held_cells(&self) -> Vec<Cell> {
        let mut cells = vec![self.cell];
        if let Some(step) = self.step {
            cells.push(step.to);
        }
        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creature(id: u32) -> CreatureId {
        CreatureId::new(id)
    }

    /// Маршрут из направлений — так же, как его строит поиск, но без поиска.
    fn route_from(from: Cell, directions: &[Direction]) -> Route {
        let mut cells = vec![from];
        let mut current = from;
        for &direction in directions {
            current = current.neighbour(direction).unwrap();
            cells.push(current);
        }
        Route::from_cells(&cells, current).unwrap()
    }

    #[test]
    fn step_last_exactly_the_given_number_of_ticks() {
        let from = Cell::new(4, 4);
        for ticks in [1u32, 2, 5, 7] {
            let mut walker = Walker::new(creature(1), from);
            walker.set_route(&route_from(from, &[Direction::East]));
            walker.start_step(ticks).unwrap();
            let mut finished_at = None;
            for tick in 0..ticks + 3 {
                if walker.tick() {
                    finished_at = Some(tick);
                    break;
                }
            }
            assert_eq!(finished_at, Some(ticks - 1), "шаг в {ticks} тиков");
            assert_eq!(walker.cell(), from.neighbour(Direction::East).unwrap());
        }
    }

    #[test]
    fn route_is_consumed_one_step_at_a_time() {
        let from = Cell::new(4, 4);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East, Direction::SouthEast]));
        let first = walker.next_cell().unwrap();
        walker.start_step(2).unwrap();
        assert_eq!(
            walker.next_cell(),
            Some(first),
            "маршрут сдвинулся раньше времени"
        );
        assert_eq!(walker.ticks_left_in_step(), 1);
        assert!(!walker.tick());
        assert!(walker.tick());
        assert_eq!(walker.cell(), first);
        assert_eq!(walker.route_len(), 1);
        assert_ne!(walker.next_cell(), Some(first));
        walker.start_step(1).unwrap();
        assert!(walker.tick());
        assert_eq!(walker.route_len(), 0);
        assert!(walker.finish());
        assert!(!walker.is_moving());
        assert_eq!(walker.goal(), None);
    }

    /// Направление меняется на границе клетки: середина шага его не трогает.
    #[test]
    fn direction_changes_only_at_the_cell_border() {
        let from = Cell::new(6, 6);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East, Direction::SouthEast]));
        walker.start_step(4).unwrap();
        for _ in 0..3 {
            assert!(!walker.tick());
            assert_eq!(walker.facing(), Some(Direction::East));
            assert_eq!(walker.cell(), from, "клетка сменилась до конца шага");
        }
        assert!(walker.tick());
        assert_eq!(walker.cell(), from.neighbour(Direction::East).unwrap());
        assert_eq!(
            walker.facing(),
            Some(Direction::SouthEast),
            "не повернулось к следующему шагу"
        );
        assert_eq!(walker.ticks_left_in_step(), 0);
    }

    #[test]
    fn walker_stops_at_the_last_cell() {
        let from = Cell::new(2, 2);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East]));
        walker.start_step(1).unwrap();
        assert!(walker.tick());
        assert_eq!(
            walker.facing(),
            Some(Direction::East),
            "поворот остался после прихода"
        );
        assert_eq!(walker.entered_from(), Some(from));
        assert_eq!(walker.held_cells(), vec![walker.cell()], "резерв не снят");
        assert!(!walker.is_moving());
        assert!(walker.next_cell().is_none());
        assert!(walker.start_step(3).is_none());
        assert!(!walker.tick(), "пустой тик после остановки");
        assert!(walker.finish());
        assert!(!walker.finish());
    }

    #[test]
    fn route_is_not_replaced_mid_step() {
        let from = Cell::new(2, 2);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East; 2]));
        walker.start_step(3).unwrap();
        let other = route_from(
            from.neighbour(Direction::East).unwrap(),
            &[Direction::North],
        );
        assert!(!walker.set_route(&other), "маршрут заменён посреди шага");
        assert!(!walker.is_waiting());
        assert_eq!(walker.facing(), Some(Direction::East));
    }

    #[test]
    fn state_reports_enough_for_a_sprite() {
        let from = Cell::new(3, 4);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::SouthWest]));
        walker.start_step(5).unwrap();
        walker.tick();
        let state = walker.state();
        assert_eq!(state.cell, from);
        assert_eq!(state.pixel, from.centre());
        assert_eq!(state.direction, Some(Direction::SouthWest));
        assert!(state.moving);
        assert_eq!(state.step_to, from.neighbour(Direction::SouthWest));
        assert_eq!((state.step_ticks, state.step_elapsed), (5, 2));
        assert_eq!(state.ticks_left_in_step(), 3);
        assert_eq!(state.goal, from.neighbour(Direction::SouthWest));
        assert_eq!(state.entered_from, None);
    }

    #[test]
    fn zero_tick_step_is_one_tick() {
        let from = Cell::new(1, 1);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::West]));
        walker.start_step(0).unwrap();
        assert!(walker.tick());
        assert_eq!(walker.cell(), from.neighbour(Direction::West).unwrap());
    }

    #[test]
    fn waiting_needs_a_route_and_no_step() {
        let from = Cell::new(5, 5);
        let mut walker = Walker::new(creature(1), from);
        assert!(!walker.is_waiting());
        walker.set_route(&route_from(from, &[Direction::North; 2]));
        assert!(
            walker.is_waiting(),
            "маршрут есть, шага нет — существо ждёт"
        );
        walker.start_step(2).unwrap();
        assert!(!walker.is_waiting(), "во время шага существо не ждёт");
    }
}
