//! Measurement of a rules tick over the real rosters (issue #36, criterion 10).
//!
//! An `#[ignore]`d test: it measures in release and prints a table, so it does
//! not get in the way of an ordinary `cargo test`. The numbers go to `docs/rust.md`.
//!
//! ```bash
//! cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
//!     --test rules_cost -- --ignored --nocapture
//! ```

use std::collections::HashSet;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use k1_data::{
    game_state::{self, GameState},
    map::{self, KonungMap},
    repo::{GAME_FILES, GameDir},
};
use k1_game::{creatures, placement, resources::Resources, rules};
use kn_game::{
    movement::{
        Cell, CreatureId, DIRECTION_COUNT, Grid, Passage, Rng,
        world::{World, WorldRules},
    },
    rules::{FailedOrderCooldown, live},
    step_durations::StepDurations,
};

/// Ticks measured after the warm-up, per scenario.
const TICKS: u32 = 300;

fn game_dir(edition: &str) -> Option<GameDir> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let game = GameDir::new(root.join(edition).join("game"));
    game.root().is_dir().then_some(game)
}

fn state(game: &GameDir, hero: u32) -> GameState {
    let data = game.state_bytes(hero, GAME_FILES).unwrap();
    game_state::parse(&data).unwrap()
}

fn parse_map(game: &GameDir, number: u32) -> KonungMap {
    let data = game.map_bytes(number, GAME_FILES).unwrap();
    map::parse(&data).unwrap()
}

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
        (column < map::FINE_WIDTH as u32 && row < map::FINE_HEIGHT as u32)
            .then_some(Cell::new(column, row))
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

fn report(label: &str, iterations: u32, total: Duration) {
    let per = total.as_secs_f64() * 1e6 / f64::from(iterations);
    println!("{label:<52} {per:>9.2} us/tick");
}

/// One roster on a real map: the world mid-run, the rules' inputs, and the
/// pieces of a tick, timed apart.
struct Crowd {
    world: World<Foot, StepDurations>,
    roster: Vec<creatures::Creature>,
    jobs: Vec<creatures::Workplace>,
    ids: Vec<Option<CreatureId>>,
    cooldown: FailedOrderCooldown,
    random: Rng,
}

impl Crowd {
    fn new(game: &GameDir, state: &GameState, map_number: u32, seed: u64) -> Crowd {
        let roster = creatures::roster(state, map_number as usize, Some(0));
        let konung_map = parse_map(game, map_number);
        let ground = Foot::of(&konung_map);
        let placed = placement::place_roaming(&roster, &ground, &mut Rng::new(seed));
        let resources = Resources::load(game).unwrap();
        let durations = resources
            .with_graphics(|graph, objects| {
                creatures::step_durations(state, &roster, graph, objects)
            })
            .unwrap();
        let mut rows: Vec<[u32; DIRECTION_COUNT]> = Vec::new();
        let mut plan: Vec<Option<Cell>> = Vec::new();
        let mut occupied: HashSet<Cell> = HashSet::new();
        for (index, _) in roster.iter().enumerate() {
            match ground.contains(placed.cells[index]) {
                Some(cell) if ground.is_walkable(cell) && occupied.insert(cell) => {
                    rows.push(durations[index]);
                    plan.push(Some(cell));
                }
                _ => plan.push(None),
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
        let ids = plan
            .iter()
            .map(|cell| cell.and_then(|cell| world.add(cell).ok()))
            .collect();
        Crowd {
            world,
            roster,
            jobs: creatures::job_slots(state, map_number as usize),
            ids,
            cooldown: FailedOrderCooldown::new(world_rules.repath_cooldown_ticks),
            random: Rng::new(seed ^ 0xa5a5),
        }
    }

    /// The rules phases of one tick: the picture, the squad states and the
    /// decisions — without the world call or the step.
    fn decide(&mut self) -> Vec<(CreatureId, rules::Decision)> {
        let picture: Vec<_> = self
            .ids
            .iter()
            .map(|id| id.and_then(|id| live(&self.world, &[id]).pop()?))
            .collect();
        let states = rules::group_states(&self.roster, &picture);
        let mut context = rules::Context {
            ground: &self.world,
            random: &mut self.random,
            jobs: &self.jobs,
            states: &states,
        };
        let decisions = rules::evaluate(&self.roster, &picture, &mut context);
        self.ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| Some(((*id)?, decisions.get(index).copied()?)))
            .collect()
    }

    /// A whole tick of the loop: decide, apply, step.
    fn tick(&mut self) {
        let commands = self.decide();
        self.cooldown.apply(&mut self.world, &commands);
        black_box(self.world.tick_step());
    }

    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick();
        }
    }
}

#[test]
#[ignore = "cost measurement: release only, and only when run explicitly"]
fn measures_a_rules_tick_over_the_real_rosters() {
    // Map 19 walks (villagers roll their walks and walk them), map 17 stands
    // (calm squads) — the tick is measured mid-motion and at rest, and the
    // load-time placement is measured for the whole shipped roster.
    for edition in ["rus", "eng"] {
        let Some(game) = game_dir(edition) else {
            eprintln!("skipping {edition}: no original/k1/{edition}/game");
            continue;
        };
        let state = state(&game, 0);
        for map_number in [17, 19] {
            let mut crowd = Crowd::new(&game, &state, map_number, 0xC057);
            // Warm-up: the crowd into motion and the caches warm.
            crowd.run(100);
            let moving = crowd
                .world
                .states()
                .iter()
                .filter(|state| state.moving)
                .count();

            let start = Instant::now();
            for _ in 0..TICKS {
                black_box(crowd.decide());
            }
            report(
                &format!(
                    "{edition} map {map_number}: decide ({} creatures, {moving} walking)",
                    crowd.ids.len()
                ),
                TICKS,
                start.elapsed(),
            );

            let mut crowd = {
                let mut crowd = Crowd::new(&game, &state, map_number, 0xC057);
                crowd.run(100);
                crowd
            };
            let start = Instant::now();
            for _ in 0..TICKS {
                crowd.tick();
            }
            report(
                &format!("{edition} map {map_number}: full tick (rules + apply + step)",),
                TICKS,
                start.elapsed(),
            );
        }

        // Load-time placement for every creature of every map at once — the
        // whole of criterion 2, paid once per game load.
        let mut rosters = 0usize;
        let start = Instant::now();
        for number in game.map_numbers(GAME_FILES).unwrap() {
            let map = parse_map(&game, number);
            let ground = Foot::of(&map);
            let placed =
                placement::place_roaming(&roster(&state, number), &ground, &mut Rng::new(1));
            rosters += placed.cells.len();
            black_box(placed);
        }
        let maps = game.map_numbers(GAME_FILES).unwrap().len() as u32;
        let per = start.elapsed().as_secs_f64() * 1e6 / f64::from(maps);
        println!(
            "{edition:<6} placement of all maps ({rosters} creatures)  {:>13.2} us/map",
            per
        );
    }
}

fn roster(state: &GameState, map_number: u32) -> Vec<creatures::Creature> {
    creatures::roster(state, map_number as usize, Some(0))
}
