//! Local editions exercise the complete facade, resources, pursuit and animation clock.
use std::path::PathBuf;

use kn_game::{
    Game, session::AttackStatus, session::CreatureStage, session::HeroOrderStatus,
    session::Simulation,
};
use kn_isometric::{Cell, Direction};

fn editions() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let editions: Vec<_> = ["rus", "eng"]
        .into_iter()
        .map(|edition| root.join(edition).join("game"))
        .filter(|path| path.is_dir())
        .collect();
    assert!(
        !root.exists() || !editions.is_empty(),
        "original/k1 exists without an edition"
    );
    if editions.is_empty() {
        eprintln!("skip: no user-owned K1 editions");
    }
    editions
}

fn encounter(game: &Game) -> Simulation {
    let map = game.start_map(0);
    let initial = game.simulation_with_hero(map, 7, Some(0)).unwrap();
    let hero = initial
        .states()
        .into_iter()
        .find(|state| Some(state.id) == initial.hero_id())
        .unwrap();
    let mut targets: Vec<_> = initial
        .states()
        .into_iter()
        .filter(|state| initial.can_hero_attack(state.id))
        .collect();
    targets.sort_by_key(|state| {
        (
            (state.pixel.0 - hero.pixel.0).unsigned_abs()
                + (state.pixel.1 - hero.pixel.1).unsigned_abs(),
            state.id,
        )
    });
    for target in targets {
        let mut simulation = game.simulation_with_hero(map, 7, Some(0)).unwrap();
        simulation.order_hero_attack(target.id).unwrap();
        for _ in 0..4000 {
            simulation.tick();
            match simulation.hero_attack_order().map(|order| order.status) {
                Some(AttackStatus::Attacking) => return simulation,
                Some(AttackStatus::Refused(_)) => break,
                _ => {}
            }
        }
    }
    panic!("no reachable melee target on starting map");
}

#[test]
fn both_editions_approach_repeat_switch_target_and_leave() {
    for directory in editions() {
        let mut game = Game::open(&directory).unwrap();
        game.configure_new_game(0, 0, 0, 0, None).unwrap();
        let mut simulation = encounter(&game);
        let hero = simulation.hero_id().unwrap();
        let first = simulation.hero_attack_order().unwrap().target_id;
        let mut starts = 0;
        for _ in 0..80 {
            if simulation.creature_stage(hero) == Some(CreatureStage::Melee { frame: 0 }) {
                starts += 1;
            }
            simulation.tick();
        }
        assert!(starts >= 3, "complete repeat cycles");
        assert!(
            simulation
                .inspect_creatures()
                .iter()
                .any(|creature| creature.id != hero
                    && creature
                        .attack
                        .is_some_and(|order| order.target_id == hero
                            && order.status == AttackStatus::Attacking))
        );
        let second = simulation
            .states()
            .into_iter()
            .find(|state| state.id != first && simulation.can_hero_attack(state.id))
            .unwrap()
            .id;
        simulation.order_hero_attack(second).unwrap();
        for _ in 0..20 {
            simulation.tick();
        }
        assert_eq!(simulation.hero_attack_order().unwrap().target_id, second);
        let position = simulation
            .states()
            .into_iter()
            .find(|state| state.id == hero)
            .unwrap()
            .cell;
        let cell = Cell::new(position.0, position.1);
        let mut arrived = false;
        for direction in Direction::ALL {
            let Some(target) = cell.neighbour(direction) else {
                continue;
            };
            if simulation.order_hero_move(target).is_err() {
                continue;
            }
            for _ in 0..80 {
                simulation.tick();
            }
            if simulation.hero_order().unwrap().status == HeroOrderStatus::Arrived {
                arrived = true;
                break;
            }
        }
        assert!(arrived, "hero leaves through a reachable neighbouring cell");
        assert!(simulation.order_failures().is_empty());
    }
}

#[test]
#[ignore = "explicit release measurement on user-owned data"]
fn melee_tick_cost() {
    use std::{hint::black_box, time::Instant};
    for directory in editions() {
        let mut game = Game::open(&directory).unwrap();
        game.configure_new_game(0, 0, 0, 0, None).unwrap();
        let mut quiet = game
            .simulation_with_hero(game.start_map(0), 7, Some(0))
            .unwrap();
        let mut fighting = encounter(&game);
        let target = fighting.hero_attack_order().unwrap().target_id;
        let mut approach_seconds = 0.0;
        for _ in 0..100 {
            let mut simulation = game
                .simulation_with_hero(game.start_map(0), 7, Some(0))
                .unwrap();
            simulation.order_hero_attack(target).unwrap();
            let started = Instant::now();
            black_box(simulation.tick());
            approach_seconds += started.elapsed().as_secs_f64();
        }
        eprintln!(
            "first approach including A*: {:.2} us/tick (100 fresh routes)",
            approach_seconds * 10000.0
        );
        for (label, simulation) in [("quiet", &mut quiet), ("melee", &mut fighting)] {
            let started = Instant::now();
            for _ in 0..10000 {
                black_box(simulation.tick());
            }
            eprintln!(
                "{} {label}: {} creatures, {:.2} us/tick (10000 ticks)",
                directory
                    .parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_string_lossy(),
                simulation.creature_count(),
                started.elapsed().as_secs_f64() * 100.0
            );
        }
    }
    let mut candidates: Vec<_> = (0..50)
        .map(|id| k1_game::rules::combat::Combatant {
            id,
            group: 1,
            group_kind: 0x41,
            class_id: 0x43,
            state: 0,
            cell: (id as i16, 40),
            target: None,
        })
        .collect();
    candidates.push(k1_game::rules::combat::Combatant {
        id: 50,
        group: 0,
        group_kind: 0x20,
        class_id: 5,
        state: 0,
        cell: (30, 40),
        target: None,
    });
    let started = Instant::now();
    for _ in 0..10000 {
        for actor in &candidates[..50] {
            black_box(k1_game::rules::combat::monster_target(*actor, &candidates));
        }
    }
    eprintln!(
        "synthetic target decision (51 actors): {:.3} us/decision (500000 calls)",
        started.elapsed().as_secs_f64() * 2.0
    );
}
