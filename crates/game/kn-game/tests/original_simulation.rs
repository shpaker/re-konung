//! Confirmed rules over a real K1 map for hundreds of ticks, headless
//! (skipped when `original/` is absent — docs/k1/editions.md).
//!
//! Criterion 9 of #36: dozens of creatures of the shipped data moving in one
//! world at once. The world is fed the real foot data as walkability, the
//! real walk-animation frame counts as step durations, the real records as
//! the roster, and the load-time re-placement (criterion 2) as starting
//! cells; `k1_game::rules` decide every tick exactly as the façade will
//! later. What the run must never violate: nobody stands in an impassable
//! cell, no two creatures stand in one cell, and a blocked step recomputes
//! its route. Map 17 carries the calm answer too: its roaming squads stand
//! while no player walks their spawn areas, so not one of its twenty-eight
//! monsters moves.

use std::collections::HashSet;
use std::path::PathBuf;

use k1_data::{
    game_state::{self, GameState},
    map::{self, KonungMap},
    repo::{GAME_FILES, GameDir},
};
use k1_game::{creatures, placement, resources::Resources, rules};
use kn_game::{
    movement::{
        Cell, CreatureId, DIRECTION_COUNT, Grid, Passage, Rng,
        world::{Event, World, WorldRules},
    },
    rules::{FailedOrderCooldown, live},
    step_durations::StepDurations,
};

/// How many ticks each map runs: "several hundred" of the criterion.
const TICKS: u64 = 400;
/// The tick the blocked-step scenario arms on, once the crowd is moving.
const ARM_TICK: u64 = 50;

fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            if game.root().is_dir() {
                Some((edition, game))
            } else {
                eprintln!("skipping: no original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} exists but holds no edition with a game/ folder: the tests on data would \
         not have run. How to unpack one — docs/k1/editions.md",
        root.display()
    );
    dirs
}

fn state(game: &GameDir, hero: u32) -> GameState {
    let data = game.state_bytes(hero, GAME_FILES).unwrap();
    game_state::parse(&data).unwrap()
}

fn parse_map(game: &GameDir, number: u32) -> KonungMap {
    let data = game.map_bytes(number, GAME_FILES).unwrap();
    map::parse(&data).unwrap()
}

/// The map's foot data as walkability — the first real [`Passage`] supplier
/// in the repository; the blocked set is open so the scenario can close one
/// cell and watch the engine repath. It is the rules' terrain `Ground` too:
/// occupancy belongs to the world and to `place_roaming`'s own bookkeeping,
/// not to the foot.
struct Foot {
    blocked: HashSet<Cell>,
}

impl Foot {
    fn of(map: &KonungMap) -> Foot {
        let mut blocked = HashSet::new();
        for (row, bytes) in map.foot.iter().enumerate() {
            for (column, &foot) in bytes.iter().enumerate() {
                if map::is_blocked(foot) {
                    blocked.insert(Cell::new(column as u32, row as u32));
                }
            }
        }
        Foot { blocked }
    }

    fn contains(&self, cell: (i16, i16)) -> Option<Cell> {
        let (Ok(column), Ok(row)) = (u32::try_from(cell.0), u32::try_from(cell.1)) else {
            return None;
        };
        let cell = Cell::new(column, row);
        (column < map::FINE_WIDTH as u32 && row < map::FINE_HEIGHT as u32).then_some(cell)
    }
}

impl Passage for Foot {
    fn is_walkable(&self, cell: Cell) -> bool {
        cell.column < map::FINE_WIDTH as u32
            && cell.row < map::FINE_HEIGHT as u32
            && !self.blocked.contains(&cell)
    }
}

impl rules::Ground for Foot {
    fn is_free(&self, cell: (i16, i16)) -> bool {
        self.contains(cell)
            .is_some_and(|cell| self.is_walkable(cell))
    }
}

/// The scenario's handle on one blocked step: the creature whose reserved
/// cell was closed, whether the engine cancelled that step, and whether the
/// route was then recomputed.
struct Blocked {
    creature: CreatureId,
    step_cancelled: bool,
}

