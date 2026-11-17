//! Real-hero orders on user-owned editions, without copied resource fixtures.
use std::path::PathBuf;

use k1_data::{
    game_state,
    repo::{GAME_FILES, GameDir},
};
use kn_game::{Game, MapSource, session::HeroOrderStatus};
use kn_isometric::{Cell, Direction};
use kn_movement::Event;

#[test]
fn every_starting_hero_accepts_exact_orders_while_the_world_keeps_running() {
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let mut editions = 0;
    for edition in ["rus", "eng"] {
        let root = original.join(edition).join("game");
        if !root.is_dir() {
            eprintln!("skip: user-owned {edition} edition absent");
            continue;
        }
        editions += 1;
        let mut game = Game::open(&root).unwrap();
        let directory = GameDir::new(&root);
        let mut other_steps = 0;
        for hero in game.heroes(MapSource::Game).unwrap() {
            game.configure_new_game(hero, 0, 0, 0, None).unwrap();
            let map = game.start_map(hero);
            let state =
                game_state::parse(&directory.state_bytes(hero, GAME_FILES).unwrap()).unwrap();
            let expected = game_state::on_map(&state, map.number as usize, Some(hero as usize))
                .start
                .unwrap()
                .members()
                .next()
                .unwrap();
            let mut simulation = game.simulation_with_hero(map, 42, Some(hero)).unwrap();
            assert_eq!(
                simulation.hero_id(),
                Some(expected),
                "{edition} hero {hero}"
            );
            let world_id = simulation
                .debug_snapshot()
                .creatures
                .into_iter()
                .find(|creature| creature.record_id == expected)
                .unwrap()
                .movement
                .unwrap()
                .world_id;
            let start = simulation
                .states()
                .into_iter()
                .find(|state| state.id == expected)
                .unwrap();
            let origin = Cell::new(start.cell.0, start.cell.1);
            let target = Direction::ALL
                .into_iter()
                .filter_map(|direction| origin.neighbour(direction))
                .find(|target| simulation.order_hero_move(*target).is_ok())
                .unwrap_or_else(|| panic!("{edition} hero {hero}: no reachable neighbour"));
            let mut route_seen = false;
            for _ in 0..1000 {
                for event in simulation.tick() {
                    match event {
                        Event::RouteSet { creature, .. } if creature.raw() == world_id => {
                            route_seen = true
                        }
                        Event::StepFinished { creature, .. } if creature.raw() != world_id => {
                            other_steps += 1
                        }
                        _ => {}
                    }
                }
            }
            assert!(route_seen, "{edition} hero {hero}: route events missing");
            assert_eq!(simulation.tick_number(), 1000);
            assert_eq!(simulation.hero_goal_reaches(), 1, "{edition} hero {hero}");
            assert_eq!(simulation.hero_order().unwrap().target, target);
            assert_eq!(
                simulation.hero_order().unwrap().status,
                HeroOrderStatus::Arrived
            );
            let current = simulation
                .states()
                .into_iter()
                .find(|state| state.id == expected)
                .unwrap();
            assert_eq!(current.cell, (target.column, target.row));
            assert!(simulation.order_failures().is_empty());
            game.clear_new_game();
        }
        assert!(
            other_steps > 0,
            "{edition}: no surrounding creature finished a step"
        );
    }
    assert!(
        !original.is_dir() || editions > 0,
        "original/k1 exists but has no supported edition"
    );
}
