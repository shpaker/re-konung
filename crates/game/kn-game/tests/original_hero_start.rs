//! The hero placement contracts of the new-game flow (#121), headless, over
//! both editions' data.
//!
//! Placement: each hero's player squad starts inside its own start map
//! (`start_map`: 0→1, 1→4, 2→7) on walkable cells, and the engine actually
//! stands every member there.
//!
//! World-unchanged: two simulations built from the SAME state bytes, map and
//! seed, differing only in whether the player squad joins the roster, stand
//! every NPC at the same load position — and the marked configuration, the
//! one every real session runs, replays deterministically (M1's criteria).
//! The facade's `None` cannot express this comparison (it resolves to the
//! first hero, a different `Game.N`), so the tests build `Simulation::new`
//! directly: the state is the caller's choice, the `hero: Option<usize>`
//! parameter only decides the squad. The decision-layer half of the
//! contract — the squad earns no decision and changes nobody else's — lives
//! in k1-game's `original_rules.rs`, where the ground can be held identical
//! across the two variants; in a full world the squad's occupied cells
//! legitimately shift the shared decision RNG (docs/rust.md, "The marked
//! hero's footprint"), so whole-world counter identity is not the pin.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use k1_data::{
    game_state,
    map::{self, KonungMap},
    repo::{GAME_FILES, GameDir},
};
use k1_game::creatures;
use k1_game::resources::Resources;
use kn_game::movement::{Cell, Grid, Passage};
use kn_game::simulation::{MapPassage, Simulation};
use kn_game::{Game, MapSource};

/// The same fixed seed the viewer replays with and the player window starts a
/// session with — one reproducible world everywhere.
const SEED: u64 = 42;
/// Long enough for village arrivals to accumulate: the viewer's pins on map 1
/// hold at 5000 ticks of this seed.
const TICKS: u64 = 5000;

