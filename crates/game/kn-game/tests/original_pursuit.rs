//! Issue #79 scenarios use owned maps and the same commands as the viewer.
use std::{collections::HashSet, path::PathBuf};

use k1_data::{
    map,
    repo::{GAME_FILES, GameDir},
};
use kn_game::{
    Game, MapRef, MapSource,
    debug::{DebugCommand, DebugJournal},
    movement::{Cell, Direction},
    simulation::{CreatureStage, PursuitStatus},
};

#[test]
fn both_editions_acquire_pursue_stop_and_replay_without_attack_frames() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let editions: Vec<_> = ["rus", "eng"]
        .into_iter()
        .map(|edition| (edition, root.join(edition).join("game")))
        .filter(|(_, directory)| directory.is_dir())
        .collect();
    assert!(
        !root.exists() || !editions.is_empty(),
        "original/k1 exists without an edition"
    );
    if editions.is_empty() {
        eprintln!("skip: no user-owned K1 editions");
    }
    for (edition, directory) in editions {
        let game = Game::open(&directory).unwrap();
        let map_ref = MapRef {
            source: MapSource::Game,
            number: 17,
        };
        let terrain =
            map::parse(&GameDir::new(&directory).map_bytes(17, GAME_FILES).unwrap()).unwrap();
        let profile = game.debug_player_profile(map_ref, None).unwrap();
        let fresh = || {
            let mut simulation = game
                .simulation(map_ref, 42)
                .map_err(|error| error.to_string())?;
            simulation.configure_melee_frames(vec![]);
            Ok(simulation)
        };
        // Source rules and the fixed scenarios: creatures/combat.md, hostile squad research.
        for (group, placement, arrival_tick) in [(162, (44, 48), 114), (163, (64, 114), 126)] {
            let trace = || {
                let mut simulation = fresh().unwrap();
                let initial = simulation.debug_snapshot();
                for _ in 0..40 {
                    simulation.tick();
                }
                let calm = simulation.debug_snapshot();
                for creature in calm
                    .creatures
                    .iter()
                    .filter(|creature| creature.group == group)
                {
                    assert_eq!(creature.used_group_state, 0);
                    assert!(creature.pursuit.is_none());
                    assert_eq!(
                        creature.movement.as_ref().unwrap().cell,
                        initial
                            .creatures
                            .iter()
                            .find(|initial| initial.record_id == creature.record_id)
                            .unwrap()
                            .movement
                            .as_ref()
                            .unwrap()
                            .cell
                    );
                }
                let mut journal = DebugJournal::default();
                journal
                    .dispatch(
                        &mut simulation,
                        Some(&profile),
                        DebugCommand::Place { cell: placement },
                        fresh,
                    )
                    .result
                    .unwrap();
                let runtime_id = simulation.debug_player().unwrap().id;
                let mut trace = Vec::new();
                let mut first_acquisition = None;
                let mut first_arrival = None;
                for _ in 0..600 {
                    let events = simulation.tick();
                    let snapshot = simulation.debug_snapshot();
                    let mut occupied = HashSet::new();
                    for movement in snapshot
                        .creatures
                        .iter()
                        .filter_map(|creature| creature.movement.as_ref())
                        .chain(snapshot.player.iter().map(|player| &player.movement))
                    {
                        let (column, row) = movement.cell;
                        assert!(!map::is_blocked(
                            terrain.foot[row as usize][column as usize]
                        ));
                        assert!(
                            occupied.insert(movement.cell),
                            "two actors occupy {:?}",
                            movement.cell
                        );
                        if let Some((column, row)) = movement.step_to {
                            assert!(!map::is_blocked(
                                terrain.foot[row as usize][column as usize]
                            ));
                        }
                    }
                    for creature in snapshot
                        .creatures
                        .iter()
                        .filter(|creature| creature.group == group)
                    {
                        assert_eq!(creature.used_group_state, 1);
                        assert!(!matches!(
                            simulation.creature_stage(creature.record_id),
                            Some(CreatureStage::Melee { .. })
                        ));
                        if let Some(pursuit) = creature.pursuit {
                            assert_eq!(pursuit.target_id, runtime_id);
                            first_acquisition.get_or_insert(snapshot.tick);
                            if pursuit.status == PursuitStatus::InRange {
                                first_arrival.get_or_insert(snapshot.tick);
                                let (column, row) = creature.movement.as_ref().unwrap().cell;
                                assert!(
                                    Direction::between(
                                        Cell::new(column, row),
                                        Cell::new(placement.0, placement.1)
                                    )
                                    .is_some()
                                );
                                assert!(!creature.movement.as_ref().unwrap().moving);
                            }
                        }
                    }
                    trace.push((events, snapshot));
                }
                assert_eq!(
                    first_acquisition,
                    Some(41),
                    "{edition} group {group} acquisition"
                );
                assert_eq!(
                    first_arrival,
                    Some(arrival_tick),
                    "{edition} group {group} arrival"
                );
                assert!(simulation.order_failures().is_empty());
                let final_snapshot = simulation.debug_snapshot();
                journal.observe_tick(simulation.tick_number());
                while !journal
                    .seek(&mut simulation, Some(&profile), 40, 64, fresh)
                    .unwrap()
                    .caught_up
                {}
                while !journal
                    .seek(
                        &mut simulation,
                        Some(&profile),
                        final_snapshot.tick,
                        64,
                        fresh,
                    )
                    .unwrap()
                    .caught_up
                {}
                assert_eq!(
                    simulation.debug_snapshot(),
                    final_snapshot,
                    "backward replay must restore pursuits"
                );
                eprintln!(
                    "{edition} map17 seed42 group {group} placement {placement:?}: acquired {first_acquisition:?}, in range {first_arrival:?}"
                );
                trace
            };
            assert_eq!(
                trace(),
                trace(),
                "same seed and commands must reproduce every event and snapshot"
            );
        }
    }
}