/// A world over one real map with the rules loop wired to it.
struct Simulation {
    world: World<Foot, StepDurations>,
    /// Roster index ↔ engine id. A record sharing a cell with an earlier one
    /// — the shipped data does put two villagers in one cell (map 7 does) —
    /// gets no id: it is absent from the simulation, which `evaluate` is
    /// specified for (no live picture, no decision).
    ids: Vec<Option<CreatureId>>,
    jobs: Vec<creatures::Workplace>,
    cooldown: FailedOrderCooldown,
    random: Rng,
    /// Creatures that started at least one step.
    movers: HashSet<CreatureId>,
    /// The largest number of creatures mid-step in one tick.
    most_moving_at_once: usize,
    blocked: Option<Blocked>,
    recomputed: bool,
}

impl Simulation {
    fn new(
        edition: &'static str,
        map_number: u32,
        game: &GameDir,
        state: &GameState,
        seed: u64,
    ) -> Simulation {
        let roster = creatures::roster(state, map_number as usize, Some(0));
        assert!(
            !roster.is_empty(),
            "{edition}: map {map_number} carries no creatures"
        );
        let konung_map = parse_map(game, map_number);
        let ground = Foot::of(&konung_map);
        // The load-time cells: roaming squads re-placed on free cells
        // (criterion 2), everyone else at the record's cell.
        let placed = placement::place_roaming(&roster, &ground, &mut Rng::new(seed));
        assert!(
            placed.dropped.is_empty(),
            "{edition}: map {map_number} squads were trimmed at {:#?}",
            placed.dropped
        );

        let resources = Resources::load(game).unwrap();
        let durations = resources
            .with_graphics(|graph, objects| {
                creatures::step_durations(state, &roster, graph, objects)
            })
            .unwrap();

        // The pace table is indexed by engine id, and ids arrive in add
        // order — so who gets in is planned over the cells before the world
        // exists, and the rows travel in that same order. The plan mirrors
        // `World::add` (inside the grid, walkable, unshared); the adds below
        // assert the mirror holds.
        let mut rows: Vec<[u32; DIRECTION_COUNT]> = Vec::new();
        let mut entered: Vec<Option<(usize, Cell)>> = Vec::with_capacity(roster.len());
        let mut occupied: HashSet<Cell> = HashSet::new();
        for (index, _) in roster.iter().enumerate() {
            let cell = placed.cells[index];
            match ground.contains(cell) {
                Some(cell) if ground.is_walkable(cell) && occupied.insert(cell) => {
                    rows.push(durations[index]);
                    entered.push(Some((rows.len() - 1, cell)));
                }
                _ => entered.push(None),
            }
        }

        let world_rules = WorldRules::default();
        let mut world = World::new(
            Grid::fine(),
            ground,
            StepDurations::new(rows),
            world_rules,
            seed,
        );
        assert_eq!(world.grid().columns as usize, map::FINE_WIDTH);
        assert_eq!(world.grid().rows as usize, map::FINE_HEIGHT);
        let mut ids: Vec<Option<CreatureId>> = Vec::with_capacity(roster.len());
        for (index, entry) in entered.iter().enumerate() {
            match entry {
                Some((row, cell)) => {
                    let id = world.add(*cell).unwrap_or_else(|error| {
                        panic!(
                            "plan and world disagree on the cell of roster \
                             record {}: {error:?}",
                            roster[index].id
                        )
                    });
                    assert_eq!(id, CreatureId::new(*row as u32));
                    ids.push(Some(id));
                }
                None => ids.push(None),
            }
        }

        Simulation {
            world,
            ids,
            jobs: creatures::job_slots(state, map_number as usize),
            cooldown: FailedOrderCooldown::new(world_rules.repath_cooldown_ticks),
            random: Rng::new(seed ^ 0xa5a5),
            movers: HashSet::new(),
            most_moving_at_once: 0,
            blocked: None,
            recomputed: false,
        }
    }

    /// The live picture indexed like the roster: records without an engine
    /// id report as absent, the rest through the engine's own view.
    fn picture(&self) -> Vec<Option<rules::Live>> {
        self.ids
            .iter()
            .map(|id| id.and_then(|id| live(&self.world, &[id]).pop()?))
            .collect()
    }

    /// One tick of the full loop plus the invariants of criterion 9.
    fn tick(&mut self, roster: &[creatures::Creature], tick: u64, edition: &str) {
        let picture = self.picture();
        let states = rules::group_states(roster, &picture);
        let decisions = {
            let mut context = rules::Context {
                ground: &self.world,
                random: &mut self.random,
                jobs: &self.jobs,
                states: &states,
            };
            rules::evaluate(roster, &picture, &mut context)
        };
        let commands: Vec<(CreatureId, rules::Decision)> = self
            .ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| Some(((*id)?, *decisions.get(index)?)))
            .collect();
        self.cooldown.apply(&mut self.world, &commands);

