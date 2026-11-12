//! The rules and the engine in one loop: a decision becomes real movement.
//!
//! Criteria 3 and 4 of #36 end to end on a synthetic map. The decision tables
//! are `k1-game`'s own tests; what only a loop shows is that the order reaches
//! the world, the creature really walks it, and the squad state then changes
//! because of where it ended up — no decision-level test over `Game.N` can.

use std::collections::HashSet;

use k1_data::game_state::Rect;
use k1_game::creatures::Creature;
use k1_game::rules::{self, Context, Decision, GroupState};
use kn_game::movement::grid::{Cell, Grid};
use kn_game::movement::rng::Rng;
use kn_game::movement::world::{Event, World, WorldRules};
use kn_game::movement::{CreatureId, Direction, Pace, Passage};
use kn_game::rules::{Applied, FailedOrderCooldown, live};

/// Walkability as a set of blocked cells.
#[derive(Default)]
struct Map {
    blocked: HashSet<Cell>,
}

impl Passage for Map {
    fn is_walkable(&self, cell: Cell) -> bool {
        !self.blocked.contains(&cell)
    }
}

struct Fixed(u32);

impl Pace for Fixed {
    fn step_ticks(&self, _creature: CreatureId, _direction: Direction) -> u32 {
        self.0
    }
}

/// The spawn area the tests use: `(4, 4)..(8, 8)`, interior `5..=7`.
fn area() -> Rect {
    Rect {
        left: 4,
        top: 4,
        right: 8,
        bottom: 8,
    }
}

/// One member of a roaming squad standing at `cell`.
fn monster(cell: (i16, i16)) -> Creature {
    Creature {
        id: 1,
        cell,
        direction: 0,
        state: 0,
        class_id: 0x41,
        group: 7,
        group_kind: 0x41,
        roaming: true,
        spawn: area(),
        workplace: None,
        name_id: 0,
        nick_id: 0,
        sprite_set: 0,
    }
}

fn inside(area: &Rect, cell: (i16, i16)) -> bool {
    area.left <= cell.0 && cell.0 <= area.right && area.top <= cell.1 && cell.1 <= area.bottom
}

/// An open world. The engine's own same-goal throttle is off, so every order
/// that reaches the engine pays a search and the counts below mean searches.
fn open(blocked: HashSet<Cell>) -> World<Map, Fixed> {
    World::new(
        Grid::new(16, 16),
        Map { blocked },
        Fixed(2),
        WorldRules {
            repath_cooldown_ticks: 0,
            ..WorldRules::default()
        },
        3,
    )
}

/// A world with a ring one cell outside the spawn area: its interior stays
/// walkable and free, so the rules keep picking it, and no route reaches it.
fn walled() -> World<Map, Fixed> {
    let mut blocked = HashSet::new();
    for column in 3..=9 {
        blocked.insert(Cell::new(column, 3));
        blocked.insert(Cell::new(column, 9));
    }
    for row in 3..=9 {
        blocked.insert(Cell::new(3, row));
        blocked.insert(Cell::new(9, row));
    }
    open(blocked)
}

/// One tick of a rules loop: read the world, evaluate the roster, apply the
/// decisions, step.
fn tick(
    world: &mut World<Map, Fixed>,
    roster: &[Creature],
    ids: &[CreatureId],
    random: &mut Rng,
    cooldown: &mut FailedOrderCooldown,
) -> (Vec<Decision>, Vec<Applied>, Vec<Event>) {
    let picture = live(world, ids);
    let states = rules::group_states(roster, &picture);
    let decisions = {
        let mut context = Context {
            ground: &*world,
            random,
            jobs: &[],
            states: &states,
        };
        rules::evaluate(roster, &picture, &mut context)
    };
    let commands: Vec<(CreatureId, Decision)> =
        ids.iter().copied().zip(decisions.iter().copied()).collect();
    let applied = cooldown.apply(world, &commands);
    let events = world.tick_step();
    (decisions, applied, events)
}

/// The full cycle of criterion 4: the strayed member is seen outside its area,
/// the order really walks it home, and once it stands inside, the squad reads
/// calm and nobody is sent anywhere again.
#[test]
fn a_strayed_monster_walks_home_and_the_squad_goes_calm() {
    let mut world = open(HashSet::new());
    let roster = [monster((12, 12))];
    let ids = [world.add(Cell::new(12, 12)).unwrap()];
    let mut random = Rng::new(11);
    let mut cooldown = FailedOrderCooldown::new(WorldRules::default().repath_cooldown_ticks);
    let area = area();

    let mut ordered = false;
    let mut went_home = false;
    for _ in 0..200 {
        let (decisions, applied, _) = tick(&mut world, &roster, &ids, &mut random, &mut cooldown);
        for (decision, result) in decisions.iter().zip(&applied) {
            if matches!(decision, Decision::MoveTo(..)) {
                assert!(
                    matches!(result, Applied::Accepted { route_steps } if *route_steps > 0),
                    "the way home must exist: {decision:?} gave {result:?}"
                );
                ordered = true;
            }
        }

        let cell = world.cell_of(ids[0]).unwrap();
        let states = rules::group_states(&roster, &live(&world, &ids));
        if inside(&area, (cell.column as i16, cell.row as i16)) {
            assert_eq!(states[0], GroupState::Calm, "back home, the squad is calm");
            assert!(
                decisions.iter().all(|decision| *decision == Decision::None),
                "a calm squad stands: {decisions:?}"
            );
            went_home = true;
            break;
        }
        assert_eq!(states[0], GroupState::MemberOutside, "still strayed");
    }
    assert!(ordered, "the strayed member must be sent home");
    assert!(
        went_home,
        "the walk must actually end inside the spawn area"
    );
}

/// A spawn area that cannot be reached: the rules re-roll a target every tick,
/// so without a bound every tick costs a route search. With the window it is
/// one search per window, and the creature stays where it stands.
#[test]
fn an_unreachable_area_costs_one_search_per_window() {
    const WINDOW: u32 = 10;
    const TICKS: usize = 40;
    let start = Cell::new(13, 13);
    let roster = [monster((13, 13))];

    let mut searches = [0usize; 2];
    // A window of zero holds nothing, so the first run is the unbounded one.
    for (run, window) in searches.iter_mut().zip([None, Some(WINDOW)]) {
        let mut world = walled();
        let ids = [world.add(start).unwrap()];
        let mut random = Rng::new(11);
        let mut cooldown = FailedOrderCooldown::new(window.unwrap_or(0));
        for _ in 0..TICKS {
            let (_, _, events) = tick(&mut world, &roster, &ids, &mut random, &mut cooldown);
            *run += events
                .iter()
                .filter(|event| matches!(event, Event::NoRoute { .. }))
                .count();
        }
        assert_eq!(
            world.cell_of(ids[0]),
            Some(start),
            "an unreachable area moves nobody"
        );
        assert!(
            world.passage().is_walkable(start),
            "the creature must not stand in a blocked cell"
        );
    }
    let [unthrottled, throttled] = searches;
    assert_eq!(
        unthrottled, TICKS,
        "without the window every tick re-rolls and pays a search"
    );
    assert_eq!(
        throttled,
        TICKS / WINDOW as usize,
        "with the window one search per window is the bound"
    );
}
