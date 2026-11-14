//! Export locally owned game previews and compare allocation cost with state parsing.
use kn_game::{Game, newgame_allocation};
use std::{hint::black_box, path::PathBuf, time::Instant};
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = root.join("out/new-game");
    std::fs::create_dir_all(&output).unwrap();
    for edition in ["rus", "eng"] {
        let directory = root.join("original/k1").join(edition).join("game");
        let mut game = Game::open(&directory).unwrap();
        for hero in 0..3 {
            let menu = game.hero_preview(hero).unwrap();
            game.configure_new_game(hero, 1, 1, 2, None).unwrap();
            let session = game.configured_hero_preview(hero).unwrap();
            assert!(menu.rgba == session.rgba);
            let stem = output.join(format!("{edition}-{hero}"));
            std::fs::write(stem.with_extension("rgba"), menu.rgba).unwrap();
            std::fs::write(
                stem.with_extension("size"),
                format!("{} {}", menu.width, menu.height),
            )
            .unwrap();
        }
        let start = Instant::now();
        for _ in 0..100_000 {
            black_box(newgame_allocation(0, None).unwrap().adjusted(0, 2, -1));
        }
        let allocation_ns = start.elapsed().as_nanos() / 100_000;
        let bytes = std::fs::read(directory.join("Game.0")).unwrap();
        let start = Instant::now();
        for _ in 0..100 {
            black_box(k1_data::game_state::parse(&bytes).unwrap());
        }
        println!(
            "{edition}: allocation+adjustment {allocation_ns} ns; Game.0 parse {} ns",
            start.elapsed().as_nanos() / 100
        );
    }
}