        let events = self.world.tick_step();
        for event in events {
            match event {
                Event::StepStarted { creature, to, .. } => {
                    self.movers.insert(creature);
                    // Arm once: close the cell the step reserved. Nobody
                    // stands there (a reservation is exclusive), so only the
                    // walking creature can lose it — and losing it must cost
                    // a cancelled step and a recomputed route.
                    if tick >= ARM_TICK && self.blocked.is_none() {
                        self.world.passage_mut().blocked.insert(to);
                        self.blocked = Some(Blocked {
                            creature,
                            step_cancelled: false,
                        });
                    }
                }
                Event::TerrainBlocked { creature, .. } => {
                    if let Some(blocked) = self.blocked.as_mut() {
                        blocked.step_cancelled |= blocked.creature == creature;
                    }
                }
                Event::RouteSet { creature, .. } | Event::NoRoute { creature, .. }
                    if self
                        .blocked
                        .as_ref()
                        .is_some_and(|b| b.creature == creature && b.step_cancelled) =>
                {
                    self.recomputed = true;
                }
                _ => {}
            }
        }

        // The invariants, checked after every tick.
        let mut cells: HashSet<Cell> = HashSet::new();
        let mut moving = 0usize;
        for id in self.ids.iter().copied().flatten() {
            let Some(state) = self.world.state(id) else {
                continue;
            };
            assert!(
                self.world.passage().is_walkable(state.cell),
                "{edition}: creature {} stands in an impassable cell {:?}",
                id.raw(),
                state.cell,
            );
            assert!(
                cells.insert(state.cell),
                "{edition}: creatures share the cell {:?}",
                state.cell,
            );
            if state.moving {
                moving += 1;
            }
        }
        self.most_moving_at_once = self.most_moving_at_once.max(moving);
    }

    fn run(&mut self, roster: &[creatures::Creature], edition: &str) {
        for tick in 0..TICKS {
            self.tick(roster, tick, edition);
        }
    }
}

/// Map 19 — nineteen villagers: the walk rolls keep several of them moving
/// at once while the workplace holders stand on their places.
#[test]
fn villagers_walk_a_real_map_for_hundreds_of_ticks() {
    const MAP: u32 = 19;
    for (edition, game) in game_dirs() {
        let state = state(&game, 0);
        let roster = creatures::roster(&state, MAP as usize, Some(0));
        let mut world = Simulation::new(edition, MAP, &game, &state, 0x36C0);
        world.run(&roster, edition);

        // Creatures free of a workplace must produce real movement, not a
        // statue garden: several movers in the run, and at least one tick
        // with several of them mid-step at once.
        assert!(
            world.movers.len() >= 2,
            "{edition}: only {} creature(s) of map {MAP} ever started a step",
            world.movers.len(),
        );
        assert!(
            world.most_moving_at_once >= 2,
            "{edition}: map {MAP} never had two creatures mid-step in the same tick",
        );
        assert!(
            world.recomputed,
            "{edition}: the blocked step on map {MAP} never saw its route recomputed",
        );
        println!(
            "{edition}: map {MAP} — {} creatures, {} moved, up to {} at once, \
             route recomputed after a blockage",
            roster.len(),
            world.movers.len(),
            world.most_moving_at_once,
        );
    }
}

/// Map 17 — six calm roaming squads, twenty-eight monsters: with no player
/// in any spawn area the recomputed group state is calm for every squad, and
/// over the whole run not one monster takes a step.
#[test]
fn calm_squads_stand_on_a_real_map_for_hundreds_of_ticks() {
    const MAP: u32 = 17;
    for (edition, game) in game_dirs() {
        let state = state(&game, 0);
        let roster = creatures::roster(&state, MAP as usize, Some(0));
        let roaming = roster.iter().filter(|creature| creature.roaming).count();
        assert!(
            roaming >= 20,
            "{edition}: map {MAP} was expected to carry its twenty-eight monsters, \
             found {roaming}"
        );
        let mut world = Simulation::new(edition, MAP, &game, &state, 0x1717);
        world.run(&roster, edition);
        assert!(
            world.movers.is_empty(),
            "{edition}: calm squads moved {} creature(s) on map {MAP}",
            world.movers.len(),
        );
        println!("{edition}: map {MAP} — {roaming} monsters stood their ground for {TICKS} ticks");
    }
}