#[test]
#[ignore = "explicit release measurement on user-owned data"]
fn pursuit_tick_cost() {
    use std::{hint::black_box, time::Instant};
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1/eng/game");
    let game = Game::open(directory).expect("measurement requires the owned ENG edition");
    let map_ref = MapRef {
        source: MapSource::Game,
        number: 17,
    };
    let profile = game.debug_player_profile(map_ref, None).unwrap();
    let fresh = || {
        game.simulation(map_ref, 42)
            .map_err(|error| error.to_string())
    };
    let mut quiet = fresh().unwrap();
    let mut pursuit = fresh().unwrap();
    for _ in 0..40 {
        quiet.tick();
        pursuit.tick();
    }
    pursuit
        .apply_debug_command(
            &DebugCommand::Place { cell: (44, 48) },
            Some(&profile),
            fresh,
        )
        .unwrap();
    for (label, simulation) in [
        ("calm", &mut quiet),
        ("pursuit_then_in_range", &mut pursuit),
    ] {
        let started = Instant::now();
        for _ in 0..10000 {
            black_box(simulation.tick());
        }
        eprintln!(
            "ENG map17 seed42 {label}: {} creatures, {:.2} us/tick (10000 ticks)",
            simulation.creature_count(),
            started.elapsed().as_secs_f64() * 100.0
        );
    }
    let mut elapsed = 0.0;
    for _ in 0..100 {
        let mut simulation = fresh().unwrap();
        for _ in 0..40 {
            simulation.tick();
        }
        simulation
            .apply_debug_command(
                &DebugCommand::Place { cell: (44, 48) },
                Some(&profile),
                fresh,
            )
            .unwrap();
        let started = Instant::now();
        black_box(simulation.tick());
        elapsed += started.elapsed().as_secs_f64();
    }
    eprintln!(
        "ENG map17 seed42 first pursuit including A*: {:.2} us/tick (100 fresh scenes)",
        elapsed * 10000.0
    );
}