fn game_roots() -> Vec<(&'static str, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = root.join(edition).join("game");
            if game.is_dir() {
                Some((edition, game))
            } else {
                eprintln!("skip: no original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} exists but holds no edition with a game/ folder: the data-backed \
         test could not run. How to unpack — docs/k1/editions.md",
        root.display()
    );
    dirs
}

/// One hero's start map, loaded once and shared by both comparison variants:
/// the map and the state bytes are fixed, `hero` below decides only whether
/// the player squad joins the roster.
struct Start {
    map: KonungMap,
    passage: MapPassage,
    state: game_state::GameState,
    resources: Resources,
    map_number: usize,
    hero: u32,
    squad: Squad,
}

/// The player squad of a start state: the original's start point (the spawn
/// rectangle's top-left corner, docs/k1/formats/game-state.md, group 0x20),
/// and per-member record numbers and record cells — what a non-roaming group
/// stands on.
struct Squad {
    ids: Vec<usize>,
    cells: Vec<(i16, i16)>,
    spawn_corner: (i16, i16),
}

impl Start {
    fn load(root: &Path, hero: u32) -> Start {
        let dir = GameDir::new(root);
        let map_number = game_state::start_map(hero as usize);
        let map_bytes = dir.map_bytes(map_number as u32, GAME_FILES).unwrap();
        let map = map::parse(&map_bytes).unwrap();
        let state_bytes = dir.state_bytes(hero, GAME_FILES).unwrap();
        let state = game_state::parse(&state_bytes).unwrap();
        let entities = game_state::on_map(&state, map_number, Some(hero as usize));
        let start = entities
            .start
            .unwrap_or_else(|| panic!("hero {hero}: no player group on its start map"));
        let mut squad = Squad {
            ids: Vec::new(),
            cells: Vec::new(),
            spawn_corner: (start.spawn.top, start.spawn.left),
        };
        for member in start.members() {
            let character = state.characters[member]
                .as_ref()
                .unwrap_or_else(|| panic!("hero {hero}: squad member {member} has no record"));
            squad.ids.push(character.slot);
            squad.cells.push((character.x, character.y));
        }
        assert!(
            !squad.ids.is_empty(),
            "hero {hero}: an empty player squad cannot start a game"
        );
        Start {
            passage: MapPassage::from_map(&map),
            map,
            state,
            resources: Resources::load(&dir).unwrap(),
            map_number,
            hero,
            squad,
        }
    }

    /// The start-map simulation: `marked` joins the player squad, `!marked`
    /// leaves it out. Everything else — map, state bytes, jobs, seed, texts —
    /// is identical by construction.
    fn simulation(&self, marked: bool) -> Simulation {
        let hero = marked.then_some(self.hero as usize);
        let roster = creatures::roster(&self.state, self.map_number, hero);
        let jobs = creatures::job_slots(&self.state, self.map_number);
        let (steps, specials) = self
            .resources
            .with_graphics(|graph, objects| {
                (
                    creatures::step_durations(&self.state, &roster, graph, objects),
                    creatures::special_frame_counts(&self.state, &roster, graph, objects),
                )
            })
            .unwrap();
        Simulation::new(
            &self.map,
            &self.state,
            self.map_number,
            hero,
            steps,
            specials,
            jobs,
            SEED,
            self.resources.tables.texts.as_ref(),
        )
    }

    fn grid(&self) -> Grid {
        Grid::fine()
    }
}

#[test]
fn the_hero_squad_starts_inside_the_map_on_walkable_cells() {
    let roots = game_roots();
    if roots.is_empty() {
        // original/ is absent: skip the data-backed run cleanly rather than
        // assert against missing data, matching the sibling original_* tests.
        return;
    }
    for (edition, root) in roots {
        let dir = GameDir::new(&root);
        for hero in dir.state_numbers(GAME_FILES).unwrap() {
            let start = Start::load(&root, hero);
            let grid = start.grid();
            // The original's start point first: the spawn rectangle's corner
            // (docs/k1/formats/game-state.md, "Confirmed" over every state
            // file of both editions).
            let (corner_x, corner_y) = start.squad.spawn_corner;
            let corner = Cell::new(corner_x.max(0) as u32, corner_y.max(0) as u32);
            assert!(
                grid.contains(corner),
                "{edition} hero {hero}: the start corner {corner_x},{corner_y} is outside the map"
            );
            assert!(
                start.passage.is_walkable(corner),
                "{edition} hero {hero}: the start corner {corner_x},{corner_y} is impassable"
            );
            // Then every squad member's record cell — the cells `world.add`
            // is asked for, since the player group is not re-placed at load.
            for (index, &(x, y)) in start.squad.cells.iter().enumerate() {
                let cell = Cell::new(x.max(0) as u32, y.max(0) as u32);
                assert!(
                    grid.contains(cell),
                    "{edition} hero {hero}: member {} cell {x},{y} is outside the map",
                    start.squad.ids[index]
                );
                assert!(
                    start.passage.is_walkable(cell),
                    "{edition} hero {hero}: member {} cell {x},{y} is impassable",
                    start.squad.ids[index]
                );
            }
            // The engine stands the squad: no load refusal or trim for the
            // squad's records (refusals of other records are documented data
            // facts, not this contract's business), every member actually in
            // the world, and at its record cell — a non-roaming group stands
            // where the file puts it. `states()`, not `initial_pixels()`: the
            // latter also holds refused records at their load cells.
            let sim = start.simulation(true);
            assert!(
                !sim.spawn_failures()
                    .iter()
                    .any(|(id, _)| start.squad.ids.contains(id)),
                "{edition} hero {hero}: the world refused a squad member: {:?}",
                sim.spawn_failures()
            );
            assert!(
                !sim.load_trimmed()
                    .iter()
                    .any(|id| start.squad.ids.contains(id)),
                "{edition} hero {hero}: the load trimmed a squad member: {:?}",
                sim.load_trimmed()
            );
            let standing: HashMap<usize, _> = sim
                .states()
                .into_iter()
                .map(|state| (state.id, state))
                .collect();
            for (index, &id) in start.squad.ids.iter().enumerate() {
                let state = standing.get(&id).unwrap_or_else(|| {
                    panic!("{edition} hero {hero}: squad member {id} does not stand in the world")
                });
                let (x, y) = start.squad.cells[index];
                assert_eq!(
                    state.cell,
                    (x.max(0) as u32, y.max(0) as u32),
                    "{edition} hero {hero}: member {id} stands off its record cell"
                );
            }
        }
    }
}

#[test]
fn npc_load_positions_are_identical_with_and_without_the_marked_hero() {
    let roots = game_roots();
    if roots.is_empty() {
        return;
    }
    for (edition, root) in roots {
        let dir = GameDir::new(&root);
        for hero in dir.state_numbers(GAME_FILES).unwrap() {
            let start = Start::load(&root, hero);
            let marked = start.simulation(true);
            let unmarked = start.simulation(false);
            let is_npc = |id: usize| !start.squad.ids.contains(&id);
            // The NPC roster and its placement are untouched: the same
            // positions for the same records, whether the squad joined or
            // not (`initial_pixels()` includes refused records, so this
            // covers composition and placement at once). The decision-layer
            // half of the world-unchanged contract lives in k1-game's
            // `original_rules.rs`: in a full world the squad's occupied
            // cells legitimately shift the villager walk draws (the walk
            // rule's occupancy test over the shared decision RNG — a
            // documented difference of ours, hero-agnostic), so counter
            // identity across a marked and an unmarked world is not the
            // contract any engine run can pin.
            let npc_pixels = |pixels: Vec<(usize, (i32, i32))>| -> Vec<(usize, (i32, i32))> {
                pixels.into_iter().filter(|&(id, _)| is_npc(id)).collect()
            };
            assert_eq!(
                npc_pixels(marked.initial_pixels()),
                npc_pixels(unmarked.initial_pixels()),
                "{edition} hero {hero}: NPC load positions moved because of the hero squad"
            );
            assert!(
                marked.creature_count() > unmarked.creature_count(),
                "{edition} hero {hero}: the marked simulation has no more creatures — \
                 the squad did not join the world"
            );
        }
    }
}

#[test]
fn a_marked_start_map_replays_deterministically() {
    // M1's criteria keep holding with the hero in the world (#88 AC5, the
    // world-unchanged half): the same seed over the shipped configuration —
    // the squad marked, as every real session builds it — reaps the same
    // counters.
    let roots = game_roots();
    if roots.is_empty() {
        return;
    }
    for (edition, root) in roots {
        let dir = GameDir::new(&root);
        let heroes = dir.state_numbers(GAME_FILES).unwrap();
        for hero in &heroes {
            let start = Start::load(&root, *hero);
            let mut first = start.simulation(true);
            let mut second = start.simulation(true);
            for _ in 0..TICKS {
                first.tick();
                second.tick();
            }
            assert_eq!(
                first.goal_reaches(),
                second.goal_reaches(),
                "{edition} hero {hero}: two runs of the same seed differ"
            );
        }
        // Non-vacuousness: map 1 (hero 0) is known to reap arrivals at this
        // seed — the viewer pins its workplace arrivals at 5000 ticks.
        let first = Start::load(&root, heroes[0]);
        let mut marked = first.simulation(true);
        for _ in 0..TICKS {
            marked.tick();
        }
        assert!(
            !marked.goal_reaches().is_empty(),
            "{edition}: no arrivals on the first hero's start map — the replay \
             comparison would hold vacuously"
        );
    }
}

/// The hero presentation of the original's new-game menu, resolvable from
/// the facade: names and descriptions resolve from the executable's tables
/// and are distinct, the portraits sit in the original's screen-box order
/// (Драгомир 3/4, Михаил 5/6, Волк 7/8 — selected/plain), and the menu's
/// numbers live in the occupation/specialization tables, pinned here (facts
/// from the original's code, `game_newgame.cpp:367-462` and
/// `game.cpp:2301-2327` — they are not file data).
#[test]
fn the_menu_can_present_every_hero() {
    let roots = game_roots();
    if roots.is_empty() {
        return;
    }
    for (edition, root) in roots {
        let game = Game::open(&root).unwrap();
        let summaries = game.hero_summaries(MapSource::Game).unwrap();
        let heroes = game.heroes(MapSource::Game).unwrap();
        assert_eq!(
            summaries.len(),
            heroes.len(),
            "{edition}: one summary per starting state"
        );
        for summary in &summaries {
            assert!(
                !summary.name.is_empty(),
                "{edition} hero {}: the hero's name-table entry resolves",
                summary.hero
            );
            assert!(
                !summary.description.is_empty(),
                "{edition} hero {}: the hero's description resolves",
                summary.hero
            );
            assert_eq!(summary.level, 1, "{edition} hero {}", summary.hero);
            assert_eq!(summary.gold, 100, "{edition} hero {}", summary.hero);
            assert_eq!(summary.health, 100, "{edition} hero {}", summary.hero);
        }
        let mut names: Vec<_> = summaries.iter().map(|s| s.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            summaries.len(),
            "{edition}: the heroes carry distinct names"
        );
        let mut descriptions: Vec<_> = summaries.iter().map(|s| s.description.clone()).collect();
        descriptions.sort();
        descriptions.dedup();
        assert_eq!(
            descriptions.len(),
            summaries.len(),
            "{edition}: the heroes carry distinct descriptions"
        );
        // The portraits sit in the original's screen-box order: hero 0 is box
        // 2 (records 7/8), hero 1 is box 0 (3/4), hero 2 is box 1 (5/6); the
        // first of each pair is the selected variant.
        let portraits: Vec<_> = summaries
            .iter()
            .map(|s| (s.portrait_selected, s.portrait))
            .collect();
        assert_eq!(portraits, vec![(7, 8), (3, 4), (5, 6)]);
    }
}

/// The occupation table of the new-game screen, exactly the original's
/// click-branch numbers (`game_newgame.cpp:367-413`) and the derived
/// carry/party numbers.
#[test]
fn the_occupation_table_matches_the_original() {
    let classes = kn_game::newgame_classes();
    assert_eq!(classes.len(), 4);
    // Warrior — the default the menu opens with.
    let warrior = &classes[0];
    assert_eq!(warrior.class_id, kn_game::CLASS_VOIN);
    assert_eq!(
        (
            warrior.agility,
            warrior.charisma,
            warrior.strength,
            warrior.endurance
        ),
        (3, 3, 15, 10)
    );
    assert_eq!(warrior.max_party, 1);
    assert!((warrior.carrying - 13.333).abs() < 0.01);
    // Hunter.
    let hunter = &classes[1];
    assert_eq!(hunter.class_id, kn_game::CLASS_OHOTNIK);
    assert_eq!(
        (
            hunter.agility,
            hunter.charisma,
            hunter.strength,
            hunter.endurance
        ),
        (15, 3, 3, 10)
    );
    // Merchant.
    let merchant = &classes[2];
    assert_eq!(merchant.class_id, kn_game::CLASS_KUPETC);
    assert_eq!(
        (
            merchant.agility,
            merchant.charisma,
            merchant.strength,
            merchant.endurance
        ),
        (3, 3, 10, 12)
    );
    assert!((merchant.carrying - 14.0).abs() < 0.01);
    // Chief: the only occupation with a charisma high enough to gather more
    // than one companion.
    let chief = &classes[3];
    assert_eq!(chief.class_id, kn_game::CLASS_VOJD);
    assert_eq!(
        (
            chief.agility,
            chief.charisma,
            chief.strength,
            chief.endurance
        ),
        (3, 25, 7, 7)
    );
    assert_eq!(chief.max_party, 3);
}

/// The combat styles and the secondary sets, exactly the original's
/// click-branch numbers (`game_newgame.cpp:420-462`).
#[test]
fn the_specialization_tables_match_the_original() {
    let (fencing, accuracy) = (kn_game::COMBAT_FENCING, kn_game::COMBAT_ACCURACY);
    assert_eq!(
        (
            fencing.fencing,
            fencing.accuracy,
            fencing.carpentry,
            fencing.smithery
        ),
        (100, 0, 1, 10)
    );
    assert_eq!(
        (
            accuracy.fencing,
            accuracy.accuracy,
            accuracy.carpentry,
            accuracy.smithery
        ),
        (0, 100, 2, 5)
    );
    let sets = [
        kn_game::SECONDARY_DEFAULT,
        kn_game::SECONDARY_TRADING,
        kn_game::SECONDARY_MEDICINE,
    ];
    assert_eq!(
        sets.iter()
            .map(|s| (s.trading, s.medicine, s.identification))
            .collect::<Vec<_>>(),
        vec![(10, 50, 20), (20, 50, 10), (10, 100, 10)]
    );
}

/// The model preview of a hero composes from `GRAPH.RES`: a non-empty RGBA
/// image, and different heroes differ — their starting equipment is
/// assigned per hero in the original's code.
#[test]
fn the_hero_preview_composes_from_graph_res() {
    let roots = game_roots();
    if roots.is_empty() {
        return;
    }
    for (edition, root) in roots {
        let game = Game::open(&root).unwrap();
        let heroes = game.heroes(MapSource::Game).unwrap();
        let mut previews = Vec::new();
        for hero in &heroes {
            let preview = game
                .hero_preview(*hero)
                .unwrap_or_else(|error| panic!("{edition} hero {hero}: {error}"));
            assert!(
                preview.width > 0 && preview.height > 0,
                "{edition} hero {hero}: the preview has a size"
            );
            assert!(
                preview.rgba.chunks(4).any(|px| px[3] > 0),
                "{edition} hero {hero}: the preview is not empty"
            );
            previews.push(preview);
        }
        let same = previews.windows(2).all(|pair| pair[0].rgba == pair[1].rgba);
        assert!(
            !same,
            "{edition}: different heroes compose to the same model — the starting              equipment is per hero"
        );
    }
}
